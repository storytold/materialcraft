//! The 3D view's renderer: one ray per pixel (plus 2×2 supersampling when idle) against the
//! scene mesh's BVH, the material's maps sampled with tiling, then metallic/roughness GGX shading
//! under image-based light (SH diffuse, prefiltered specular, an analytic environment BRDF),
//! exposure and ACES tone mapping. The height output displaces real geometry ([`Scene::with_relief`]).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{Displacement, Light, Look, Relief};
use craft_texture::{Image, linear_to_srgb};
use materialcraft_geom::glam::{Vec2, Vec3};
use materialcraft_geom::view::{Background, ColorSpace, MeshInfo, Shader, View3d, ViewChannels};
use materialcraft_geom::{Bvh, Mesh, OrbitCamera};
use rayon::prelude::*;

use crate::ibl::{self, EnvImage};

mod output;
mod surface;

use output::Output;
use surface::Surface;

/// The material's maps (any may be missing: defaults are mid gray, flat, rough 0.5, dielectric).
#[derive(Clone, Default)]
pub struct Maps {
    pub base_color: Option<Image>,
    pub normal: Option<Image>,
    pub roughness: Option<Image>,
    pub metallic: Option<Image>,
    pub height: Option<Image>,
    pub ao: Option<Image>,
    pub emissive: Option<Image>,
    pub opacity: Option<Image>,
}

impl Maps {
    /// Maps from `(usage id, image)` pairs (`baseColor`, `normal`, …).
    pub fn from_usages<'a>(items: impl IntoIterator<Item = (&'a str, &'a Image)>) -> Maps {
        let mut m = Maps::default();
        for (usage, img) in items {
            use materialcraft_core::MaterialChannel;
            let Some(channel) = MaterialChannel::from_id(usage) else { continue };

            let slot = match channel {
                MaterialChannel::BaseColor => &mut m.base_color,
                MaterialChannel::Normal => &mut m.normal,
                MaterialChannel::Roughness => &mut m.roughness,
                MaterialChannel::Metallic => &mut m.metallic,
                MaterialChannel::Height => &mut m.height,
                MaterialChannel::AmbientOcclusion => &mut m.ao,
                MaterialChannel::Emissive => &mut m.emissive,
                MaterialChannel::Opacity => &mut m.opacity,
            };
            if slot.is_none() {
                *slot = Some(img.clone());
            }
        }
        m
    }
}

/// A mesh ready for ray casts.
pub struct Scene {
    pub mesh: Arc<Mesh>,
    pub bvh: Arc<Bvh>,
    pub radius: f32,
    /// Center of the mesh bounds (the camera orbits it).
    pub center: Vec3,
}

impl Scene {
    pub fn new(mut mesh: Mesh) -> Scene {
        if mesh.normals.len() != mesh.positions.len() {
            mesh.compute_normals();
        }
        if mesh.tangents.len() != mesh.positions.len() {
            mesh.compute_tangents();
        }
        let bvh = Bvh::new(&mesh);
        let (lo, hi) = mesh.bounds();
        let center = (lo + hi) * 0.5;
        let radius = ((hi - lo).length() * 0.5).max(1e-3);
        Scene { mesh: Arc::new(mesh), bvh: Arc::new(bvh), radius, center }
    }

    /// A built-in scene (`rounded_cube`, `sphere`, …); unknown ids give the rounded cube.
    pub fn builtin(id: &str) -> Scene {
        Scene::new(builtin_mesh(id))
    }

    /// `base` subdivided and moved along its normals by `depth × (height − level)`; vertices at one spot move together so it stays closed.
    pub fn displaced(base: &Mesh, height: &Image, displacement: &Displacement) -> Scene {
        let mut mesh = base.clone();
        if mesh.normals.len() != mesh.positions.len() {
            mesh.compute_normals();
        }

        while mesh.triangle_count() > 0 && mesh.triangle_count() * 4 <= displacement.max_triangles {
            mesh = mesh.subdivided();
        }

        // Pre-filtered so detail finer than the mesh (left to the normal map) does not spike the silhouette
        let height = height.resize(height.width().min(512), height.height().min(512), true);
        let level = if displacement.level.is_finite() { displacement.level.clamp(0.0, 1.0) } else { 0.0 };
        let depth = if displacement.depth.is_finite() { displacement.depth } else { 0.0 };
        let repeats = displacement.repeats.map(|repeats| if repeats.is_finite() { repeats.max(0.01) } else { 1.0 });

        // Vertices at the same spot (to 0.1 mm) are one point of the surface
        let spots: Vec<[i32; 3]> = mesh.positions.iter().map(|p| (*p * 1e4).round().as_ivec3().to_array()).collect();
        let mut spot_sums: HashMap<[i32; 3], (Vec3, f32, f32)> = HashMap::new();
        for ((spot, n), uv) in spots.iter().zip(&mesh.normals).zip(&mesh.uvs) {
            let (normal_sum, height_sum, count) = spot_sums.entry(*spot).or_default();
            *normal_sum += *n;
            *height_sum += height.sample_gray(uv.x * repeats[0], uv.y * repeats[1], true);
            *count += 1.0;
        }

        let offsets: Vec<Vec3> =
            spots.iter().map(|spot| spot_sums.get(spot).map_or(Vec3::ZERO, |(n, h, count)| n.normalize_or_zero() * depth * (h / count - level))).collect();

        // The normal accumulation needs a separate map, so release the height sums first
        drop(spot_sums);

        if displacement.shade_relief {
            // Normals of the displaced surface, shared per spot so seams don't show
            let moved = |i: u32| mesh.positions.get(i as usize).zip(offsets.get(i as usize)).map(|(p, o)| *p + *o);
            let mut face_sums: HashMap<[i32; 3], Vec3> = HashMap::new();
            for triangle in mesh.indices.as_chunks::<3>().0 {
                let (Some(a), Some(b), Some(c)) = (moved(triangle[0]), moved(triangle[1]), moved(triangle[2])) else { continue };
                let face = (b - a).cross(c - a);
                for spot in triangle.iter().filter_map(|&i| spots.get(i as usize)) {
                    *face_sums.entry(*spot).or_default() += face;
                }
            }

            for (spot, n) in spots.iter().zip(mesh.normals.iter_mut()) {
                if let Some(sum) = face_sums.get(spot).and_then(|s| s.try_normalize()) {
                    *n = sum;
                }
            }

            mesh.tangents.clear();
        }

        for (p, offset) in mesh.positions.iter_mut().zip(&offsets) {
            *p += *offset;
        }

        // Subdivision scratch must not overlap tangent and BVH construction
        drop(spots);
        drop(offsets);
        Scene::new(mesh)
    }

    /// The scene the 3D view shows for `maps`: displaced by the height output repeated `repeats` times along U and V, or `None` when nothing moves.
    pub fn with_relief(&self, maps: &Maps, relief: Relief, repeats: [f32; 2], max_triangles: usize) -> Option<Scene> {
        let height = maps.height.as_ref()?;
        if !(relief.depth.is_finite() && relief.depth > 0.0) {
            return None;
        }

        // Without a normal map the displaced surface's own normals show the relief
        let displacement = Displacement { depth: relief.depth * self.radius, level: relief.level, repeats, max_triangles, shade_relief: maps.normal.is_none() };
        Some(Scene::displaced(&self.mesh, height, &displacement))
    }
}

/// Triangle budget of a displaced scene in the interactive 3D view (builds in about 0.2–0.3 s on two cores).
pub const VIEW_TRIANGLES: usize = 500_000;

/// A built-in mesh by id; unknown ids give the rounded cube.
pub fn builtin_mesh(id: &str) -> Mesh {
    let mut m = materialcraft_geom::primitives::builtin(id).unwrap_or_else(|| materialcraft_geom::primitives::rounded_box(Vec3::ONE, 0.15));
    if id == "sphere" {
        // The texture wraps twice around, so texels are square at the equator
        m.uvs.iter_mut().for_each(|uv| uv.x *= 2.0);
    }

    m
}

/// Prefiltered environment lighting.
pub struct Lighting {
    pub name: String,
    pub specular: Vec<EnvImage>,
    pub sh: [[f32; 3]; 9],
}

impl Lighting {
    pub fn new(id: &str) -> Lighting {
        let env = ibl::environment(ibl::by_id(id), 256);
        Lighting { name: id.into(), specular: ibl::prefilter(&env, 6), sh: ibl::sh9(&env) }
    }

    fn diffuse(&self, n: Vec3) -> Vec3 {
        let b = ibl::sh_basis(n);
        let mut c = Vec3::ZERO;
        for k in 0..9 {
            c += Vec3::from(self.sh[k]) * b[k];
        }
        c.max(Vec3::ZERO)
    }

    fn specular(&self, r: Vec3, rough: f32) -> Vec3 {
        let levels = self.specular.len();
        if levels == 0 {
            return Vec3::ZERO;
        }

        let l = rough.clamp(0.0, 1.0) * (levels - 1) as f32;
        let i = (l.floor() as usize).min(levels - 1);
        let j = (i + 1).min(levels - 1);
        let (a, b) = (self.specular[i].sample(r), self.specular[j].sample(r));
        a.lerp(b, l - i as f32)
    }
}

/// Camera and look.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Degrees.
    pub yaw: f32,
    pub pitch: f32,
    /// In scene radii.
    pub distance: f32,
    /// Orbit target offset from the scene center, in scene radii (the caller bounds it).
    pub target: [f32; 3],
    pub tiling: f32,
    /// Further UV repeats along U and V
    pub uv_scale: [f32; 2],
    /// Stops.
    pub exposure: f32,
    /// Degrees.
    pub env_rotation: f32,
    /// Interpret the green channel as DirectX (-Y) instead of OpenGL (+Y).
    pub normal_y_flip: bool,
    /// 1 = one ray per pixel, 2 = 2×2 supersampling.
    pub supersample: u32,
    /// Ray-traced ambient occlusion rays per sample (0 = off): darkens the crevices of a
    /// displaced scene.
    pub ao_rays: u32,
    /// Strength of a shadow-casting key light from the camera's upper left (0 = off).
    pub key_light: f32,
    /// See through the surface where the material's alpha (the opacity output, else the base
    /// color's alpha channel) is below one: what lies behind, back faces included, shows through.
    pub transparency: bool,
    /// Show the material's alpha channel on the mesh, unlit (white = opaque, black = clear).
    pub alpha_view: bool,
    /// Vertical field of view, degrees
    pub fov: f32,
    /// Parallel projection instead of perspective
    pub ortho: bool,
    /// Near and far clip distances from the camera, in mesh units; `None` fits them to the scene
    pub clip: Option<[f32; 2]>,
    /// What shows behind the mesh
    pub background: Background,
    /// Linear display color of a uniform background
    pub background_color: [f32; 3],
    /// The environment lights the scene
    pub env_light: bool,
    /// Linear RGB of light that reaches every surface
    pub ambient: [f32; 3],
    pub lights: [Option<Light>; 2],
    /// The point lights cast shadows
    pub shadows: bool,
    /// Faces that point away from the camera are not drawn
    pub backface_culling: bool,
    /// Draw the edges of the triangles only
    pub wireframe: bool,
    pub channels: ViewChannels,
    pub color_space: ColorSpace,
    pub look: Look,
    /// Depth of the parallax relief in scene radii and the height that stays on the surface
    pub relief: Relief,
}

impl Default for Settings {
    fn default() -> Self {
        Settings::for_view(&View3d::default())
    }
}

/// The orbit camera for `s` around `scene`.
pub fn camera(scene: &Scene, s: &Settings) -> OrbitCamera {
    let target = scene.center + Vec3::from(s.target) * scene.radius;
    OrbitCamera {
        target: if target.is_finite() { target } else { scene.center },
        yaw: s.yaw.to_radians(),
        pitch: s.pitch.clamp(-89.0, 89.0).to_radians(),
        distance: s.distance.clamp(1.05, 50.0) * scene.radius,
        fov: if s.fov.is_finite() { s.fov.clamp(5.0, 150.0) } else { 35.0 },
        scene_radius: scene.radius,
        ortho: s.ortho,
    }
}

/// `s.target` after panning the view by (dx, dy) viewport heights: the scene follows the pointer.
/// Unbounded; non-finite parts become 0.
pub fn pan(scene: &Scene, s: &Settings, dx: f32, dy: f32) -> [f32; 3] {
    let mut cam = camera(scene, s);
    cam.pan(dx, dy);
    let t = (cam.target - scene.center) / scene.radius.max(f32::EPSILON);
    t.to_array().map(|v| if v.is_finite() { v } else { 0.0 })
}

#[inline]
fn tex(img: &Option<Image>, uv: Vec2) -> Option<[f32; 4]> {
    img.as_ref().map(|i| i.sample(uv.x, uv.y, true))
}

#[inline]
fn tex_gray(img: &Option<Image>, uv: Vec2, default: f32) -> f32 {
    img.as_ref().map_or(default, |i| i.sample_gray(uv.x, uv.y, true))
}

/// Narkowicz's fit of the ACES filmic curve.
#[inline]
fn aces(x: f32) -> f32 {
    ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
}

/// Scene light as linear display light
fn tone_map(space: ColorSpace, value: f32) -> f32 {
    match space {
        ColorSpace::Aces => aces(value),
        ColorSpace::Srgb => value.clamp(0.0, 1.0),
    }
}

/// The scene light that [`tone_map`] turns into `display`, so a chosen background color shows as chosen
fn inverse_tone_map(space: ColorSpace, display: f32) -> f32 {
    let display = if display.is_finite() { display.clamp(0.0, 1.0) } else { 0.0 };
    if space == ColorSpace::Srgb {
        return display;
    }

    // The curve rises all the way, so halving the range finds the light
    let (mut low, mut high) = (0.0f32, BRIGHTEST_BACKGROUND);
    for _ in 0..INVERSE_STEPS {
        let middle = (low + high) * 0.5;
        if aces(middle) < display {
            low = middle;
        } else {
            high = middle;
        }
    }
    (low + high) * 0.5
}

/// Scene light that the filmic curve shows as white
const BRIGHTEST_BACKGROUND: f32 = 16.0;
const INVERSE_STEPS: usize = 28;

/// Karis' analytic approximation of the split-sum environment BRDF: (scale, bias) for F0.
#[inline]
fn env_brdf(roughness: f32, ndv: f32) -> (f32, f32) {
    let c0 = [-1.0, -0.0275, -0.572, 0.022];
    let c1 = [1.0, 0.0425, 1.04, -0.04];
    let r = [roughness * c0[0] + c1[0], roughness * c0[1] + c1[1], roughness * c0[2] + c1[2], roughness * c0[3] + c1[3]];
    let a004 = (r[0] * r[0]).min((-9.28 * ndv).exp2()) * r[0] + r[1];
    (a004 * -1.04 + r[2], a004 * 1.04 + r[3])
}

fn rotate_y(v: Vec3, deg: f32) -> Vec3 {
    let (s, c) = deg.to_radians().sin_cos();
    Vec3::new(v.x * c + v.z * s, v.y, -v.x * s + v.z * c)
}

/// The material's alpha at `uv`: the opacity output, else the base color's alpha channel, else
/// the model's uniform opacity.
fn alpha(maps: &Maps, look: &Look, uv: Vec2) -> f32 {
    let a = match (&maps.opacity, &maps.base_color) {
        (Some(o), _) if look.opacity_map => o.sample_gray(uv.x, uv.y, true),
        (_, Some(b)) if look.base_color_map && !b.is_gray() => b.sample(uv.x, uv.y, true)[3] * look.opacity,
        _ => look.opacity,
    };
    if a.is_finite() { a.clamp(0.0, 1.0) } else { 1.0 }
}

/// Surfaces that show (even faintly) along a ray before it gives up; what is left shows the background
const MAX_LAYERS: usize = 8;
/// Culled faces and the open insides of wireframe triangles a ray may pass before it gives up
const MAX_PASSES: usize = 96;
/// Steps of the parallax march through the height field
const PARALLAX_STEPS: usize = 24;
/// Half the width of a wireframe line, in pixels
const WIRE_HALF_WIDTH: f32 = 0.75;
/// Wavelengths of red, green and blue light in micrometers, for thin-film interference
const WAVELENGTHS: [f32; 3] = [0.65, 0.53, 0.46];
/// How much of the light a subsurface layer lets through from behind
const BACK_SCATTER: f32 = 0.35;
/// The share of the base's light a full coat with full darkening keeps
const DARKEST_COAT: f32 = 0.82;
/// Blinn highlight exponent of a fully glossy surface
const MAX_SHININESS: f32 = 256.0;

/// Per-pixel inputs of [`shade`] beyond the camera ray.
struct Px {
    /// Direction toward the key light.
    key: Vec3,
    /// Random rotation of the ambient occlusion ray pattern.
    jitter: Vec2,
    /// Width of a pixel at the ray's start and how much it grows per unit of distance
    footprint: [f32; 2],
    /// Original camera-ray origin before clipping or transparency passes
    ray_start: Vec3,
}

/// The fraction of `n`'s hemisphere at `p` that is open within `reach` (cosine-weighted).
fn ambient_occlusion(scene: &Scene, p: Vec3, n: Vec3, rays: u32, jitter: Vec2) -> f32 {
    let (t, b) = n.any_orthonormal_pair();
    let reach = scene.radius * 0.3;
    let open = (0..rays)
        .filter(|&i| {
            let u = (ibl::hammersley(i, rays) + jitter).fract();
            let (r, phi) = (u.x.sqrt(), u.y * std::f32::consts::TAU);
            let d = t * (r * phi.cos()) + b * (r * phi.sin()) + n * (1.0 - u.x).max(0.0).sqrt();
            !scene.bvh.occluded(p, d, reach)
        })
        .count();
    open as f32 / rays as f32
}

/// GGX specular plus Lambert diffuse from a white directional light along `l`.
fn direct(n: Vec3, v: Vec3, l: Vec3, diffuse: Vec3, f0: Vec3, rough: f32) -> Vec3 {
    let ndl = n.dot(l);
    if ndl <= 0.0 {
        return Vec3::ZERO;
    }

    let ndv = n.dot(v).max(1e-4);
    let h = (v + l).normalize_or_zero();
    let (ndh, vdh) = (n.dot(h).max(0.0), v.dot(h).max(0.0));
    let a2 = (rough * rough).powi(2).max(1e-4);
    let d = a2 / (std::f32::consts::PI * (ndh * ndh * (a2 - 1.0) + 1.0).powi(2));
    let k = (rough + 1.0).powi(2) / 8.0;
    let g = ndl / (ndl * (1.0 - k) + k) * ndv / (ndv * (1.0 - k) + k);
    let f = f0 + (Vec3::ONE - f0) * (1.0 - vdh).powi(5);
    let spec = f * (d * g / (4.0 * ndv * ndl));
    let diff = (Vec3::ONE - f) * diffuse / std::f32::consts::PI;
    (diff + spec) * ndl
}

/// Reflectance of a surface seen straight on, from its index of refraction
fn reflectance(ior: f32) -> f32 {
    let ior = ior.max(1.0);
    ((ior - 1.0) / (ior + 1.0)).powi(2)
}

/// The tint a thin film gives reflections: light that travels through the film and back meets
/// the light reflected off its top, in or out of step depending on its wavelength
fn thin_film(ndv: f32, thickness: f32, ior: f32) -> Vec3 {
    let ior = ior.max(1.0);
    let inside = (1.0 - (1.0 - ndv * ndv) / (ior * ior)).max(0.0).sqrt();
    let path = 2.0 * ior * thickness.max(0.0) * inside;
    Vec3::from(WAVELENGTHS.map(|wavelength| 0.5 + 0.5 * (std::f32::consts::TAU * path / wavelength).cos()))
}

/// What one surface along a ray adds to the pixel
struct Shaded {
    /// Light toward the camera (or a display value, see [`Output`])
    color: Vec3,
    /// How much of `color` shows
    opacity: f32,
    /// The share of what lies behind that shows through, per channel
    pass: Vec3,
    t: f32,
}

impl Shaded {
    /// A surface that is not drawn: everything behind it shows
    fn open(t: f32) -> Shaded {
        Shaded { color: Vec3::ZERO, opacity: 0.0, pass: Vec3::ONE, t }
    }

    fn solid(color: Vec3, t: f32) -> Shaded {
        Shaded { color, opacity: 1.0, pass: Vec3::ZERO, t }
    }
}

/// Light arriving at a surface from the point lights and the key light, as (direction, color) pairs
fn arriving(scene: &Scene, s: &Settings, px: &Px, surface: &Surface) -> impl Iterator<Item = (Vec3, Vec3)> {
    let point_lights = s.lights.into_iter().flatten().filter_map(move |light| {
        let position = scene.center + Vec3::from(light.position) * scene.radius;
        let to_light = position - surface.p;
        let distance = to_light.length();
        let direction = to_light.try_normalize()?;

        let lit = surface.face.dot(direction) > 0.0 && !(s.shadows && scene.bvh.occluded(surface.p, direction, distance));
        lit.then_some((direction, Vec3::from(light.color)))
    });
    let key_light =
        (s.key_light > 0.0 && surface.face.dot(px.key) > 0.0 && !scene.bvh.occluded(surface.p, px.key, f32::MAX)).then_some((px.key, Vec3::splat(s.key_light)));

    point_lights.chain(key_light).collect::<Vec<_>>().into_iter()
}

/// The physically based models: metal and dielectric base, with the coat, fuzz, subsurface,
/// thin film and transmission of the models that have them (their weights are 0 elsewhere).
/// Returns the light toward the viewer and the share of what lies behind that passes through
fn physical(scene: &Scene, light: &Lighting, s: &Settings, px: &Px, surface: &Surface) -> (Vec3, Vec3) {
    let look = &s.look;
    let Surface { n, v, base, rough, metal, ao, .. } = *surface;
    let ndv = n.dot(v).clamp(1e-4, 1.0);
    let rotated = |direction: Vec3| rotate_y(direction, -s.env_rotation);
    let environment = if s.env_light { 1.0 } else { 0.0 };

    // The glossy models carry their own specular color; the others derive it from the metalness
    let glossy = matches!(look.shader, Shader::SpecularGlossiness);
    let dielectric = Vec3::splat(reflectance(look.ior)) * look.specular_color * look.specular_weight;
    let mut f0 = if glossy { look.glossy_specular } else { dielectric.lerp(base * look.specular_weight, metal) };
    if look.thin_film_weight > 0.0 {
        f0 *= Vec3::ONE.lerp(thin_film(ndv, look.thin_film_thickness, look.thin_film_ior) * 2.0, look.thin_film_weight);
    }

    let transmitted = look.transmission_weight * (1.0 - metal);
    let scattering = base.lerp(look.subsurface_color, look.subsurface_weight);

    // A rough diffuse surface is flatter: dimmer straight on, brighter toward its edges
    let flattening = 1.0 + look.diffuse_roughness * (0.5 * (1.0 - ndv) - 0.25);
    let diffuse = scattering * (1.0 - metal) * (1.0 - transmitted) * flattening * if glossy { 1.0 - f0.max_element() } else { 1.0 };

    // Stretched highlights: the reflection bends around the direction of the grain
    let bent = if look.anisotropy > 0.0 {
        let across = surface.bitangent.cross(v).cross(surface.bitangent);
        n.lerp(across, look.anisotropy * (rough * 5.0).min(1.0)).try_normalize().unwrap_or(n)
    } else {
        n
    };
    let reflected = (bent * (2.0 * bent.dot(v)) - v).normalize_or_zero();
    let (scale, bias) = env_brdf(rough, ndv);
    let specular = light.specular(rotated(reflected), rough) * (f0 * scale + Vec3::splat(bias * if glossy { 1.0 } else { look.specular_weight.max(metal) }));
    let irradiance = light.diffuse(rotated(n));

    let mut color = (irradiance * diffuse + specular) * environment * ao + Vec3::from(s.ambient) * diffuse * ao;
    if look.subsurface_weight > 0.0 {
        color += light.diffuse(rotated(-n))
            * look.subsurface_color
            * look.subsurface_tint
            * (look.subsurface_weight * BACK_SCATTER * (1.0 - metal) * environment * ao);
    }
    if look.fuzz_weight > 0.0 {
        // Fibers catch light toward the edges; rougher fuzz spreads further in
        let edge = (1.0 - ndv).powf(1.0 + 4.0 * (1.0 - look.fuzz_roughness));
        color += irradiance * look.fuzz_color * (look.fuzz_weight * edge * environment * ao);
    }
    for (direction, arriving) in arriving(scene, s, px, surface) {
        color += direct(n, v, direction, diffuse, f0, rough) * arriving;
    }

    if look.coat_weight > 0.0 {
        let coat_f0 = reflectance(look.coat_ior);
        let fresnel = (coat_f0 + (1.0 - coat_f0) * (1.0 - ndv).powi(5)) * look.coat_weight;
        let coat_rough = look.coat_roughness.clamp(0.02, 1.0);
        let mirror = (surface.face * (2.0 * surface.face.dot(v)) - v).normalize_or_zero();

        // Under the coat: tinted by it, darkened by light caught inside it and dimmed by what it reflects
        color *= Vec3::ONE.lerp(look.coat_color, look.coat_weight) * (1.0 - fresnel) * (1.0 - (1.0 - DARKEST_COAT) * look.coat_weight * look.coat_darkening);
        color += light.specular(rotated(mirror), coat_rough) * (fresnel * environment * ao);
        for (direction, arriving) in arriving(scene, s, px, surface) {
            color += direct(surface.face, v, direction, Vec3::ZERO, Vec3::splat(coat_f0), coat_rough) * arriving * look.coat_weight;
        }
    }

    (color, look.transmission_color * transmitted)
}

/// Blinn: diffuse light plus a highlight whose tightness is the glossiness. Lambert is its diffuse part
fn classic(scene: &Scene, light: &Lighting, s: &Settings, px: &Px, surface: &Surface) -> Vec3 {
    let look = &s.look;
    let Surface { n, v, base, ao, .. } = *surface;
    let highlights = look.shader == Shader::Blinn;
    let environment = if s.env_light { 1.0 } else { 0.0 };
    let shininess = 2.0 + look.glossiness.powi(2) * MAX_SHININESS;

    let mut color = (light.diffuse(rotate_y(n, -s.env_rotation)) * environment + Vec3::from(s.ambient)) * base * ao;
    if highlights {
        let reflected = (n * (2.0 * n.dot(v)) - v).normalize_or_zero();
        color += light.specular(rotate_y(reflected, -s.env_rotation), 1.0 - look.glossiness) * look.glossy_specular * (environment * ao);
    }
    for (direction, arriving) in arriving(scene, s, px, surface) {
        let facing = n.dot(direction).max(0.0);
        color += base * arriving * (facing / std::f32::consts::PI);
        if highlights {
            let half = (v + direction).normalize_or_zero();
            color += look.glossy_specular * arriving * (n.dot(half).max(0.0).powf(shininess) * facing);
        }
    }
    color
}

/// A color per UV tile, so neighbouring tiles differ
fn tile_color(tile: Vec2) -> Vec3 {
    let hash = jitter(tile.x as i32 as u32, tile.y as i32 as u32);
    Vec3::new(0.25 + 0.75 * hash.x, 0.25 + 0.75 * hash.y, 0.25 + 0.75 * (1.0 - hash.x * hash.y))
}

/// The texture coordinate where the view ray meets the height field under `uv`: the march steps
/// down from the top of the relief until it is below the surface
fn parallax(height: &Image, uv: Vec2, view_in_tangent: Vec3, depth: Vec2, level: f32) -> Vec2 {
    if depth.max_element() <= 0.0 || view_in_tangent.z <= 1e-3 {
        return uv;
    }

    // The relief spans `depth` in units where the texture is one across the mesh
    let across = Vec2::new(-view_in_tangent.x, view_in_tangent.y) / view_in_tangent.z * depth;
    let top = 1.0 - level.clamp(0.0, 1.0);
    let mut shifted = uv - across * top;
    for step in 0..PARALLAX_STEPS {
        let ray_height = 1.0 - step as f32 / PARALLAX_STEPS as f32;
        if height.sample_gray(shifted.x, shifted.y, true) >= ray_height {
            break;
        }
        shifted += across / PARALLAX_STEPS as f32;
    }
    shifted
}

/// Shade the first surface along one ray within `reach`; `None` = missed the mesh.
#[allow(clippy::too_many_arguments)]
fn shade(scene: &Scene, maps: &Maps, light: &Lighting, s: &Settings, origin: Vec3, dir: Vec3, reach: f32, px: &Px) -> Option<Shaded> {
    let hit = scene.bvh.closest(origin, dir, reach)?;
    let m = &scene.mesh;
    let look = &s.look;
    let [i0, i1, i2] = m.tri(hit.tri as usize)?;
    let (w1, w2) = (hit.u, hit.v);
    let w0 = 1.0 - w1 - w2;
    let lerp3 = |a: Vec3, b: Vec3, c: Vec3| a * w0 + b * w1 + c * w2;
    let (p0, p1, p2) = (*m.positions.get(i0)?, *m.positions.get(i1)?, *m.positions.get(i2)?);
    let outward = (p1 - p0).cross(p2 - p0);

    if s.backface_culling && outward.dot(dir) > 0.0 {
        return Some(Shaded::open(hit.t));
    }

    if s.wireframe {
        // Distance to the nearest edge: each weight times the height of the triangle over that edge
        let doubled_area = outward.length();
        let heights =
            [doubled_area / (p2 - p1).length().max(1e-12), doubled_area / (p0 - p2).length().max(1e-12), doubled_area / (p1 - p0).length().max(1e-12)];
        let to_edge = (w0 * heights[0]).min(w1 * heights[1]).min(w2 * heights[2]);
        let camera_distance = hit.t + (origin - px.ray_start).dot(dir);
        if to_edge > WIRE_HALF_WIDTH * (px.footprint[0] + px.footprint[1] * camera_distance) {
            return Some(Shaded::open(hit.t));
        }
    }

    let nrm = lerp3(*m.normals.get(i0)?, *m.normals.get(i1)?, *m.normals.get(i2)?).normalize_or_zero();

    // A back face (seen through a transparent front, or under an open mesh) faces the viewer
    let nrm = if nrm.dot(dir) > 0.0 { -nrm } else { nrm };
    let (t0, t1, t2) = (*m.tangents.get(i0)?, *m.tangents.get(i1)?, *m.tangents.get(i2)?);
    let tan = (t0.truncate() * w0 + t1.truncate() * w1 + t2.truncate() * w2).normalize_or_zero();
    let sign = if t0.w < 0.0 { -1.0 } else { 1.0 };
    let tan = (tan - nrm * nrm.dot(tan)).normalize_or_zero();
    let bit = nrm.cross(tan) * sign;
    let uv0 = *m.uvs.get(i0)? * w0 + *m.uvs.get(i1)? * w1 + *m.uvs.get(i2)? * w2;
    let v = -dir;
    let mut uv = uv0 * s.tiling * Vec2::from(s.uv_scale);
    if look.parallax
        && let Some(height) = &maps.height
    {
        uv = parallax(height, uv, Vec3::new(v.dot(tan), v.dot(bit), v.dot(nrm)), Vec2::from(s.uv_scale) * (s.relief.depth * s.tiling * 0.5), s.relief.level);
    }

    let a = alpha(maps, look, uv);
    if s.alpha_view {
        return Some(Shaded::solid(Vec3::splat(a), hit.t));
    }

    if look.shader == Shader::MeshInfo {
        let info = match look.mesh_info {
            MeshInfo::Normal => nrm * 0.5 + 0.5,
            MeshInfo::Tangent => tan * 0.5 + 0.5,
            MeshInfo::Binormal => bit * 0.5 + 0.5,
            MeshInfo::Uv => Vec3::new(uv0.x.rem_euclid(1.0), uv0.y.rem_euclid(1.0), 0.0),
            MeshInfo::UvTile => tile_color(uv0.floor()),
            MeshInfo::VertexColor => match (m.colors.get(i0), m.colors.get(i1), m.colors.get(i2)) {
                (Some(c0), Some(c1), Some(c2)) => lerp3(c0.truncate(), c1.truncate(), c2.truncate()),
                _ => Vec3::ONE,
            },
            MeshInfo::Position => (origin + dir * hit.t - scene.center) / (scene.radius * 2.0) + 0.5,
        };
        return Some(Shaded::solid(info, hit.t));
    }

    let opacity = if s.transparency { a } else { 1.0 };
    let base_map = if look.base_color_map { tex(&maps.base_color, uv) } else { None };
    let base = base_map.map_or(look.base_color, |c| Vec3::new(c[0], c[1], c[2])) * look.base_weight;
    let emissive_map = if look.emission_map { tex(&maps.emissive, uv) } else { None };
    let emission = match emissive_map {
        // An emissive output glows as it is until a luminance is set
        Some(e) => Vec3::new(e[0], e[1], e[2]) * if look.emission_luminance > 0.0 { look.emission_luminance } else { 1.0 },
        None => look.emission_color * look.emission_luminance,
    };
    if look.shader == Shader::Unlit {
        return Some(Shaded { color: base + emission, opacity, pass: Vec3::splat(1.0 - opacity), t: hit.t });
    }

    let glossy = matches!(look.shader, Shader::SpecularGlossiness | Shader::Blinn);
    let uniform_rough = if glossy { 1.0 - look.glossiness } else { look.roughness };
    let rough = if look.roughness_map { tex_gray(&maps.roughness, uv, uniform_rough) } else { uniform_rough }.clamp(0.02, 1.0);
    let metal = if look.metalness_map { tex_gray(&maps.metallic, uv, look.metalness) } else { look.metalness }.clamp(0.0, 1.0);
    let ao = if look.ambient_occlusion_map { tex_gray(&maps.ao, uv, look.ambient_occlusion) } else { look.ambient_occlusion }.clamp(0.0, 1.0);
    let normal_map = if look.normal_map { tex(&maps.normal, uv) } else { None };
    let stored = normal_map.map_or(look.normal, |c| Vec3::new(c[0], c[1], c[2]));
    let n = if normal_map.is_some() || (stored - Vec3::new(0.5, 0.5, 1.0)).length_squared() > 1e-6 {
        let mut t = stored * 2.0 - 1.0;
        if s.normal_y_flip {
            t.y = -t.y;
        }
        // Top-left UVs make OpenGL's +v green point against the bitangent
        (tan * t.x - bit * t.y + nrm * t.z.max(0.05)).normalize_or_zero()
    } else {
        nrm
    };
    let n = if n.dot(v) < 0.0 { nrm } else { n };

    // The true surface (a displaced scene's facets), offset to avoid self-hits
    let face = outward.normalize_or_zero();
    let face = if face.dot(nrm) < 0.0 { -face } else { face };
    let p = origin + dir * hit.t + face * scene.radius * 2e-4;
    let surface = Surface { n, v, bitangent: bit, p, face, base, rough, metal, ao };

    let (mut color, transmitted) = match look.shader {
        Shader::Blinn | Shader::Lambert => (classic(scene, light, s, px, &surface), Vec3::ZERO),
        _ => physical(scene, light, s, px, &surface),
    };
    if s.ao_rays > 0 {
        color *= ambient_occlusion(scene, p, face, s.ao_rays, px.jitter);
    }
    color += emission;

    Some(Shaded { color, opacity, pass: Vec3::splat(1.0 - opacity) + transmitted * opacity, t: hit.t })
}

/// Follow one camera ray within `reach` through the surfaces that let light pass, compositing
/// front to back (`miss` is what shows behind the mesh). Shaded colors are scaled by the exposure `k`.
#[allow(clippy::too_many_arguments)]
fn trace(scene: &Scene, maps: &Maps, light: &Lighting, s: &Settings, origin: Vec3, dir: Vec3, reach: f32, px: &Px, k: f32, miss: Vec3) -> Vec3 {
    let mut acc = Vec3::ZERO;
    let mut through = Vec3::ONE;
    let mut o = origin;
    let mut reach = reach;
    let mut layers = 0;
    let step = scene.radius * 1e-4;

    for _ in 0..MAX_PASSES {
        let Some(h) = shade(scene, maps, light, s, o, dir, reach, px) else { return acc + miss * through };

        acc += h.color * k * h.opacity * through;
        through *= h.pass;
        if through.max_element() < 1e-3 {
            return acc;
        }

        if h.opacity > 0.0 {
            layers += 1;
            if layers >= MAX_LAYERS {
                break;
            }
        }
        o += dir * (h.t + step);
        reach -= h.t + step;
    }

    acc + miss * through
}

/// A cheap integer hash to 0..1 pairs (the per-sample AO pattern rotation).
fn jitter(x: u32, y: u32) -> Vec2 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    Vec2::new((h & 0xFFFF) as f32 / 65536.0, (h >> 16) as f32 / 65536.0)
}

/// Where a camera ray starts and how far it reaches: the automatic near plane, or the view's own clip planes
fn clipped(cam: &OrbitCamera, ray: &materialcraft_geom::Ray, clip: Option<[f32; 2]>) -> (Vec3, f32) {
    let Some([near, far]) = clip else { return (ray.origin, f32::MAX) };
    let near = if near.is_finite() { near.max(0.0) } else { 0.0 };
    let far = if far.is_finite() { far.max(near) } else { f32::MAX };
    let forward = cam.forward();

    if cam.ortho {
        let depth = (ray.origin - cam.eye()).dot(forward);
        return (ray.origin + forward * (near - depth), far - near);
    }

    let along = ray.dir.dot(forward).max(1e-4);
    (cam.eye() + ray.dir * (near / along), (far - near) / along)
}

/// Scene light of the studio backdrop in the middle of the frame and at its corners
const BACKDROP_CENTER: f32 = 0.04;
const BACKDROP_EDGE: f32 = 0.012;
/// How much of the way to the frame's edge the backdrop takes to darken fully
const BACKDROP_FALLOFF: f32 = 0.75;

/// The studio backdrop at screen position `at` (−1..1): gray, darkening toward the corners like a vignette
fn backdrop(at: Vec2) -> Vec3 {
    let toward_edge = (at.length() * BACKDROP_FALLOFF).clamp(0.0, 1.0);
    let eased = toward_edge * toward_edge * (3.0 - 2.0 * toward_edge);
    Vec3::splat(BACKDROP_CENTER).lerp(Vec3::splat(BACKDROP_EDGE), eased)
}

/// Render `w × h` pixels to sRGB RGBA8.
pub fn render(scene: &Scene, maps: &Maps, light: &Lighting, s: &Settings, w: u32, h: u32) -> Vec<u8> {
    let (w, h) = (w.clamp(1, 4096), h.clamp(1, 4096));
    render_cancellable(scene, maps, light, s, w, h, &AtomicBool::new(false)).unwrap_or_else(|| vec![0; (w * h * 4) as usize])
}

/// [`render`] that gives up once `cancel` is set (rows not yet started are skipped) and then
/// returns `None`: a frame the viewer no longer needs (the camera moved, the view was resized)
/// stops holding the CPU.
pub fn render_cancellable(scene: &Scene, maps: &Maps, light: &Lighting, s: &Settings, w: u32, h: u32, cancel: &AtomicBool) -> Option<Vec<u8>> {
    let (w, h) = (w.clamp(1, 4096), h.clamp(1, 4096));
    let cam = camera(scene, s);
    let aspect = w as f32 / h as f32;
    let ss = s.supersample.clamp(1, 4);
    let output = Output::of(s);
    let k = if output == Output::Lit { 2f32.powf(s.exposure) } else { 1.0 };
    let environment = light.specular.first();

    // Key light from the camera's upper left, a little in front of the subject
    let fwd = cam.forward();
    let right = fwd.cross(Vec3::Y).normalize_or(Vec3::X);
    let key = (right * -0.6 + right.cross(fwd) * 0.75 - fwd * 0.5).normalize_or_zero();

    // The width of a pixel in the scene: fixed in a parallel projection, growing with distance otherwise
    let half_height = (cam.fov.to_radians() * 0.5).tan();
    let footprint = if cam.ortho { [2.0 * cam.distance * half_height / h as f32, 0.0] } else { [0.0, 2.0 * half_height / h as f32] };

    // A uniform background shows as chosen: as scene light it is what the tone curve maps to it
    let uniform = match output {
        Output::Lit => Vec3::from(s.background_color.map(|channel| inverse_tone_map(s.color_space, channel))),
        Output::Color => Vec3::from(s.background_color),
        Output::Data => Vec3::from(s.background_color.map(linear_to_srgb)),
    };

    // Scene light behind the mesh as this output holds it: the lit picture is tone mapped later, the others here
    let behind = |light: Vec3| match output {
        Output::Lit => light,
        Output::Color => light.map(|v| tone_map(s.color_space, v)),
        Output::Data => light.map(|v| linear_to_srgb(tone_map(s.color_space, v))),
    };
    let exposure = 2f32.powf(s.exposure);

    let mut out = vec![0u8; (w * h * 4) as usize];
    out.par_chunks_mut(w as usize * 4).enumerate().for_each(|(y, row)| {
        if cancel.load(Ordering::Relaxed) {
            return;
        }

        for x in 0..w as usize {
            let mut acc = Vec3::ZERO;
            for sy in 0..ss {
                for sx in 0..ss {
                    let px = (x as f32 + (sx as f32 + 0.5) / ss as f32) / w as f32 * 2.0 - 1.0;
                    let py = 1.0 - (y as f32 + (sy as f32 + 0.5) / ss as f32) / h as f32 * 2.0;
                    let ray = cam.ray(Vec2::new(px, py), aspect);
                    let (origin, reach) = clipped(&cam, &ray, s.clip);
                    let p = Px { key, jitter: jitter(x as u32 * ss + sx, y as u32 * ss + sy), footprint, ray_start: ray.origin };
                    let miss = match (s.background, environment) {
                        (Background::Studio, _) => behind(backdrop(Vec2::new(px, py))),
                        (Background::Color, _) => uniform,
                        (Background::Environment, Some(e)) if s.env_light => behind(e.sample(rotate_y(ray.dir, -s.env_rotation)) * exposure),
                        (Background::Environment, _) => Vec3::ZERO,
                    };
                    acc += trace(scene, maps, light, s, origin, ray.dir, reach, &p, k, miss);
                }
            }

            let c = acc / (ss * ss) as f32;
            let display = |v: f32| match output {
                Output::Lit => linear_to_srgb(tone_map(s.color_space, v)),
                Output::Color => linear_to_srgb(v.clamp(0.0, 1.0)),
                Output::Data => v.clamp(0.0, 1.0),
            };
            let shown = [display(c.x), display(c.y), display(c.z)];
            let shown = match s.channels {
                ViewChannels::Rgb => shown,
                ViewChannels::Red => [shown[0]; 3],
                ViewChannels::Green => [shown[1]; 3],
                ViewChannels::Blue => [shown[2]; 3],
            };
            if let Some(px) = row.get_mut(x * 4..x * 4 + 4) {
                px.copy_from_slice(&[quantized(shown[0]), quantized(shown[1]), quantized(shown[2]), 255]);
            }
        }
    });
    if cancel.load(Ordering::Relaxed) { None } else { Some(out) }
}

/// A display value 0..1 as a byte
fn quantized(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_texture::Kind;

    #[test]
    fn renders_a_material_on_every_scene() {
        let light = Lighting::new("studio");
        let maps = Maps {
            base_color: Some(Image::filled(16, 16, Kind::Color, [0.8, 0.1, 0.1, 1.0])),
            roughness: Some(Image::filled(16, 16, Kind::Gray, [0.3; 4])),
            height: Some(Image::gray(16, 16, |x, _| (x % 4) as f32 / 4.0)),
            ..Maps::default()
        };

        for id in ["rounded_cube", "sphere", "plane", "cylinder", "torus", "material_ball", "cube"] {
            let scene = Scene::builtin(id);
            let px = render(&scene, &maps, &light, &Settings::default(), 64, 48);

            assert_eq!(px.len(), 64 * 48 * 4);

            // The center hits the mesh (the torus' hole is left of it): reddish
            let c = if id == "torus" { ((24 * 64 + 20) * 4) as usize } else { ((24 * 64 + 32) * 4) as usize };

            assert!(px[c] > px[c + 1], "{id}: center is not red ({:?})", &px[c..c + 3]);
        }
    }

    #[test]
    fn alpha_shows_what_lies_behind_and_the_alpha_view_shows_the_channel() {
        let (scene, light) = (Scene::builtin("sphere"), Lighting::new("studio"));
        let red = |a: f32| Maps { base_color: Some(Image::filled(16, 16, Kind::Color, [0.8, 0.1, 0.1, a])), ..Maps::default() };
        let c = ((24 * 64 + 32) * 4) as usize;
        let s = Settings::default();
        let opaque = render(&scene, &red(1.0), &light, &s, 64, 48);
        let clear = render(&scene, &red(0.0), &light, &s, 64, 48);

        assert!(opaque[c] > opaque[c + 1] + 40, "opaque center is red: {:?}", &opaque[c..c + 3]);

        // Fully clear: the backdrop shows through both the front and the back of the sphere
        let behind = backdrop(Vec2::new(32.5 / 64.0 * 2.0 - 1.0, 1.0 - 24.5 / 48.0 * 2.0)).x;
        let bg = quantized(linear_to_srgb(tone_map(s.color_space, behind)));
        assert!(clear[c..c + 3].iter().all(|v| v.abs_diff(bg) <= 1), "clear {:?} vs backdrop {bg}", &clear[c..c + 3]);

        // Transparency off: alpha is ignored
        assert_eq!(render(&scene, &red(0.0), &light, &Settings { transparency: false, ..s }, 64, 48), opaque);

        // An opacity output wins over the base color's alpha
        let half = Maps { opacity: Some(Image::filled(16, 16, Kind::Gray, [0.5; 4])), ..red(1.0) };
        let px = render(&scene, &half, &light, &Settings { alpha_view: true, ..s }, 64, 48);

        assert!(px[c].abs_diff(128) <= 1 && px[c] == px[c + 1], "alpha view: {:?}", &px[c..c + 3]);

        let px = render(&scene, &red(1.0), &light, &Settings { alpha_view: true, ..s }, 64, 48);

        assert_eq!(&px[c..c + 3], &[255, 255, 255]);
    }

    fn flat(value: f32) -> Image {
        Image::filled(8, 8, Kind::Gray, [value; 4])
    }

    fn displacement(depth: f32, level: f32) -> Displacement {
        Displacement { depth, level, repeats: [1.0; 2], max_triangles: 2048, shade_relief: false }
    }

    /// Heights of a displaced plane's vertices (it faces +y and starts at y = 0).
    fn plane_heights(height: &Image, displacement: &Displacement) -> Vec<f32> {
        let plane = materialcraft_geom::primitives::plane(2.0, 4);
        Scene::displaced(&plane, height, displacement).mesh.positions.iter().map(|p| p.y).collect()
    }

    /// At level 0 the relief rises out of the surface: black stays on it and nothing sinks in.
    #[test]
    fn the_relief_rises_out_of_the_surface() {
        let near = |heights: Vec<f32>, expected: f32| heights.iter().all(|y| (y - expected).abs() < 1e-5);

        assert!(near(plane_heights(&flat(0.0), &displacement(0.2, 0.0)), 0.0), "black stays on the surface");
        assert!(near(plane_heights(&flat(1.0), &displacement(0.2, 0.0)), 0.2), "white rises by the depth");
        assert!(near(plane_heights(&flat(0.5), &displacement(0.2, 0.5)), 0.0), "the level height stays on the surface");
        assert!(near(plane_heights(&flat(0.0), &displacement(0.2, 0.5)), -0.1), "below the level sinks");
        assert!(near(plane_heights(&flat(1.0), &displacement(0.2, f32::NAN)), 0.2), "a broken level reads as 0");
    }

    /// Vertices at one spot (UV seams, hard edges) move together, so a displaced cube stays closed.
    #[test]
    fn a_displaced_cube_stays_closed() {
        // No subdivision, so vertex i of the result is vertex i of the cube
        let cube = builtin_mesh("cube");
        let ramp = Image::gray(32, 32, |x, y| ((x + y) % 32) as f32 / 31.0);
        let displaced = Scene::displaced(&cube, &ramp, &Displacement { max_triangles: 0, ..displacement(0.1, 0.0) });
        assert_eq!(displaced.mesh.positions.len(), cube.positions.len());

        let spot = |p: Vec3| (p * 1e4).round().as_ivec3().to_array();
        let mut moved_to: HashMap<[i32; 3], Vec3> = HashMap::new();
        for (before, after) in cube.positions.iter().zip(&displaced.mesh.positions) {
            let first = *moved_to.entry(spot(*before)).or_insert(*after);
            assert!((first - *after).length() < 1e-4, "{before:?} split into {first:?} and {after:?}");
        }
    }

    /// Without a normal map the displaced surface's own normals shade the relief; with one, the original normals stay.
    #[test]
    fn relief_normals_follow_the_displaced_surface_only_when_asked() {
        let plane = materialcraft_geom::primitives::plane(2.0, 8);
        let slope = Image::gray(16, 16, |x, _| x as f32 / 15.0);
        let tilted = |shade_relief| {
            let scene = Scene::displaced(&plane, &slope, &Displacement { shade_relief, ..displacement(0.5, 0.0) });
            scene.mesh.normals.iter().map(|n| n.x.abs()).fold(0.0f32, f32::max)
        };

        assert!(tilted(true) > 0.1, "a slope tilts the normals");
        assert!(tilted(false) < 1e-5, "the original normals are kept");
    }

    #[test]
    fn with_relief_needs_a_height_output_and_a_depth() {
        let scene = Scene::builtin("plane");
        let maps = Maps { height: Some(flat(1.0)), ..Maps::default() };
        let relief = Relief { depth: 0.1, level: 0.0 };

        assert!(scene.with_relief(&Maps::default(), relief, [1.0; 2], 2048).is_none());
        for depth in [0.0, -1.0, f32::NAN] {
            assert!(scene.with_relief(&maps, Relief { depth, ..relief }, [1.0; 2], 2048).is_none(), "{depth}");
        }

        // The depth is in scene radii
        let raised = scene.with_relief(&maps, relief, [1.0; 2], 2048).unwrap();
        let top = raised.mesh.positions.iter().map(|p| p.y).fold(f32::MIN, f32::max);
        assert!((top - 0.1 * scene.radius).abs() < 1e-4, "{top} vs {}", 0.1 * scene.radius);
    }

    #[test]
    fn a_cancelled_render_gives_up_and_an_uncancelled_one_matches_render() {
        let (scene, light, maps, s) = (Scene::builtin("sphere"), Lighting::new("studio"), Maps::default(), Settings::default());
        assert!(render_cancellable(&scene, &maps, &light, &s, 32, 24, &AtomicBool::new(true)).is_none());
        let px = render_cancellable(&scene, &maps, &light, &s, 32, 24, &AtomicBool::new(false));
        assert_eq!(px, Some(render(&scene, &maps, &light, &s, 32, 24)));
    }

    #[test]
    fn panning_moves_the_scene_with_the_pointer() {
        let scene = Scene::builtin("sphere");
        let s = Settings::default();
        let at = |s: &Settings| camera(&scene, s).project(scene.center, 1.0).unwrap();

        // Drag fractions move the center by the same fraction in y-up NDC
        let right = Settings { target: pan(&scene, &s, 0.1, 0.0), ..s };
        let p = at(&right);

        assert!((p.x - 0.2).abs() < 1e-3 && p.y.abs() < 1e-3, "{p:?}");

        let down = Settings { target: pan(&scene, &s, 0.0, 0.1), ..s };
        let p = at(&down);

        assert!(p.x.abs() < 1e-3 && (p.y + 0.2).abs() < 1e-3, "{p:?}");

        // Orbiting turns around the panned target, not the scene center
        let turned = Settings { yaw: s.yaw + 90.0, ..right };
        let cam = camera(&scene, &turned);

        assert!((cam.target - camera(&scene, &right).target).length() < 1e-5);

        // Panning back returns home; hostile inputs stay finite
        let back = pan(&scene, &right, -0.1, 0.0);

        assert!(back.iter().all(|v| v.abs() < 1e-4), "{back:?}");

        let far = pan(&scene, &s, 1e9, f32::NAN);

        assert!(far.iter().all(|v| v.is_finite()), "{far:?}");

        for target in [[f32::NAN, 0.0, 0.0], [0.0, f32::INFINITY, 0.0], [0.0, 0.0, f32::MAX]] {
            let cam = camera(&scene, &Settings { target, ..s });

            assert_eq!(cam.target, scene.center, "{target:?}");
            assert!(cam.eye().is_finite());
        }
    }
}
