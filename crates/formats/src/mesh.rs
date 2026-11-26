//! Meshes for the 3D view: OBJ and glTF/GLB (self-contained: embedded or data-URI buffers),
//! read with craft-mesh, placed by their node hierarchy, then normalized to fit a unit sphere at
//! the origin, with normals, tangents and (when the file has none) generated UVs.

use craft_mesh::{Limits, Scene};
use materialcraft_geom::Mesh;
use materialcraft_geom::glam::{Vec2, Vec3};

use crate::{FormatError, Result};

/// Mesh file extensions [`load_mesh`] reads.
pub const MESH_EXTENSIONS: &[&str] = &["obj", "glb", "gltf"];

/// Largest mesh accepted (triangles).
const MAX_TRIANGLES: usize = 4_000_000;

/// Largest mesh accepted (vertices).
const MAX_VERTICES: usize = 3 * MAX_TRIANGLES;

/// What the readers may take from one file. Every attribute pool is bounded by the vertex limit
/// and the file's faces by the triangle limit, before anything is allocated for them.
fn limits() -> Limits {
    Limits { max_vertices: MAX_VERTICES, max_triangles: MAX_TRIANGLES, ..Limits::default() }
}

fn check_vertex_bound(current: usize, added: usize) -> Result<()> {
    if current.checked_add(added).is_none_or(|total| total > MAX_VERTICES) {
        return Err(bad("the mesh is too large"));
    }

    Ok(())
}

fn bad(msg: impl Into<String>) -> FormatError {
    FormatError::BadPackage(msg.into())
}

/// A reader error as the message the 3D view shows; every exceeded limit reads the same.
fn read_error(kind: &str, e: craft_mesh::Error) -> FormatError {
    match e {
        craft_mesh::Error::TooLarge(_) => bad("the mesh is too large"),
        e => bad(format!("{kind}: {e}")),
    }
}

/// Build the mesh, rejecting non-finite positions, normals and UVs before anything computes
/// normals, tangents or bounds from them.
fn finish(positions: Vec<Vec3>, normals: Option<Vec<Vec3>>, uvs: Vec<Vec2>, indices: Vec<u32>) -> Result<Mesh> {
    if positions.iter().any(|p| !p.is_finite()) {
        return Err(bad("the mesh has non-finite (NaN or infinite) positions"));
    }

    if normals.iter().flatten().any(|n| !n.is_finite()) {
        return Err(bad("the mesh has non-finite (NaN or infinite) normals"));
    }

    if uvs.iter().any(|u| !u.is_finite()) {
        return Err(bad("the mesh has non-finite (NaN or infinite) UVs"));
    }

    Mesh::from_parts(positions, normals, uvs, indices, "material").map_err(|e| bad(e.to_string()))
}

/// `mesh` as Wavefront OBJ text: positions, texture coordinates, normals and triangles
pub fn obj_text(mesh: &Mesh) -> Result<String> {
    let (textured, shaded) = (mesh.uvs.len() == mesh.positions.len(), mesh.normals.len() == mesh.positions.len());
    let corners = mesh
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .flatten()
        .map(|&index| craft_mesh::Corner {
            position: index,
            uv: if textured { index } else { craft_mesh::NONE },
            normal: if shaded { index } else { craft_mesh::NONE },
        })
        .collect();
    let shared = craft_mesh::Mesh {
        positions: mesh.positions.iter().map(|p| [f64::from(p.x), f64::from(p.y), f64::from(p.z)]).collect(),
        normals: if shaded { mesh.normals.iter().map(|n| [f64::from(n.x), f64::from(n.y), f64::from(n.z)]).collect() } else { Vec::new() },
        uv_sets: if textured { vec![mesh.uvs.iter().map(|u| [u.x, u.y]).collect()] } else { Vec::new() },
        parts: vec![craft_mesh::Part { corners, ..craft_mesh::Part::default() }],
        ..craft_mesh::Mesh::default()
    };
    let scene = craft_mesh::Scene::from_meshes(vec![shared], Vec::new());
    craft_mesh::obj::write(&scene, None, &limits()).map_err(|e| read_error("OBJ", e))
}

/// Parse a mesh file by extension.
pub fn load_mesh(name: &str, bytes: &[u8]) -> Result<Mesh> {
    let extension = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    let limits = limits();

    // The view has no source folder for external file references
    let scene = match extension.as_str() {
        "obj" => craft_mesh::obj::read(bytes, &limits, &mut craft_mesh::no_external).map_err(|e| read_error("OBJ", e))?,
        "glb" | "gltf" => craft_mesh::gltf::read(bytes, &limits, &mut craft_mesh::no_external).map_err(|e| read_error("glTF", e))?,
        other => return Err(bad(format!("unsupported mesh format `{other}` (OBJ, glTF or GLB)"))),
    };
    let mut mesh = scene_mesh(&scene, &limits)?;
    if mesh.indices.is_empty() || mesh.positions.is_empty() {
        return Err(bad("the mesh has no triangles"));
    }

    if mesh.triangle_count() > MAX_TRIANGLES {
        return Err(bad(format!("the mesh has {} triangles (limit {MAX_TRIANGLES})", mesh.triangle_count())));
    }

    if mesh.uvs.len() != mesh.positions.len() || mesh.uvs.iter().all(|u| *u == mesh.uvs[0]) {
        // No usable UVs: a box projection keeps materials tiling across the surface
        mesh.uvs = box_uvs(&mesh);
    }

    mesh.normalize_to(1.0);
    if mesh.normals.len() != mesh.positions.len() {
        mesh.compute_normals();
    }

    mesh.compute_tangents();
    mesh.validate().map_err(|e| bad(e.to_string()))?;

    Ok(mesh)
}

/// Whether any value the file stored for a vertex is NaN or infinite. The reader drops the
/// triangles that reach a non-finite position; the view rejects the whole file instead, so a
/// damaged mesh is reported rather than shown with holes.
fn non_finite(scene: &Scene) -> Option<&'static str> {
    let f64s = |v: &[[f64; 3]]| v.iter().flatten().any(|x| !x.is_finite());
    if scene.meshes.iter().any(|mesh| f64s(&mesh.positions)) {
        return Some("positions");
    }

    if scene.meshes.iter().any(|mesh| f64s(&mesh.normals)) {
        return Some("normals");
    }

    if scene.meshes.iter().flat_map(|mesh| mesh.uv_sets.first()).any(|s| s.iter().flatten().any(|x| !x.is_finite())) {
        return Some("UVs");
    }

    None
}

/// One triangle mesh of every placed mesh in the scene. Positions are transformed into world
/// space in `f64`, then centered and scaled to a unit-size box before conversion to `f32`, so a
/// placement far from the origin keeps its shape; [`load_mesh`] then normalizes to a unit sphere.
/// Normals and UVs are kept only when every placed mesh has them (otherwise [`load_mesh`]
/// computes normals and projects UVs for the whole mesh); the first UV set is used. Normals go
/// through the inverse transpose, and a mirrored placement has its winding reversed so its
/// triangles keep facing out.
fn scene_mesh(scene: &Scene, limits: &Limits) -> Result<Mesh> {
    if let Some(what) = non_finite(scene) {
        return Err(bad(format!("the mesh has non-finite (NaN or infinite) {what}")));
    }

    let instances = scene.instances(limits).map_err(|e| read_error("mesh", e))?;
    let mut world_positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    let mut all_normals = true;
    let mut all_uvs = true;
    for (index, world) in instances {
        let Some(mesh) = scene.meshes.get(index) else { continue };
        let unified = mesh.unified();
        let vertex_count = unified.positions.len();
        check_vertex_bound(world_positions.len(), vertex_count)?;
        let Ok(base) = u32::try_from(world_positions.len()) else { return Err(bad("the mesh is too large")) };
        world_positions.extend(unified.positions.iter().map(|p| craft_mesh::transform_point(&world, *p)));
        if unified.normals.len() == vertex_count {
            normals.extend(unified.normals.iter().map(|q| to_vec3(craft_mesh::transform_normal(&world, *q))));
        } else {
            all_normals = false;
        }
        if unified.uvs.len() == vertex_count {
            uvs.extend(unified.uvs.iter().map(|t| Vec2::from(*t)));
        } else {
            all_uvs = false;
        }
        let mirror = craft_mesh::is_mirror(&world);
        for tri in unified.parts.iter().flatten() {
            if indices.len() / 3 >= MAX_TRIANGLES {
                return Err(bad("the mesh is too large"));
            }

            let [a, b, c] = tri.map(|i| i.saturating_add(base));
            indices.extend(if mirror { [a, c, b] } else { [a, b, c] });
        }
    }

    let positions = to_f32_positions(&world_positions);
    let normals = (all_normals && normals.len() == positions.len()).then_some(normals);

    // Partial UVs would hide the gap from the projection fallback: drop them all instead
    if !all_uvs || uvs.len() != positions.len() {
        uvs.clear();
    }

    finish(positions, normals, uvs, indices)
}

/// Center and scale world-space positions in `f64`, then convert to `f32`. A unit quad whose
/// node sits at a large translation would otherwise collapse: at magnitude `2^25` the `f32`
/// spacing is 4, so the four corners round to one point and [`Mesh::normalize_to`] cannot
/// recover the shape.
fn to_f32_positions(world: &[[f64; 3]]) -> Vec<Vec3> {
    if world.is_empty() {
        return Vec::new();
    }

    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in world {
        for (i, &x) in p.iter().enumerate() {
            lo[i] = lo[i].min(x);
            hi[i] = hi[i].max(x);
        }
    }
    let center = [0, 1, 2].map(|i| (lo[i] + hi[i]) * 0.5);
    let extent = hi.iter().zip(&lo).map(|(h, l)| h - l).fold(0.0, f64::max);
    let scale = if extent.is_finite() && extent > 0.0 { 1.0 / extent } else { 1.0 };
    world.iter().map(|p| to_vec3([(p[0] - center[0]) * scale, (p[1] - center[1]) * scale, (p[2] - center[2]) * scale])).collect()
}

fn to_vec3(p: [f64; 3]) -> Vec3 {
    Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32)
}

/// Box-projected UVs: each vertex takes the plane of its normal's largest axis.
fn box_uvs(mesh: &Mesh) -> Vec<Vec2> {
    let (lo, hi) = mesh.bounds();
    let size = (hi - lo).max_element().max(1e-6);
    mesh.positions
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let q = (*p - lo) / size;
            let n = mesh.normals.get(i).copied().unwrap_or(Vec3::Y).abs();
            if n.x >= n.y && n.x >= n.z {
                Vec2::new(q.z, 1.0 - q.y)
            } else if n.y >= n.z {
                Vec2::new(q.x, q.z)
            } else {
                Vec2::new(q.x, 1.0 - q.y)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mesh_written_as_obj_loads_back_as_it_was() {
        let mesh = materialcraft_geom::primitives::torus(1.0, 0.35, 12, 6);

        let loaded = load_mesh("torus.obj", obj_text(&mesh).unwrap().as_bytes()).unwrap();

        assert_eq!((loaded.triangle_count(), loaded.vertex_count()), (mesh.triangle_count(), mesh.vertex_count()));

        // Compare reindexed meshes within their own centered, scaled bounds
        let (loaded_low, loaded_high) = loaded.bounds();
        let (mesh_low, mesh_high) = mesh.bounds();
        let scale = (loaded_high - loaded_low).length() / (mesh_high - mesh_low).length();
        let (loaded_center, mesh_center) = ((loaded_low + loaded_high) * 0.5, (mesh_low + mesh_high) * 0.5);

        for (&loaded_corner, &mesh_corner) in loaded.indices.iter().zip(&mesh.indices) {
            let (a, b) = (loaded_corner as usize, mesh_corner as usize);
            assert!((loaded.positions[a] - loaded_center - (mesh.positions[b] - mesh_center) * scale).length() < 1e-4);
            assert!((loaded.normals[a] - mesh.normals[b]).length() < 1e-4);
            assert!((loaded.uvs[a] - mesh.uvs[b]).length() < 1e-5, "the texture is not flipped on the way");
        }
    }

    #[test]
    fn obj_quad_loads_with_uvs_and_tangents() {
        let obj = b"v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf 1/1 2/2 3/3 4/4\n";
        let mesh = load_mesh("quad.obj", obj).unwrap();

        assert_eq!(mesh.triangle_count(), 2);
        assert_eq!(mesh.tangents.len(), mesh.positions.len());
        assert!(mesh.radius() <= 1.0 + 1e-4);

        // A mesh without UVs gets projected ones
        let mesh = load_mesh("tri.obj", b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();

        assert!(mesh.uvs.iter().any(|u| *u != mesh.uvs[0]));
    }

    /// A glTF with 4 vertices (positions given by `positions`, optional UVs) and the given indices.
    fn gltf_json(positions: [[f32; 3]; 4], uvs: Option<[[f32; 2]; 4]>, indices: &[u16]) -> Vec<u8> {
        use base64::Engine as _;
        let mut buffer: Vec<u8> = positions.iter().flatten().flat_map(|f| f.to_le_bytes()).collect();
        let uv_offset_bytes = buffer.len();
        if let Some(uvs) = uvs {
            buffer.extend(uvs.iter().flatten().flat_map(|f| f.to_le_bytes()));
        }
        let index_offset_bytes = buffer.len();
        buffer.extend(indices.iter().flat_map(|i| i.to_le_bytes()));
        let encoded_buffer = base64::engine::general_purpose::STANDARD.encode(&buffer);
        let uv_attr = if uvs.is_some() { r#","TEXCOORD_0":1"# } else { "" };
        let uv_view = if uvs.is_some() { format!(r#"{{"buffer":0,"byteOffset":{uv_offset_bytes},"byteLength":32}},"#) } else { String::new() };
        let uv_acc = if uvs.is_some() { r#"{"bufferView":1,"componentType":5126,"count":4,"type":"VEC2"},"# } else { "" };

        // View/accessor layout: with UVs, views are [positions, uv, indices] and accessors [positions, uv, indices]
        let idx_view = if uvs.is_some() { 2 } else { 1 };
        let idx_acc = if uvs.is_some() { 2 } else { 1 };
        format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0{uv_attr}}},"indices":{idx_acc}}}]}}],
"accessors":[{{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3","min":[-1e9,-1e9,-1e9],"max":[1e9,1e9,1e9]}},{uv_acc}{{"bufferView":{idx_view},"componentType":5123,"count":{},"type":"SCALAR"}}],
"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":48}},{uv_view}{{"buffer":0,"byteOffset":{index_offset_bytes},"byteLength":{}}}],
"buffers":[{{"byteLength":{},"uri":"data:application/octet-stream;base64,{encoded_buffer}"}}]}}"#,
            indices.len(),
            indices.len() * 2,
            buffer.len()
        )
        .into_bytes()
    }

    const QUAD: [[f32; 3]; 4] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]];

    #[test]
    fn obj_with_excess_vertices_is_rejected() {
        let mut obj = String::from("v 0 0 0\nv 1 0 0\nv 0 1 0\n");
        for _ in 0..3 * MAX_TRIANGLES {
            obj.push_str("v 1 0 0\n");
        }
        obj.push_str("f 1 2 3\n");

        let e = load_mesh("excess.obj", obj.as_bytes()).unwrap_err();

        assert!(matches!(&e, FormatError::BadPackage(msg) if msg == "the mesh is too large"), "{e:?}");
    }

    #[test]
    fn gltf_primitives_sharing_a_large_accessor_stay_bounded() {
        use base64::Engine as _;

        // Shared accessors must count only vertices used by triangles
        const COUNT: usize = 100_000;
        let mut buffer = vec![0u8; COUNT * 12];

        // Give the referenced triangle non-degenerate positions
        for (i, v) in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]].iter().enumerate() {
            buffer[i * 12..i * 12 + 12].copy_from_slice(&v.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<_>>());
        }
        buffer.extend([0u16, 1, 2].iter().flat_map(|i| i.to_le_bytes()));
        let encoded_buffer = base64::engine::general_purpose::STANDARD.encode(&buffer);
        let primitive = r#"{"attributes":{"POSITION":0},"indices":1}"#;
        let primitives = (0..121).map(|_| primitive).collect::<Vec<_>>().join(",");
        let gltf = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
"meshes":[{{"primitives":[{primitives}]}}],
"accessors":[{{"bufferView":0,"componentType":5126,"count":{COUNT},"type":"VEC3","min":[0.0,0.0,0.0],"max":[1.0,1.0,0.0]}},{{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}}],
"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":{}}},{{"buffer":0,"byteOffset":{},"byteLength":6}}],
"buffers":[{{"byteLength":{},"uri":"data:application/octet-stream;base64,{encoded_buffer}"}}]}}"#,
            COUNT * 12,
            COUNT * 12,
            buffer.len()
        );
        let mesh = load_mesh("shared.gltf", gltf.as_bytes()).unwrap();

        assert_eq!(mesh.positions.len(), 3);
        assert_eq!(mesh.triangle_count(), 121);
    }

    #[test]
    fn obj_non_finite_normals_and_uvs_never_reach_the_mesh() {
        // Dropped invalid OBJ attributes are replaced with finite fallbacks
        for obj in
            [&b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvn nan 0 1\nf 1//1 2//1 3//1\n"[..], &b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvt nan 0\nvt 1 0\nvt 0 1\nf 1/1 2/2 3/3\n"[..]]
        {
            let mesh = load_mesh("x.obj", obj).unwrap();

            assert!(mesh.normals.iter().all(|n| n.is_finite()) && mesh.uvs.iter().all(|u| u.is_finite()), "{mesh:?}");
        }

        // A triangle on a NaN position is dropped; with nothing left the file is refused
        assert!(load_mesh("x.obj", b"v 0 0 0\nv 1 0 0\nv nan 1 0\nf 1 2 3\n").is_err());
    }

    /// [`QUAD`] with the given vertex normals, placed by one node.
    fn gltf_with_normals(normals: [[f32; 3]; 4]) -> Vec<u8> {
        use base64::Engine as _;
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
        let mut buffer: Vec<u8> = QUAD.iter().flatten().flat_map(|f| f.to_le_bytes()).collect();
        buffer.extend(normals.iter().flatten().flat_map(|f| f.to_le_bytes()));
        buffer.extend(indices.iter().flat_map(|i| i.to_le_bytes()));
        let encoded_buffer = base64::engine::general_purpose::STANDARD.encode(&buffer);
        format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1}},"indices":2}}]}}],
"accessors":[{{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3","min":[-1e9,-1e9,-1e9],"max":[1e9,1e9,1e9]}},{{"bufferView":1,"componentType":5126,"count":4,"type":"VEC3"}},{{"bufferView":2,"componentType":5123,"count":6,"type":"SCALAR"}}],
"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":48}},{{"buffer":0,"byteOffset":48,"byteLength":48}},{{"buffer":0,"byteOffset":96,"byteLength":12}}],
"buffers":[{{"byteLength":{},"uri":"data:application/octet-stream;base64,{encoded_buffer}"}}]}}"#,
            buffer.len()
        )
        .into_bytes()
    }

    /// `gltf` with its one placing node replaced by `node`.
    fn placed(gltf: Vec<u8>, node: &str) -> Vec<u8> {
        let base = String::from_utf8(gltf).unwrap();
        let out = base.replace(r#""nodes":[{"mesh":0}]"#, &format!(r#""nodes":[{node}]"#));

        assert_ne!(base, out, "the node was not replaced");
        out.into_bytes()
    }

    #[test]
    fn gltf_non_finite_normals_are_rejected() {
        let mut nan_normals = [[0.0f32, 0.0, 1.0]; 4];
        nan_normals[2][0] = f32::NAN;

        assert!(load_mesh("x.gltf", &gltf_with_normals(nan_normals)).is_err());

        let mut inf_normals = [[0.0f32, 0.0, 1.0]; 4];
        inf_normals[1][2] = f32::INFINITY;

        assert!(load_mesh("x.gltf", &gltf_with_normals(inf_normals)).is_err());
    }

    #[test]
    fn a_far_translation_keeps_the_quads_shape() {
        // At 2^25, f32 conversion before centering would collapse the unit quad
        let local = load_mesh("quad.gltf", &gltf_json(QUAD, None, &[0, 1, 2, 0, 2, 3])).unwrap();
        let far = 33_554_432.0;
        let placed = placed(gltf_json(QUAD, None, &[0, 1, 2, 0, 2, 3]), &format!(r#"{{"mesh":0,"translation":[{far},{far},0]}}"#));
        let distant = load_mesh("far.gltf", &placed).unwrap();

        let local_extent = extent(&local);
        let distant_extent = extent(&distant);

        assert_eq!(distant.triangle_count(), local.triangle_count());
        assert!((distant_extent - local_extent).length() < 1e-4, "local={local_extent} far={distant_extent}");
        for (a, b) in local.positions.iter().zip(&distant.positions) {
            assert!(a.distance(*b) < 1e-4, "local={a} far={b}");
        }
    }

    #[test]
    fn normals_follow_a_non_uniform_scale_by_the_inverse_transpose() {
        // Stretched twice along x, a (1,1,0) normal leans towards y: (0.5,1,0), normalised
        let gltf = placed(gltf_with_normals([[1.0, 1.0, 0.0]; 4]), r#"{"mesh":0,"scale":[2,1,1]}"#);
        let mesh = load_mesh("x.gltf", &gltf).unwrap();
        let want = Vec3::new(0.5, 1.0, 0.0).normalize();

        assert!(mesh.normals.iter().all(|n| n.distance(want) < 1e-5), "{:?}", mesh.normals);
    }

    #[test]
    fn a_mirrored_placement_keeps_its_triangles_facing_out() {
        let gltf = placed(gltf_with_normals([[0.0, 0.0, 1.0]; 4]), r#"{"mesh":0,"scale":[-1,1,1]}"#);
        let mesh = load_mesh("x.gltf", &gltf).unwrap();
        for tri in mesh.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|k| mesh.positions[tri[k] as usize]);
            let face = (b - a).cross(c - a);

            assert!(face.dot(mesh.normals[tri[0] as usize]) > 0.0, "triangle {tri:?} faces in");
        }
    }

    #[test]
    fn a_placed_mesh_without_uvs_still_gets_projected_ones() {
        // Two meshes: one with varied UVs, one with none, side by side
        let uvs = [[0.0f32, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let base = String::from_utf8(gltf_json(QUAD, Some(uvs), &[0, 1, 2, 0, 2, 3])).unwrap();
        let with = r#""meshes":[{"primitives":[{"attributes":{"POSITION":0,"TEXCOORD_0":1},"indices":2}]}]"#;

        assert!(base.contains(with));

        let two = base
            .replace(with, r#""meshes":[{"primitives":[{"attributes":{"POSITION":0,"TEXCOORD_0":1},"indices":2}]},{"primitives":[{"attributes":{"POSITION":0},"indices":2}]}]"#)
            .replace(r#""scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}]"#, r#""scenes":[{"nodes":[0,1]}],"nodes":[{"mesh":0},{"mesh":1,"translation":[2,0,0]}]"#);
        let mesh = load_mesh("two.gltf", two.as_bytes()).unwrap();

        assert_eq!(mesh.positions.len(), 8);
        let second = &mesh.uvs[4..];
        assert!(second.iter().any(|u| *u != second[0]), "the mesh without UVs got one constant UV: {second:?}");
    }

    #[test]
    fn a_bad_index_drops_its_whole_triangle_and_later_ones_keep_their_vertices() {
        // Triangles: (0,1,2), (0,1,9) out of range, (0,2,3)
        let mesh = load_mesh("x.gltf", &gltf_json(QUAD, None, &[0, 1, 2, 0, 1, 9, 0, 2, 3])).unwrap();

        assert_eq!(mesh.indices, vec![0, 1, 2, 0, 2, 3]);

        // An index count that is not a multiple of 3 drops only the partial triangle
        let mesh = load_mesh("x.gltf", &gltf_json(QUAD, None, &[0, 1, 2, 0, 2, 3, 1, 2])).unwrap();

        assert_eq!(mesh.indices, vec![0, 1, 2, 0, 2, 3]);
    }

    #[test]
    fn non_finite_positions_normals_and_uvs_are_rejected_before_processing() {
        let mut nan_pos = QUAD;
        nan_pos[2][0] = f32::NAN;
        let e = load_mesh("x.gltf", &gltf_json(nan_pos, None, &[0, 1, 2, 0, 2, 3])).unwrap_err();

        assert!(e.to_string().contains("non-finite"), "{e}");

        let mut nan_uv = [[0.0f32, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        nan_uv[1][1] = f32::NAN;
        let e = load_mesh("x.gltf", &gltf_json(QUAD, Some(nan_uv), &[0, 1, 2, 0, 2, 3])).unwrap_err();

        assert!(e.to_string().contains("non-finite"), "{e}");
    }

    #[test]
    fn bad_meshes_are_errors() {
        assert!(load_mesh("x.obj", b"").is_err());
        assert!(load_mesh("x.glb", b"glTF garbage").is_err());
        assert!(load_mesh("x.fbx", b"").is_err());
        assert!(load_mesh("x.obj", b"v 0 0 0\nf 1 2 3\n").is_err());
    }

    /// [`gltf_json`]'s quad with its scene's root nodes and node list replaced.
    fn gltf_with_nodes(roots: &str, nodes: &str) -> Vec<u8> {
        let base = String::from_utf8(gltf_json(QUAD, None, &[0, 1, 2, 0, 2, 3])).unwrap();
        let placed = base.replace(r#""scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}]"#, &format!(r#""scenes":[{{"nodes":{roots}}}],"nodes":{nodes}"#));

        assert_ne!(base, placed, "the node list was not replaced");
        placed.into_bytes()
    }

    fn extent(mesh: &Mesh) -> Vec3 {
        let (lo, hi) = mesh.bounds();
        hi - lo
    }

    #[test]
    fn gltf_meshes_take_their_parent_nodes_transforms() {
        // The quad sits under a parent that stretches it three times taller
        let nested = gltf_with_nodes("[0]", r#"[{"scale":[1,3,1],"children":[1]},{"mesh":0}]"#);
        let mesh = load_mesh("nested.gltf", &nested).unwrap();
        let size = extent(&mesh);

        assert!((size.y / size.x - 3.0).abs() < 1e-4, "{size}");

        // A mesh placed twice is drawn twice
        let twice = gltf_with_nodes("[0,1]", r#"[{"mesh":0},{"mesh":0,"translation":[5,0,0]}]"#);

        assert_eq!(load_mesh("twice.gltf", &twice).unwrap().triangle_count(), 4);

        // Nodes outside the scene are not drawn
        let unplaced = gltf_with_nodes("[0]", r#"[{"mesh":0},{"mesh":0,"translation":[5,0,0]}]"#);

        assert_eq!(load_mesh("one.gltf", &unplaced).unwrap().triangle_count(), 2);
    }

    #[test]
    fn glb_container_loads() {
        let json = String::from_utf8(gltf_json(QUAD, None, &[0, 1, 2, 0, 2, 3])).unwrap();
        let glb = craft_mesh::gltf::glb_container(json.as_bytes(), &[]).unwrap();

        assert_eq!(load_mesh("quad.GLB", &glb).unwrap().triangle_count(), 2);
    }

    #[test]
    fn obj_polygons_split_corners_and_material_libraries() {
        // Missing material libraries must not discard usable UVs
        let obj = b"mtllib missing.mtl\nv 0 0 0\nv 1 0 0\nv 1.5 1 0\nv 0.5 2 0\nv -0.5 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0.5 1\nvt 0 1\nusemtl paint\nf 1/1 2/2 3/3 4/4 5/5\n";
        let mesh = load_mesh("penta.obj", obj).unwrap();

        assert_eq!(mesh.triangle_count(), 3);

        // OBJ stores v up; the view uses v down
        assert!(mesh.uvs.iter().any(|u| (u.y - 1.0).abs() < 1e-6));
        assert!(mesh.uvs.iter().any(|u| u.y.abs() < 1e-6));
    }

    #[test]
    fn reader_errors_read_as_the_views_messages() {
        let e = read_error("OBJ", craft_mesh::Error::TooLarge("vertices".into()));

        assert!(matches!(&e, FormatError::BadPackage(msg) if msg == "the mesh is too large"), "{e:?}");

        let e = read_error("OBJ", craft_mesh::Error::Malformed("bad face".into()));

        assert!(e.to_string().contains("OBJ: bad face"), "{e}");
    }

    #[test]
    fn vertex_bound_allows_three_vertices_per_triangle() {
        assert!(check_vertex_bound(0, 3 * MAX_TRIANGLES).is_ok());
        assert!(check_vertex_bound(0, 3 * MAX_TRIANGLES + 1).is_err());
    }

    #[test]
    fn obj_text_preserves_material_and_attribute_parity() {
        let positions = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)];
        let normals = vec![Vec3::Z; 3];
        let uvs = vec![Vec2::new(0.0, 0.25), Vec2::new(1.0, 0.25), Vec2::new(0.0, 0.75)];
        let mesh = Mesh::from_parts(positions.clone(), Some(normals.clone()), uvs, vec![0, 1, 2], "material").unwrap();
        let text = obj_text(&mesh).unwrap();

        assert!(!text.contains("mtllib") && !text.contains("usemtl"));
        assert!(!text.contains("\no ") && !text.contains("\ng ") && !text.contains("\ns "));
        assert!(text.contains("vt 0 0.75\n"), "OBJ flips v: {text}");
        assert!(text.contains("f 1/1/1 2/2/2 3/3/3\n"), "{text}");

        let without_uvs = Mesh { positions: positions.clone(), normals, indices: vec![0, 1, 2], ..Mesh::default() };
        let no_uv_text = obj_text(&without_uvs).unwrap();
        assert!(!no_uv_text.contains("vt "));
        assert!(no_uv_text.contains("f 1//1 2//2 3//3\n"), "{no_uv_text}");

        let positions_only = Mesh { positions, indices: vec![0, 1, 2], ..Mesh::default() };
        let pos_only_text = obj_text(&positions_only).unwrap();
        assert!(!pos_only_text.contains("vt ") && !pos_only_text.contains("vn "));
        assert!(pos_only_text.contains("f 1 2 3\n"), "{pos_only_text}");
    }

    #[test]
    fn obj_text_propagates_shared_writer_refusals() {
        assert!(obj_text(&Mesh::default()).is_err());

        let invalid = Mesh { positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y], indices: vec![0, 1, 99], ..Mesh::default() };
        assert!(obj_text(&invalid).is_err());
    }
}
