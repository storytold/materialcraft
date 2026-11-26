//! Built-in meshes with clean, non-overlapping UVs: material ball, sphere, (rounded) cube,
//! cylinder, torus and plane. Y is up; triangles are counter-clockwise seen from outside.

use std::f32::consts::{PI, TAU};

use glam::{Vec2, Vec3};

use crate::mesh::{Mesh, SubMesh};

/// A UV-space rectangle: origin and size.
#[derive(Clone, Copy, Debug)]
struct UvRect {
    o: Vec2,
    s: Vec2,
}

impl UvRect {
    const FULL: UvRect = UvRect { o: Vec2::ZERO, s: Vec2::ONE };
    fn map(&self, u: f32, v: f32) -> Vec2 {
        self.o + self.s * Vec2::new(u, v)
    }
    fn inset(&self, m: f32) -> UvRect {
        UvRect { o: self.o + Vec2::splat(m), s: self.s - Vec2::splat(2.0 * m) }
    }
}

#[derive(Default)]
struct Builder {
    p: Vec<Vec3>,
    n: Vec<Vec3>,
    uv: Vec<Vec2>,
    idx: Vec<u32>,
}

impl Builder {
    fn vert(&mut self, p: Vec3, n: Vec3, uv: Vec2) -> u32 {
        self.p.push(p);
        self.n.push(n.try_normalize().unwrap_or(Vec3::Y));
        self.uv.push(uv);
        (self.p.len() - 1) as u32
    }

    /// A grid of (rows+1)×(cols+1) vertices from `f(row, col)`, rows going "down" the texture.
    fn grid(&mut self, rows: usize, cols: usize, mut f: impl FnMut(usize, usize) -> (Vec3, Vec3, Vec2)) {
        let base = self.p.len() as u32;
        for i in 0..=rows {
            for j in 0..=cols {
                let (p, n, uv) = f(i, j);
                self.vert(p, n, uv);
            }
        }
        let w = (cols + 1) as u32;
        for i in 0..rows as u32 {
            for j in 0..cols as u32 {
                let a = base + i * w + j;
                let (b, c, d) = (a + 1, a + w, a + w + 1);
                self.idx.extend_from_slice(&[a, c, d, a, d, b]);
            }
        }
    }

    fn finish(self, material: &str) -> Mesh {
        let count = self.idx.len() as u32;
        let mut m = Mesh {
            positions: self.p,
            normals: self.n,
            uvs: self.uv,
            indices: self.idx,
            tangents: Vec::new(),
            colors: Vec::new(),
            submeshes: vec![SubMesh { name: material.into(), material: 0, start: 0, count }],
            materials: vec![material.into()],
        };
        m.compute_tangents();
        m
    }
}

/// UV sphere of `radius` centred at `center`.
pub fn sphere(radius: f32, segments: usize, rings: usize) -> Mesh {
    let mut b = Builder::default();
    sphere_into(&mut b, Vec3::ZERO, radius, segments.max(3), rings.max(2), UvRect::FULL);
    b.finish("Sphere")
}

fn sphere_into(b: &mut Builder, center: Vec3, radius: f32, segs: usize, rings: usize, r: UvRect) {
    b.grid(rings, segs, |i, j| {
        let (u, v) = (j as f32 / segs as f32, i as f32 / rings as f32);
        let (theta, phi) = (v * PI, u * TAU + PI * 1.5);
        let n = Vec3::new(theta.sin() * phi.cos(), theta.cos(), -theta.sin() * phi.sin());
        (center + n * radius, n, r.map(u, v))
    });
}

/// A box with rounded edges (`bevel` = edge radius; 0 gives a plain cube). Faces are laid out
/// in a 3×2 UV atlas.
pub fn rounded_box(half: Vec3, bevel: f32) -> Mesh {
    let mut b = Builder::default();
    rounded_box_into(&mut b, Vec3::ZERO, half, bevel, UvRect::FULL);
    b.finish("Box")
}

/// The cube primitive (sharp edges).
pub fn cube(size: f32) -> Mesh {
    let mut m = rounded_box(Vec3::splat(size * 0.5), 0.0);
    m.materials = vec!["Cube".into()];
    if let Some(s) = m.submeshes.first_mut() {
        s.name = "Cube".into();
    }
    m
}

fn rounded_box_into(b: &mut Builder, center: Vec3, half: Vec3, bevel: f32, r: UvRect) {
    let bevel = bevel.clamp(0.0, half.min_element() * 0.95);

    // (normal, U = right, V = down) per face, seen from outside
    let faces = [
        (Vec3::Z, Vec3::X, -Vec3::Y),
        (Vec3::X, -Vec3::Z, -Vec3::Y),
        (-Vec3::Z, -Vec3::X, -Vec3::Y),
        (-Vec3::X, Vec3::Z, -Vec3::Y),
        (Vec3::Y, Vec3::X, Vec3::Z),
        (-Vec3::Y, Vec3::X, -Vec3::Z),
    ];
    let (kb, kc) = if bevel > 0.0 { (6usize, 10usize) } else { (0, 12) };

    // Coordinates along one face axis from -h to h, dense in the bevels
    let coords = |h: f32| -> Vec<f32> {
        let inner = h - bevel;
        let mut v = Vec::new();
        for i in 0..kb {
            v.push(-h + bevel * i as f32 / kb as f32);
        }
        for i in 0..=kc {
            v.push(-inner + 2.0 * inner * i as f32 / kc as f32);
        }
        for i in 1..=kb {
            v.push(inner + bevel * i as f32 / kb as f32);
        }
        v
    };
    let inner = half - Vec3::splat(bevel);
    for (f, (n, u, v)) in faces.iter().enumerate() {
        let cell = UvRect { o: r.map((f % 3) as f32 / 3.0, (f / 3) as f32 / 2.0), s: r.s * Vec2::new(1.0 / 3.0, 0.5) }.inset(0.01 * r.s.x);
        let (hu, hv, hn) = ((*u * half).abs().max_element(), (*v * half).abs().max_element(), (*n * half).abs().max_element());
        let (cu, cv) = (coords(hu), coords(hv));
        let (cols, rows) = (cu.len() - 1, cv.len() - 1);
        b.grid(rows, cols, |i, j| {
            let (a, c) = (cu[j], cv[i]);
            let q = *n * hn + *u * a + *v * c;
            let clamped = q.clamp(-inner, inner);
            let dir = (q - clamped).try_normalize().unwrap_or(*n);
            let p = if bevel > 0.0 { clamped + dir * bevel } else { q };
            let nn = if bevel > 0.0 { dir } else { *n };
            (center + p, nn, cell.map((a + hu) / (2.0 * hu), (c + hv) / (2.0 * hv)))
        });
    }
}

/// Surface of revolution of a profile `(radius, y)` listed top → bottom, mapped into `r`
/// (u around, v down the profile by arc length).
fn lathe_into(b: &mut Builder, center: Vec3, profile: &[(f32, f32)], segs: usize, r: UvRect) {
    if profile.len() < 2 {
        return;
    }

    let mut len = vec![0.0f32];
    for w in profile.windows(2) {
        let d = Vec2::new(w[1].0 - w[0].0, w[1].1 - w[0].1).length();
        len.push(len.last().copied().unwrap_or(0.0) + d);
    }
    let total = len.last().copied().unwrap_or(1.0).max(1e-6);
    let rows = profile.len() - 1;
    b.grid(rows, segs, |i, j| {
        let (rad, y) = profile[i];
        // Profile tangent (downwards) → outward normal in the (r, y) plane
        let prev = profile[i.saturating_sub(1)];
        let next = profile[(i + 1).min(rows)];
        let t = Vec2::new(next.0 - prev.0, next.1 - prev.1).try_normalize().unwrap_or(Vec2::NEG_Y);
        let n2 = Vec2::new(-t.y, t.x);
        let u = j as f32 / segs as f32;
        let phi = u * TAU + PI * 1.5;
        let dir = Vec3::new(phi.cos(), 0.0, -phi.sin());
        let p = center + dir * rad + Vec3::Y * y;
        let n = dir * n2.x + Vec3::Y * n2.y;
        (p, n, r.map(u, len[i] / total))
    });
}

/// A flat disk facing up (+Y) or down, with planar UVs inside `r`.
fn disk_into(b: &mut Builder, center: Vec3, radius: f32, up: bool, segs: usize, r: UvRect) {
    let n = if up { Vec3::Y } else { Vec3::NEG_Y };
    let c = b.vert(center, n, r.map(0.5, 0.5));
    let first = b.p.len() as u32;
    for j in 0..segs {
        // Match the lathe's first vertex and clockwise path around the rim
        let a = PI * 0.5 - j as f32 / segs as f32 * TAU;
        let (x, z) = (a.cos(), a.sin());
        // Top view: +X right, +Z down the texture; bottom view mirrors X
        let uv = if up { r.map(0.5 + x * 0.5, 0.5 + z * 0.5) } else { r.map(0.5 + x * 0.5, 0.5 - z * 0.5) };
        b.vert(center + Vec3::new(x, 0.0, z) * radius, n, uv);
    }
    for j in 0..segs as u32 {
        let (a, bb) = (first + j, first + (j + 1) % segs as u32);
        if up {
            b.idx.extend_from_slice(&[c, a, bb]);
        } else {
            b.idx.extend_from_slice(&[c, bb, a]);
        }
    }
}

/// A capped cylinder along Y.
pub fn cylinder(radius: f32, height: f32, segs: usize) -> Mesh {
    let mut b = Builder::default();
    let h = height * 0.5;
    lathe_into(&mut b, Vec3::ZERO, &[(radius, h), (radius, -h)], segs.max(3), UvRect { o: Vec2::ZERO, s: Vec2::new(1.0, 0.5) }.inset(0.01));
    disk_into(&mut b, Vec3::Y * h, radius, true, segs.max(3), UvRect { o: Vec2::new(0.0, 0.5), s: Vec2::splat(0.5) }.inset(0.01));
    disk_into(&mut b, -Vec3::Y * h, radius, false, segs.max(3), UvRect { o: Vec2::new(0.5, 0.5), s: Vec2::splat(0.5) }.inset(0.01));
    b.finish("Cylinder")
}

/// A torus around Y.
pub fn torus(major: f32, minor: f32, segs: usize, sides: usize) -> Mesh {
    let mut b = Builder::default();
    let (segs, sides) = (segs.max(3), sides.max(3));
    b.grid(sides, segs, |i, j| {
        let (u, v) = (j as f32 / segs as f32, i as f32 / sides as f32);
        let phi = u * TAU + PI * 1.5;
        let theta = v * TAU;
        let dir = Vec3::new(phi.cos(), 0.0, -phi.sin());
        let n = dir * theta.cos() - Vec3::Y * theta.sin();
        (dir * major + n * minor, n, Vec2::new(u, v))
    });
    b.finish("Torus")
}

/// A subdivided plane facing +Y.
pub fn plane(size: f32, subdivisions: usize) -> Mesh {
    let mut b = Builder::default();
    let s = subdivisions.max(1);
    b.grid(s, s, |i, j| {
        let (u, v) = (j as f32 / s as f32, i as f32 / s as f32);
        (Vec3::new((u - 0.5) * size, 0.0, (v - 0.5) * size), Vec3::Y, Vec2::new(u, v))
    });
    b.finish("Plane")
}

/// Two planes side by side on UV tiles 1001 and 1002 (UDIM): one material, 4 × 2 units.
pub fn udim_plane() -> Mesh {
    let mut m = plane(2.0, 16);
    let mut right = m.clone();
    for p in &mut m.positions {
        p.x -= 1.0;
    }
    for p in &mut right.positions {
        p.x += 1.0;
    }
    for uv in &mut right.uvs {
        uv.x += 1.0;
    }
    right.materials.clear();
    right.submeshes.clear();
    m.append(&right);
    let count = m.indices.len() as u32;
    if let Some(s) = m.submeshes.first_mut() {
        s.count = count;
    }
    m
}

/// The default project mesh: a ball on a beveled pedestal, two materials ("Ball", "Base").
pub fn material_ball() -> Mesh {
    let mut ball = Builder::default();
    sphere_into(&mut ball, Vec3::new(0.0, 1.32, 0.0), 1.0, 128, 64, UvRect::FULL);
    let mut ball = ball.finish("Ball");

    let mut base = Builder::default();
    let (r, top, bot, bev) = (1.05f32, 0.32f32, 0.0f32, 0.07f32);
    let mut profile = vec![(0.42, top + 0.06), (0.62, top + 0.06)];

    // Collar bevel and the ring the ball sits on, then the beveled drum
    for k in 1..=6 {
        let a = k as f32 / 6.0 * PI * 0.5;
        profile.push((0.62 + 0.06 * a.sin(), top + 0.06 * a.cos()));
    }
    profile.push((r - bev, top));
    for k in 1..=6 {
        let a = k as f32 / 6.0 * PI * 0.5;
        profile.push((r - bev + bev * a.sin(), top - bev + bev * a.cos()));
    }
    profile.push((r, bot + bev));
    for k in 1..=6 {
        let a = k as f32 / 6.0 * PI * 0.5;
        profile.push((r - bev + bev * a.cos(), bot + bev - bev * a.sin()));
    }
    lathe_into(&mut base, Vec3::ZERO, &profile, 128, UvRect { o: Vec2::ZERO, s: Vec2::new(1.0, 0.5) }.inset(0.01));
    disk_into(&mut base, Vec3::new(0.0, top + 0.06, 0.0), 0.42, true, 128, UvRect { o: Vec2::new(0.0, 0.5), s: Vec2::splat(0.5) }.inset(0.01));
    disk_into(&mut base, Vec3::new(0.0, bot, 0.0), r - bev, false, 128, UvRect { o: Vec2::new(0.5, 0.5), s: Vec2::splat(0.5) }.inset(0.01));
    let base = base.finish("Base");
    ball.append(&base);
    ball
}

/// A small showcase scene: a beveled crate with a rounded lid and a cylinder handle, in three
/// materials (used for demo projects and tests of multiple texture sets).
pub fn crate_scene() -> Mesh {
    let mut body = Builder::default();
    rounded_box_into(&mut body, Vec3::new(0.0, 0.8, 0.0), Vec3::new(1.2, 0.8, 0.8), 0.12, UvRect::FULL);
    let mut m = body.finish("Crate");
    let mut lid = Builder::default();
    rounded_box_into(&mut lid, Vec3::new(0.0, 1.72, 0.0), Vec3::new(1.28, 0.12, 0.88), 0.08, UvRect::FULL);
    m.append(&lid.finish("Lid"));
    let mut handle = torus(0.32, 0.07, 64, 24);
    for p in &mut handle.positions {
        *p = Vec3::new(p.x, p.z, -p.y) + Vec3::new(0.0, 1.84, 0.0);
    }
    for n in &mut handle.normals {
        *n = Vec3::new(n.x, n.z, -n.y);
    }
    handle.materials = vec!["Handle".into()];
    if let Some(s) = handle.submeshes.first_mut() {
        s.name = "Handle".into();
    }
    handle.compute_tangents();
    m.append(&handle);
    m
}

/// Squares along each side of the high-resolution plane
const HI_RES_PLANE_SUBDIVISIONS: usize = 256;
/// Squares along each side of the cloth
const CLOTH_SUBDIVISIONS: usize = 160;
/// Folds around the hanging part of the cloth
const CLOTH_FOLDS: f32 = 9.0;
/// Quarter-circle steps of the rounded cylinder's edges
const ROUNDED_EDGE_STEPS: usize = 10;

/// The sphere with its texture wrapped twice around, so texels are square at the equator
pub fn sphere_two_tiles() -> Mesh {
    let mut mesh = sphere(1.0, 128, 64);
    mesh.uvs.iter_mut().for_each(|uv| uv.x *= 2.0);
    mesh.compute_tangents();
    mesh
}

/// A cube seen from the inside: its faces point inward, so a room for a material
pub fn hollow_box(size: f32) -> Mesh {
    let mut mesh = cube(size);

    for triangle in mesh.indices.as_chunks_mut::<3>().0 {
        triangle.swap(1, 2);
    }
    mesh.normals.iter_mut().for_each(|normal| *normal = -*normal);

    // Seen from inside, the texture would read mirrored
    mesh.uvs.iter_mut().for_each(|uv| uv.x = 1.0 - uv.x);
    mesh.compute_tangents();
    rename(&mut mesh, "Hollow box");
    mesh
}

/// The hollow box with a cube half its size standing on its floor
pub fn inner_box(size: f32) -> Mesh {
    let mut mesh = hollow_box(size);
    let mut inner = cube(size * 0.5);

    inner.positions.iter_mut().for_each(|position| position.y -= size * 0.25);
    mesh.append(&inner);
    mesh
}

/// A cylinder along Y whose rims are rounded with radius `bevel`
pub fn rounded_cylinder(radius: f32, height: f32, bevel: f32, segments: usize) -> Mesh {
    let mut builder = Builder::default();
    let segments = segments.max(3);
    let half = height * 0.5;
    let bevel = bevel.clamp(0.0, radius.min(half) * 0.95);
    let quarter = |step: usize| step as f32 / ROUNDED_EDGE_STEPS as f32 * PI * 0.5;

    // Top rim, the straight side, then the bottom rim
    let mut profile: Vec<(f32, f32)> =
        (0..=ROUNDED_EDGE_STEPS).map(|step| (radius - bevel + bevel * quarter(step).sin(), half - bevel + bevel * quarter(step).cos())).collect();
    profile.extend((0..=ROUNDED_EDGE_STEPS).map(|step| (radius - bevel + bevel * quarter(step).cos(), -half + bevel - bevel * quarter(step).sin())));

    lathe_into(&mut builder, Vec3::ZERO, &profile, segments, UvRect { o: Vec2::ZERO, s: Vec2::new(1.0, 0.5) }.inset(0.01));
    disk_into(&mut builder, Vec3::Y * half, radius - bevel, true, segments, UvRect { o: Vec2::new(0.0, 0.5), s: Vec2::splat(0.5) }.inset(0.01));
    disk_into(&mut builder, -Vec3::Y * half, radius - bevel, false, segments, UvRect { o: Vec2::new(0.5, 0.5), s: Vec2::splat(0.5) }.inset(0.01));
    builder.finish("Rounded cylinder")
}

/// A square of cloth draped over a dome: flat on top, hanging in folds toward its edges
pub fn cloth(size: f32) -> Mesh {
    let mut builder = Builder::default();
    let half = size * 0.5;

    builder.grid(CLOTH_SUBDIVISIONS, CLOTH_SUBDIVISIONS, |row, column| {
        let (u, v) = (column as f32 / CLOTH_SUBDIVISIONS as f32, row as f32 / CLOTH_SUBDIVISIONS as f32);
        let (x, z) = (u * 2.0 - 1.0, v * 2.0 - 1.0);
        let reach = (x * x + z * z).sqrt();
        let hang = smooth_step(0.25, 1.3, reach);

        // The folds deepen as the cloth leaves the dome and pull the hanging part inward
        let fold = (z.atan2(x) * CLOTH_FOLDS).sin() * hang;
        let gather = 1.0 - 0.28 * hang + 0.07 * fold;
        let drop = 0.45 - 0.95 * hang.powf(1.4) + 0.05 * fold * hang;

        (Vec3::new(x * gather * half, drop * half, z * gather * half), Vec3::Y, Vec2::new(u, v))
    });

    let mut mesh = builder.finish("Cloth");
    mesh.compute_normals();
    mesh.compute_tangents();
    mesh
}

fn smooth_step(from: f32, to: f32, value: f32) -> f32 {
    let t = ((value - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One material and one part called `name`
fn rename(mesh: &mut Mesh, name: &str) {
    mesh.materials = vec![name.into()];
    for part in &mut mesh.submeshes {
        part.name = name.into();
    }
}

/// Built-in mesh by id (`builtin:<id>` in project files).
pub fn builtin(id: &str) -> Option<Mesh> {
    Some(match id {
        "material_ball" | "materialball" | "ball" => material_ball(),
        "sphere" => sphere(1.0, 128, 64),
        "sphere_2_tiles" => sphere_two_tiles(),
        "cube" => cube(2.0),
        "hollow_box" => hollow_box(2.0),
        "inner_box" => inner_box(2.0),
        "rounded_cube" | "roundedcube" | "box" => rounded_box(Vec3::ONE, 0.15),
        "cylinder" => cylinder(1.0, 2.0, 96),
        "rounded_cylinder" => rounded_cylinder(1.0, 2.0, 0.2, 96),
        "torus" => torus(1.0, 0.35, 128, 48),
        "plane" => plane(2.0, 16),
        "plane_hi_res" => plane(2.0, HI_RES_PLANE_SUBDIVISIONS),
        "cloth" => cloth(2.0),
        "crate" => crate_scene(),
        "udim_plane" | "udimplane" => udim_plane(),
        _ => return None,
    })
}

/// Ids accepted by [`builtin`], in menu order.
pub const BUILTINS: [(&str, &str); 15] = [
    ("material_ball", "Material ball"),
    ("sphere", "Sphere"),
    ("sphere_2_tiles", "Sphere 2 tiles"),
    ("cube", "Cube"),
    ("hollow_box", "Hollow box"),
    ("inner_box", "Inner box"),
    ("rounded_cube", "Rounded cube"),
    ("cylinder", "Cylinder"),
    ("rounded_cylinder", "Rounded cylinder"),
    ("torus", "Torus"),
    ("plane", "Plane"),
    ("plane_hi_res", "Plane (hi-res)"),
    ("cloth", "Cloth"),
    ("crate", "Crate"),
    ("udim_plane", "UDIM plane"),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every triangle's winding agrees with its vertex normals, UVs stay in 0..1 (u in 0..max_u),
    /// and the UV mapping is not mirrored (tangent handedness +1).
    fn check(m: &Mesh, max_u: f32) {
        m.validate().unwrap();
        let mut bad = 0;
        for t in 0..m.triangle_count() {
            let [a, b, c] = m.tri(t).unwrap();
            let face = (m.positions[b] - m.positions[a]).cross(m.positions[c] - m.positions[a]);
            if face.length() < 1e-9 {
                continue;
            }

            let n = m.normals[a] + m.normals[b] + m.normals[c];
            if face.dot(n) <= 0.0 {
                bad += 1;
            }
        }
        assert_eq!(bad, 0, "{} triangles wound against their normals", bad);
        for uv in &m.uvs {
            assert!((0.0..=max_u).contains(&uv.x) && (0.0..=1.0).contains(&uv.y), "{uv:?}");
        }
        let mirrored = m.tangents.iter().filter(|t| t.w < 0.0).count();
        assert!(mirrored * 50 < m.tangents.len().max(1), "{mirrored} mirrored tangents");
    }

    #[test]
    fn primitives_are_well_formed() {
        for (id, _) in BUILTINS {
            let m = builtin(id).unwrap();
            assert!(m.triangle_count() > 0, "{id}");
            check(&m, if id == "udim_plane" || id == "sphere_2_tiles" { 2.0 } else { 1.0 });
        }
        assert!(builtin("nope").is_none());
        assert_eq!(udim_plane().uv_tiles(0).into_iter().collect::<Vec<_>>(), vec![(0, 0), (1, 0)]);
    }

    #[test]
    fn the_new_scenes_have_the_shapes_their_names_promise() {
        let size = |mesh: &Mesh| {
            let (low, high) = mesh.bounds();
            high - low
        };

        assert_eq!(plane(2.0, HI_RES_PLANE_SUBDIVISIONS).triangle_count(), 131_072);
        assert!(sphere_two_tiles().uvs.iter().any(|uv| uv.x > 1.5), "the texture wraps twice");

        // The hollow box is the cube turned inside out
        let (solid, hollow) = (cube(2.0), hollow_box(2.0));
        assert_eq!(hollow.triangle_count(), solid.triangle_count());
        assert!(hollow.positions.iter().zip(&hollow.normals).all(|(position, normal)| position.dot(*normal) < 0.0), "the faces point inward");
        assert_eq!(inner_box(2.0).triangle_count(), solid.triangle_count() * 2);
        assert!((size(&inner_box(2.0)) - Vec3::splat(2.0)).length() < 1e-5, "the inner cube stays inside");

        let rounded = size(&rounded_cylinder(1.0, 2.0, 0.2, 96));
        assert!((rounded - Vec3::splat(2.0)).length() < 1e-3, "{rounded:?}");

        // The cloth is highest in the middle and hangs lowest at its corners
        let cloth = cloth(2.0);
        let height_at = |row: usize, column: usize| cloth.positions[row * (CLOTH_SUBDIVISIONS + 1) + column].y;
        let middle = CLOTH_SUBDIVISIONS / 2;
        assert!(height_at(middle, middle) > height_at(middle, 0) && height_at(middle, 0) > height_at(0, 0));
    }

    #[test]
    fn rounded_cylinder_caps_meet_the_rim_with_seven_segments() {
        let segments = 7;
        let mesh = rounded_cylinder(1.0, 2.0, 0.2, segments);
        let lathe_vertices = (ROUNDED_EDGE_STEPS * 2 + 2) * (segments + 1);
        let top_cap = lathe_vertices + 1;
        let bottom_cap = top_cap + segments + 1;
        let bottom_rim = lathe_vertices - segments - 1;

        for (cap, rim) in [(top_cap, 0), (bottom_cap, bottom_rim)] {
            for cap_position in &mesh.positions[cap..cap + segments] {
                assert!(mesh.positions[rim..rim + segments].iter().any(|rim_position| (*cap_position - *rim_position).length() < 1e-5));
            }
        }
    }

    #[test]
    fn material_ball_has_two_materials() {
        let m = material_ball();
        assert_eq!(m.materials, vec!["Ball".to_string(), "Base".to_string()]);
        assert!(m.triangles_of(0).count() > 0 && m.triangles_of(1).count() > 0);
    }
}
