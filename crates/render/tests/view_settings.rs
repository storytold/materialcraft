//! Every setting of the 3D view changes the rendered picture the way its name says.
use craft_texture::{Image, Kind, linear_to_srgb};
use materialcraft_geom::view::{Background, ColorSpace, HeightTechnique, MaterialValue, MeshInfo, Projection, Shader, View3d, ViewChannels};
use materialcraft_render::{Lighting, Maps, Scene, Settings, render};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 48;

/// The rendered frame of `view` on `scene` with `maps`
fn frame(scene: &str, maps: &Maps, view: &View3d) -> Vec<u8> {
    render(&Scene::builtin(scene), maps, &Lighting::new("studio"), &Settings::for_view(view), WIDTH, HEIGHT)
}

fn pixel(frame: &[u8], x: u32, y: u32) -> [u8; 3] {
    let start = ((y * WIDTH + x) * 4) as usize;
    [frame[start], frame[start + 1], frame[start + 2]]
}

fn center(frame: &[u8]) -> [u8; 3] {
    pixel(frame, WIDTH / 2, HEIGHT / 2)
}

fn brightness(color: [u8; 3]) -> u32 {
    color.iter().map(|&channel| channel as u32).sum()
}

/// A new view in front of one color, so that every pixel beside the mesh is the same
fn plain() -> View3d {
    View3d { background: Background::Color, ..View3d::default() }
}

/// How many pixels differ from the top-left corner, which always shows the background
fn covered(frame: &[u8]) -> usize {
    let background = pixel(frame, 0, 0);
    (0..WIDTH * HEIGHT).filter(|index| pixel(frame, index % WIDTH, index / WIDTH) != background).count()
}

fn with(parameter: &str, value: MaterialValue) -> View3d {
    let mut view = plain();
    view.material.insert(parameter.into(), value);
    view
}

fn display(linear: f32) -> u8 {
    (linear_to_srgb(linear) * 255.0 + 0.5) as u8
}

#[test]
fn the_uniform_background_shows_as_chosen_in_either_color_space() {
    for color_space in [ColorSpace::Aces, ColorSpace::Srgb] {
        let view = View3d { background_color: [0.5, 0.1, 0.02], color_space, ..plain() };
        let corner = pixel(&frame("sphere", &Maps::default(), &view), 0, 0);

        let expected = [display(0.5), display(0.1), display(0.02)];
        assert!(corner.iter().zip(expected).all(|(shown, wanted)| shown.abs_diff(wanted) <= 1), "{color_space:?}: {corner:?} vs {expected:?}");
    }
}

#[test]
fn a_new_view_stands_in_front_of_a_gray_backdrop_that_darkens_toward_the_corners() {
    let view = View3d::default();
    assert_eq!(view.background, Background::Studio);

    let shown = frame("sphere", &Maps::default(), &view);
    let (corner, beside) = (pixel(&shown, 0, 0), pixel(&shown, 6, HEIGHT / 2));
    assert!(corner[0] == corner[1] && corner[1] == corner[2], "the backdrop is gray: {corner:?}");
    assert!(brightness(beside) > brightness(corner) + 30, "lighter beside the mesh than in the corner: {beside:?} vs {corner:?}");
    assert!(brightness(corner) > brightness(pixel(&frame("sphere", &Maps::default(), &plain()), 0, 0)), "lighter than the uniform default");

    // The backdrop belongs to the frame, not to the scene: turning the camera leaves it where it is
    let turned = frame("sphere", &Maps::default(), &View3d { yaw: 200.0, pitch: -30.0, ..view });
    assert_eq!((pixel(&turned, 0, 0), pixel(&turned, 6, HEIGHT / 2)), (corner, beside));
}

#[test]
fn the_environment_background_differs_from_the_uniform_one_and_turns_with_the_environment() {
    let uniform = frame("sphere", &Maps::default(), &plain());
    let environment = View3d { background: Background::Environment, ..plain() };
    let shown = frame("sphere", &Maps::default(), &environment);
    assert_ne!(pixel(&shown, 0, 0), pixel(&uniform, 0, 0));

    let turned = frame("sphere", &Maps::default(), &View3d { env_rotation: 120.0, ..environment.clone() });
    assert_ne!(pixel(&turned, 2, 2), pixel(&shown, 2, 2));

    // Without the environment nothing is behind the mesh and nothing lights it
    let off = frame("sphere", &Maps::default(), &View3d { env_enabled: false, ..environment });
    assert_eq!((pixel(&off, 0, 0), center(&off)), ([0; 3], [0; 3]));
}

#[test]
fn a_longer_lens_and_a_parallel_projection_change_how_much_of_the_frame_the_mesh_fills() {
    let normal = covered(&frame("sphere", &Maps::default(), &plain()));
    let long = covered(&frame("sphere", &Maps::default(), &View3d { focal_length: 70.0, ..plain() }));
    let wide = covered(&frame("sphere", &Maps::default(), &View3d { focal_length: 18.0, ..plain() }));
    assert!(wide < normal && normal < long, "{wide} < {normal} < {long}");

    let parallel = covered(&frame("sphere", &Maps::default(), &View3d { projection: Projection::Orthographic, ..plain() }));
    assert_ne!(parallel, normal);
    assert!(parallel > 0);
}

#[test]
fn clip_planes_cut_away_what_lies_outside_them() {
    // The camera is 3.2 radii (about 5.5 mesh units, 277 scene units) from the center of the sphere
    let clipped = |near_clip: f32, far_clip: f32| covered(&frame("sphere", &Maps::default(), &View3d { auto_clip: false, near_clip, far_clip, ..plain() }));
    let whole = covered(&frame("sphere", &Maps::default(), &plain()));

    assert_eq!(clipped(1.0, 10_000.0), whole);
    assert_eq!(clipped(600.0, 10_000.0), 0, "the near plane lies behind the sphere");
    assert_eq!(clipped(1.0, 100.0), 0, "the far plane lies in front of it");

    // Cut through the middle, the far half is seen from inside once its back faces are drawn
    let sliced = covered(&frame("sphere", &Maps::default(), &View3d { auto_clip: false, near_clip: 277.0, backface_culling: false, ..plain() }));
    assert!(sliced > 0 && sliced < whole, "{sliced} of {whole}");
    assert_eq!(clipped(277.0, 10_000.0), 0, "with culling the inside is not drawn");
}

#[test]
fn the_lights_add_their_light() {
    let dark = View3d { env_enabled: false, ..plain() };
    assert_eq!(center(&frame("sphere", &Maps::default(), &dark)), [0; 3]);

    let mut ambient = dark.clone();
    ambient.ambient.color = [0.2, 0.0, 0.0];
    let lit = center(&frame("sphere", &Maps::default(), &ambient));
    assert!(lit[0] > 0 && lit[1] == 0, "{lit:?}");
    ambient.ambient.enabled = false;
    assert_eq!(center(&frame("sphere", &Maps::default(), &ambient)), [0; 3]);

    // A point light where the camera is lights the middle of the sphere; brighter with more intensity
    let mut point = dark.clone();
    point.lights[0] = materialcraft_geom::view::PointLight { enabled: true, yaw: point.yaw, pitch: point.pitch, ..point.lights[0] };
    let one = brightness(center(&frame("sphere", &Maps::default(), &point)));
    point.lights[0].intensity = 3.0;
    let three = brightness(center(&frame("sphere", &Maps::default(), &point)));
    assert!(one > 0 && three > one, "{one} then {three}");

    // From behind the sphere it does not reach the side the camera sees
    point.lights[0].yaw += 180.0;
    point.lights[0].pitch = -point.lights[0].pitch;
    assert_eq!(center(&frame("sphere", &Maps::default(), &point)), [0; 3]);
}

#[test]
fn a_shadow_falls_where_the_mesh_blocks_a_point_light() {
    // The inner cube stands on the floor of the box; a light above the far corner casts its shadow on the floor
    let mut view = View3d { env_enabled: false, pitch: 50.0, ..plain() };
    view.lights[0] = materialcraft_geom::view::PointLight { enabled: true, yaw: 215.0, pitch: 35.0, distance: 1.1, ..view.lights[0] };
    let open = frame("inner_box", &Maps::default(), &view);
    let shadowed = frame("inner_box", &Maps::default(), &View3d { shadows: true, ..view });

    let darker = (0..WIDTH * HEIGHT)
        .filter(|index| brightness(pixel(&shadowed, index % WIDTH, index / WIDTH)) < brightness(pixel(&open, index % WIDTH, index / WIDTH)))
        .count();
    let brighter = (0..WIDTH * HEIGHT)
        .filter(|index| brightness(pixel(&shadowed, index % WIDTH, index / WIDTH)) > brightness(pixel(&open, index % WIDTH, index / WIDTH)))
        .count();
    assert!(darker > 20 && brighter == 0, "{darker} darker, {brighter} brighter");
}

#[test]
fn backface_culling_hides_faces_that_point_away() {
    // The plane faces up; from below only its back is in view
    let below = View3d { pitch: -40.0, ..plain() };
    assert_eq!(covered(&frame("plane", &Maps::default(), &below)), 0);
    assert!(covered(&frame("plane", &Maps::default(), &View3d { backface_culling: false, ..below })) > 100);

    // The hollow box shows its inside through the culled walls in front
    assert!(covered(&frame("hollow_box", &Maps::default(), &plain())) > 100);
}

#[test]
fn wireframe_draws_the_edges_and_leaves_the_faces_open() {
    // Two triangles seen from above: their outline and the diagonal between them
    let quad = Scene::new(materialcraft_geom::primitives::plane(2.0, 1));
    let shown = |view: &View3d| covered(&render(&quad, &Maps::default(), &Lighting::new("studio"), &Settings::for_view(view), WIDTH, HEIGHT));
    let above = View3d { pitch: 89.0, yaw: 0.0, ..plain() };

    let solid = shown(&above);
    let wire = shown(&View3d { wireframe: true, ..above });
    assert!(wire > 20 && wire * 3 < solid, "{wire} edge pixels of {solid}");
}

#[test]
fn wireframe_width_does_not_change_when_the_near_plane_moves_before_the_mesh() {
    let view = View3d { pitch: 89.0, yaw: 0.0, wireframe: true, ..plain() };
    let clipped = View3d { auto_clip: false, near_clip: 100.0, far_clip: 500.0, ..view.clone() };
    let plane = Scene::new(materialcraft_geom::primitives::plane(2.0, 1));

    let whole = render(&plane, &Maps::default(), &Lighting::new("studio"), &Settings::for_view(&view), 256, 256);
    let near_clipped = render(&plane, &Maps::default(), &Lighting::new("studio"), &Settings::for_view(&clipped), 256, 256);

    let changed_pixels = near_clipped.as_chunks::<4>().0.iter().zip(whole.as_chunks::<4>().0.iter()).filter(|(near, uncut)| near != uncut).count();
    assert_eq!(changed_pixels, 0, "moving a near plane before the mesh must keep wire widths");
}

#[test]
fn the_channel_choice_shows_one_channel_as_gray_and_the_color_space_changes_bright_light() {
    let red = Maps { base_color: Some(Image::filled(4, 4, Kind::Color, [0.9, 0.2, 0.05, 1.0])), ..Maps::default() };
    let all = center(&frame("sphere", &red, &plain()));

    for (view_channels, channel) in [(ViewChannels::Red, 0), (ViewChannels::Green, 1), (ViewChannels::Blue, 2)] {
        let shown = center(&frame("sphere", &red, &View3d { view_channels, ..plain() }));
        assert_eq!(shown, [all[channel]; 3], "{view_channels:?}");
    }

    let exposed = View3d { exposure: 2.0, ..plain() };
    let filmic = center(&frame("sphere", &red, &exposed));
    let plain = center(&frame("sphere", &red, &View3d { color_space: ColorSpace::Srgb, ..exposed }));
    assert_ne!(filmic, plain);
}

#[test]
fn the_simple_shaders_show_what_they_are_named_for() {
    let teal = Maps { base_color: Some(Image::filled(4, 4, Kind::Color, [0.1, 0.5, 0.4, 1.0])), ..Maps::default() };

    let unlit = center(&frame("sphere", &teal, &View3d { shader: Shader::Unlit, exposure: 3.0, ..plain() }));
    assert_eq!(unlit, [display(0.1), display(0.5), display(0.4)], "the color as it is, whatever the light");

    // The middle of the sphere faces the camera: its normal is the direction to the camera
    let view = View3d { shader: Shader::MeshInfo, mesh_info: MeshInfo::Normal, yaw: 0.0, pitch: 0.0, ..plain() };
    let normal = center(&frame("sphere", &teal, &view));
    assert!(normal[0].abs_diff(128) <= 6 && normal[1].abs_diff(128) <= 6 && normal[2] >= 250, "{normal:?}");
    let uv = center(&frame("plane", &teal, &View3d { shader: Shader::MeshInfo, mesh_info: MeshInfo::Uv, pitch: 89.0, yaw: 0.0, ..plain() }));
    assert!(uv[0].abs_diff(128) <= 8 && uv[1].abs_diff(128) <= 8 && uv[2] == 0, "the middle of the plane is the middle of its UVs: {uv:?}");

    // A highlight needs a model that has one
    let mut lit = View3d { env_enabled: false, shader: Shader::Blinn, ..plain() };
    lit.lights[0] = materialcraft_geom::view::PointLight { enabled: true, yaw: lit.yaw, pitch: lit.pitch, ..lit.lights[0] };
    lit.material.insert("specular".into(), MaterialValue::Color([1.0; 3]));
    let blinn = brightness(center(&frame("sphere", &teal, &lit)));
    let lambert = brightness(center(&frame("sphere", &teal, &View3d { shader: Shader::Lambert, ..lit })));
    assert!(blinn > lambert && lambert > 0, "{blinn} with a highlight, {lambert} without");
}

#[test]
fn every_rendered_parameter_of_the_default_model_changes_the_picture() {
    let plain = frame("rounded_cube", &Maps::default(), &plain());
    let changed = |view: &View3d| frame("rounded_cube", &Maps::default(), view) != plain;

    for (parameter, value) in [
        ("baseWeight", MaterialValue::Number(0.3)),
        ("baseColor", MaterialValue::Color([0.8, 0.1, 0.1])),
        ("baseMetalness", MaterialValue::Number(1.0)),
        ("baseDiffuseRoughness", MaterialValue::Number(1.0)),
        ("specularWeight", MaterialValue::Number(0.0)),
        ("specularColor", MaterialValue::Color([1.0, 0.2, 0.2])),
        ("specularRoughness", MaterialValue::Number(0.9)),
        ("specularIor", MaterialValue::Number(2.5)),
        ("transmissionWeight", MaterialValue::Number(0.8)),
        ("subsurfaceWeight", MaterialValue::Number(1.0)),
        ("coatWeight", MaterialValue::Number(1.0)),
        ("fuzzWeight", MaterialValue::Number(1.0)),
        ("emissionLuminance", MaterialValue::Number(2.0)),
        ("thinFilmWeight", MaterialValue::Number(1.0)),
        ("geometryOpacity", MaterialValue::Number(0.4)),
        ("geometryNormal", MaterialValue::Color([0.7, 0.5, 0.8])),
        ("ambientOcclusion", MaterialValue::Number(0.3)),
    ] {
        assert!(changed(&with(parameter, value)), "{parameter} changes nothing");
    }

    // These need the layer they belong to
    for (layer, parameter, value) in [
        ("specularRoughness", "specularAnisotropy", MaterialValue::Number(1.0)),
        ("transmissionWeight", "transmissionColor", MaterialValue::Color([1.0, 0.1, 0.1])),
        ("subsurfaceWeight", "subsurfaceColor", MaterialValue::Color([1.0, 0.1, 0.1])),
        ("subsurfaceWeight", "subsurfaceRadiusScale", MaterialValue::Color([0.1, 0.1, 1.0])),
        ("coatWeight", "coatColor", MaterialValue::Color([1.0, 0.3, 0.1])),
        ("coatWeight", "coatRoughness", MaterialValue::Number(0.8)),
        ("coatWeight", "coatIor", MaterialValue::Number(2.4)),
        ("coatWeight", "coatDarkening", MaterialValue::Number(0.0)),
        ("fuzzWeight", "fuzzColor", MaterialValue::Color([1.0, 0.1, 0.1])),
        ("fuzzWeight", "fuzzRoughness", MaterialValue::Number(1.0)),
        ("emissionLuminance", "emissionColor", MaterialValue::Color([1.0, 0.1, 0.1])),
        ("thinFilmWeight", "thinFilmThickness", MaterialValue::Number(0.9)),
        ("thinFilmWeight", "thinFilmIor", MaterialValue::Number(2.2)),
    ] {
        let mut base = with(layer, MaterialValue::Number(if layer == "specularRoughness" { 0.4 } else { 1.0 }));
        let before = frame("rounded_cube", &Maps::default(), &base);
        base.material.insert(parameter.into(), value);
        assert!(frame("rounded_cube", &Maps::default(), &base) != before, "{parameter} changes nothing");
    }
}

#[test]
fn a_channel_switched_off_ignores_its_image() {
    let red = Maps { base_color: Some(Image::filled(4, 4, Kind::Color, [0.9, 0.05, 0.05, 1.0])), ..Maps::default() };
    let shown = center(&frame("sphere", &red, &View3d { shader: Shader::Unlit, ..plain() }));
    assert!(shown[0] > 200 && shown[1] < 80);

    let off = View3d { shader: Shader::Unlit, disabled: ["baseColor".to_string()].into(), ..plain() };
    assert_eq!(center(&frame("sphere", &red, &off)), [0; 3]);
}

#[test]
fn tiling_uv_scale_and_parallax_move_the_texture() {
    // Left half black, right half white: more repeats bring a different half under the middle of the view
    let halves = Maps { base_color: Some(Image::gray(64, 64, |x, _| if x < 32 { 0.0 } else { 1.0 }).to_color()), ..Maps::default() };
    let top = View3d { shader: Shader::Unlit, pitch: 89.0, yaw: 0.0, ..plain() };
    let at = |view: &View3d, x: u32| pixel(&frame("plane", &halves, view), x, HEIGHT / 2)[0];

    assert!(at(&top, WIDTH / 2 - 6) < 10 && at(&top, WIDTH / 2 + 6) > 245);
    let repeated = View3d { uv_scale: [2.0, 1.0], ..top.clone() };
    assert!(at(&repeated, WIDTH / 2 + 2) < 10, "with two repeats the middle starts the second one");
    assert_eq!(at(&View3d { tiling: 2.0, ..top.clone() }, WIDTH / 2 + 2), at(&repeated, WIDTH / 2 + 2));

    // Seen at a slant, parallax slides the texture over a height field that is not at its top
    let relief = Maps { height: Some(Image::filled(8, 8, Kind::Gray, [1.0; 4])), ..halves };
    let slanted = View3d { shader: Shader::OpenPbr, technique: HeightTechnique::Parallax, height_scale: 0.3, pitch: 35.0, yaw: 60.0, ..plain() };
    let moved = frame("plane", &relief, &slanted);
    let still = frame("plane", &relief, &View3d { technique: HeightTechnique::Tessellation, displacement: false, ..slanted });
    assert!(moved != still, "parallax leaves the picture as it is");
}

#[test]
fn parallax_respects_displacement_and_height_channel_switches() {
    let color = Image::gray(64, 64, |x, _| if x < 32 { 0.0 } else { 1.0 }).to_color();
    let with_height = Maps { base_color: Some(color.clone()), height: Some(Image::filled(8, 8, Kind::Gray, [1.0; 4])), ..Maps::default() };
    let without_height = Maps { base_color: Some(color), ..Maps::default() };
    let view = View3d { shader: Shader::OpenPbr, technique: HeightTechnique::Parallax, height_scale: 0.3, pitch: 35.0, yaw: 60.0, ..plain() };

    assert_ne!(frame("plane", &with_height, &view), frame("plane", &without_height, &view));
    for disabled_view in [View3d { displacement: false, ..view.clone() }, View3d { disabled: ["height".to_string()].into(), ..view.clone() }] {
        assert!(
            frame("plane", &with_height, &disabled_view) == frame("plane", &without_height, &disabled_view),
            "disabled displacement or height must prevent parallax"
        );
    }
}

#[test]
fn equivalent_uv_repeats_have_the_same_parallax_offset() {
    let color = Image::gray(64, 64, |x, _| x as f32 / 63.0).to_color();
    let maps = Maps { base_color: Some(color), height: Some(Image::filled(8, 8, Kind::Gray, [1.0; 4])), ..Maps::default() };
    let view = View3d { shader: Shader::OpenPbr, technique: HeightTechnique::Parallax, height_scale: 0.3, pitch: 35.0, yaw: 60.0, ..plain() };

    let tiling = View3d { tiling: 2.0, ..view.clone() };
    let uv_scale = View3d { uv_scale: [2.0, 2.0], ..view };
    assert_eq!(frame("plane", &maps, &tiling), frame("plane", &maps, &uv_scale));
}
