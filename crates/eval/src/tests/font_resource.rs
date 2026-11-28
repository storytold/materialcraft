use super::*;
use materialcraft_core::{OutputFormat, Resource, ResourceBytes};

fn rectangle_font(height: i16, metrics: [i16; 3]) -> Vec<u8> {
    outline_font(metrics, 640, &[(0, 0, true), (0, height, true), (512, height, true), (512, 0, true)])
}

/// A two-glyph TrueType face with one contour for A. A false flag denotes a quadratic control point.
fn outline_font(metrics: [i16; 3], advance: i16, contour: &[(i16, i16, bool)]) -> Vec<u8> {
    fn words(values: &[i16]) -> Vec<u8> {
        values.iter().flat_map(|value| value.to_be_bytes()).collect()
    }
    fn checksum(data: &[u8]) -> u32 {
        data.chunks(4).fold(0u32, |sum, chunk| {
            let mut word = [0; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            sum.wrapping_add(u32::from_be_bytes(word))
        })
    }

    let mut head = vec![0; 54];
    head[..4].copy_from_slice(&0x10000u32.to_be_bytes());
    head[12..16].copy_from_slice(&0x5f0f3cf5u32.to_be_bytes());
    head[18..20].copy_from_slice(&1024i16.to_be_bytes());
    let bounds = [
        contour.iter().map(|p| p.0).min().unwrap(),
        contour.iter().map(|p| p.1).min().unwrap(),
        contour.iter().map(|p| p.0).max().unwrap(),
        contour.iter().map(|p| p.1).max().unwrap(),
    ];
    head[36..44].copy_from_slice(&words(&bounds));
    let mut hhea = vec![0; 36];
    hhea[..4].copy_from_slice(&0x10000u32.to_be_bytes());
    hhea[4..12].copy_from_slice(&words(&[metrics[0], metrics[1], metrics[2], advance]));
    hhea[34..].copy_from_slice(&2i16.to_be_bytes());
    let mut maxp = vec![0; 32];
    maxp[..4].copy_from_slice(&0x10000u32.to_be_bytes());
    let count = i16::try_from(contour.len()).unwrap();
    maxp[4..10].copy_from_slice(&words(&[2, count, 1]));
    let mut cmap = words(&[0, 1, 3, 1, 0, 12]);
    cmap.extend(words(&[4, 32, 0, 4, 4, 1, 0, 65, -1, 0, 65, -1, -64, 1, 0, 0]));
    let mut glyf = words(&[1, bounds[0], bounds[1], bounds[2], bounds[3], count - 1, 0]);
    glyf.extend(contour.iter().map(|p| u8::from(p.2)));
    for axis in 0..2 {
        let mut previous = 0;
        for &(x, y, _) in contour {
            let coordinate = [x, y][axis];
            glyf.extend((coordinate - previous).to_be_bytes());
            previous = coordinate;
        }
    }
    glyf.resize(glyf.len().next_multiple_of(2), 0);
    let loca = words(&[0, 0, (glyf.len() / 2) as i16]);
    let tables =
        [(b"cmap", cmap), (b"glyf", glyf), (b"head", head), (b"hhea", hhea), (b"hmtx", words(&[advance, 0, advance, 0])), (b"loca", loca), (b"maxp", maxp)];
    let mut font = words(&[1, 0, 7, 64, 2, 48]);
    font.resize(12 + 16 * tables.len(), 0);
    let mut head_start = 0;
    for (index, (tag, data)) in tables.into_iter().enumerate() {
        let offset = font.len();
        if tag == b"head" {
            head_start = offset;
        }
        let entry = &mut font[12 + 16 * index..12 + 16 * (index + 1)];
        entry[..4].copy_from_slice(tag);
        entry[4..8].copy_from_slice(&checksum(&data).to_be_bytes());
        entry[8..12].copy_from_slice(&(offset as u32).to_be_bytes());
        entry[12..].copy_from_slice(&(data.len() as u32).to_be_bytes());
        font.extend(data);
        font.resize(font.len().next_multiple_of(4), 0);
    }
    let adjustment = 0xb1b0_afba_u32.wrapping_sub(checksum(&font));
    font[head_start + 8..head_start + 12].copy_from_slice(&adjustment.to_be_bytes());
    font
}

#[test]
fn package_font_outlines_render_and_invalidate_the_parent_instance() {
    let (mut parent, instance) = instancing("Preview", "Letter");
    parent.size = [6, 5];
    parent.format = OutputFormat::F32;
    let mut child = Graph::new("Letter");
    child.size = [6, 5];
    let text = child.add_node("text", [0.0; 2]);
    child.node_mut(text).unwrap().params.extend([
        ("text".into(), Value::Text("A".into())),
        ("fontdata".into(), Value::Text("Ink".into())),
        ("fontsize".into(), Value::Float(0.5)),
    ]);
    let output = child.add_node("output", [100.0, 0.0]);
    child.connect(text, "output", output, "input").unwrap();
    let mut package = Package::new("Original lettering");
    package.graphs = vec![parent, child];
    package.resources.push(Resource::new("Ink", "Ink.ttf", ResourceBytes::new(rectangle_font(512, [768, -256, 0]))));
    let cache = Mutex::new(Cache::default());
    let options = Options::new(&no_res);
    for (height, first_row) in [(512, 8), (256, 12), (1024, 4)] {
        package.resources[0].bytes = ResourceBytes::new(rectangle_font(height, [768, -256, 0]));
        let direct = cook(&package, 1, &options, &cache);
        assert!(direct.nodes[&text].error.is_none(), "{:?}", direct.nodes[&text].error);
        let nested = cook(&package, 0, &options, &cache);
        assert!(!nested.nodes[&instance].cached);
        let image = nested.image(instance, 0).unwrap();
        assert_eq!((image.width(), image.height()), (64, 32));
        for y in 0..32 {
            for x in 0..64 {
                let ink = (24..40).contains(&x) && (first_row..16).contains(&y);
                assert_eq!(image.get_gray(x, y, false), f32::from(ink), "({x}, {y}) at cap height {height}");
            }
        }
        assert!(cook(&package, 0, &options, &cache).nodes[&instance].cached);
    }
}

fn text_sample(size: [u32; 2], metrics: [i16; 3], params: &[(&str, Value)]) -> (Package, NodeId) {
    let mut graph = Graph::new("Print");
    graph.size = size;
    graph.format = OutputFormat::F32;
    let id = graph.add_node("text", [0.0; 2]);
    let node = graph.node_mut(id).unwrap();
    node.params.insert("fontdata".into(), Value::Text("Ink".into()));
    node.params.extend(params.iter().map(|(name, value)| (name.to_string(), value.clone())));
    let mut package = Package::new("Original type");
    package.resources.push(Resource::new("Ink", "Ink.ttf", ResourceBytes::new(rectangle_font(512, metrics))));
    package.graphs.push(graph);
    (package, id)
}

#[test]
fn package_font_uses_horizontal_height_and_line_gap() {
    let (package, id) = text_sample([6, 6], [1536, -512, 512], &[("text", Value::Text("A\nA".into())), ("fontsize", Value::Float(0.25))]);
    let cooked = cook(&package, 0, &Options::new(&no_res), &Mutex::new(Cache::default()));
    assert_eq!(cooked.nodes[&id].error, None);
    let image = cooked.image(id, 0).unwrap();
    for y in 0..64 {
        for x in 0..64 {
            let ink = (30..34).contains(&x) && ((28..32).contains(&y) || (48..52).contains(&y));
            assert_eq!(image.get_gray(x, y, false), f32::from(ink), "({x}, {y})");
        }
    }
}

#[test]
fn package_font_clips_overhang_at_the_outside_of_the_text_block() {
    let (mut package, id) = text_sample(
        [6, 6],
        [768, -256, 0],
        &[("text", Value::Text("A\nA".into())), ("fontsize", Value::Float(0.25)), ("position", Value::Float2([0.0, -0.25]))],
    );
    package.resources[0].bytes = ResourceBytes::new(rectangle_font(1024, [768, -256, 0]));
    let cooked = cook(&package, 0, &Options::new(&no_res), &Mutex::new(Cache::default()));
    assert_eq!(cooked.nodes[&id].error, None);
    let image = cooked.image(id, 0).unwrap();
    for y in 0..64 {
        for x in 0..64 {
            let ink = (28..36).contains(&x) && (4..32).contains(&y);
            assert_eq!(image.get_gray(x, y, false), f32::from(ink), "({x}, {y})");
        }
    }
}

#[test]
fn package_font_quadratic_detail_tracks_the_text_scale() {
    let metrics = [768, -256, 0];
    let (mut package, id) = text_sample([5, 5], metrics, &[("text", Value::Text("A".into())), ("position", Value::Float2([-0.0625, 1.0 / 256.0]))]);
    let curve = [(0, 0, true), (512, 1024, false), (1024, 0, true)];
    let options = Options::new(&no_res);
    let cache = Mutex::new(Cache::default());
    for (size, segments) in [(0.35, 4), (0.35001, 8)] {
        package.graphs[0].node_mut(id).unwrap().params.insert("fontsize".into(), Value::Float(size));
        let chords: Vec<_> = (0..=segments).map(|i| ((1024 * i / segments) as i16, (2048 * i * (segments - i) / (segments * segments)) as i16, true)).collect();
        let mut render = |contour: &[(i16, i16, bool)]| {
            package.resources[0].bytes = ResourceBytes::new(outline_font(metrics, 640, contour));
            let result = cook(&package, 0, &options, &cache);
            assert_eq!(result.nodes[&id].error, None);
            result.image(id, 0).unwrap().clone()
        };
        let expected = render(&chords);
        assert!(expected.data().contains(&1.0));
        let ink = render(&curve);
        for (pixel, (a, b)) in ink.data().iter().zip(expected.data()).enumerate() {
            assert_eq!(a, b, "pixel {pixel} at size {size}");
        }
    }
}

#[test]
fn adjacent_quadratic_controls_place_their_join_in_whole_design_units() {
    let metrics = [768, -256, 0];
    let (mut package, id) = text_sample(
        [7, 6],
        metrics,
        &[
            ("text", Value::Text("A".into())),
            ("fontsize", Value::Float(0.5)),
            ("alignment", Value::Enum(0)),
            ("position", Value::Float2([-0.1875, 1.0 / 256.0])),
        ],
    );
    let implicit = [(0, 0, true), (55, -385, false), (492, 126, false), (675, 612, false), (1010, 0, true)];
    let explicit = [implicit[0], implicit[1], (273, -130, true), implicit[2], (583, 369, true), implicit[3], implicit[4]];
    let cache = Mutex::new(Cache::default());
    let mut render = |contour: &[(i16, i16, bool)]| {
        package.resources[0].bytes = ResourceBytes::new(outline_font(metrics, 640, contour));
        let result = cook(&package, 0, &Options::new(&no_res), &cache);
        assert_eq!(result.nodes[&id].error, None);
        result.image(id, 0).unwrap().clone()
    };
    let expected = render(&explicit);
    assert!(expected.data().contains(&1.0));
    let actual = render(&implicit);
    for (pixel, (a, b)) in actual.data().iter().zip(expected.data()).enumerate() {
        assert_eq!(a, b, "pixel {pixel} at the implicit join");
    }
}

#[test]
fn overlapping_glyphs_round_each_coverage_composition() {
    let metrics = [768, -256, 0];
    let (mut package, id) = text_sample([5, 5], metrics, &[("fontsize", Value::Float(0.5)), ("alignment", Value::Enum(0))]);
    let cache = Mutex::new(Cache::default());
    for (width, expected) in [(11, [44u8, 80, 110, 135]), (32, [128, 192, 224, 240])] {
        let contour = [(0, 0, true), (0, 512, true), (width, 512, true), (width, 0, true)];
        package.resources[0].bytes = ResourceBytes::new(outline_font(metrics, 0, &contour));
        for (count, code) in (1..=4).zip(expected) {
            package.graphs[0].node_mut(id).unwrap().params.insert("text".into(), Value::Text("A".repeat(count)));
            let cooked = cook(&package, 0, &Options::new(&no_res), &cache);
            assert_eq!(cooked.nodes[&id].error, None);
            let ink = cooked.image(id, 0).unwrap();
            for y in 0..32 {
                for x in 0..32 {
                    let coverage = if x == 16 && (8..16).contains(&y) { code } else { 0 };
                    assert_eq!(ink.get_gray(x, y, false), f32::from(coverage) / 255.0, "width {width}, {count} glyphs at ({x},{y})");
                }
            }
        }
    }
}

#[test]
fn package_font_coverage_interpolates_the_background_channels() {
    let (mut package, id) = text_sample(
        [5, 4],
        [768, -256, 0],
        &[
            ("text", Value::Text("A".into())),
            ("fontsize", Value::Float(0.5)),
            ("position", Value::Float2([1.0 / 128.0, 0.0])),
            ("background", Value::Color([1.0, 0.0, 0.0, 0.25])),
        ],
    );
    let cache = Mutex::new(Cache::default());
    for color in [false, true] {
        package.graphs[0].node_mut(id).unwrap().params.insert("color_mode".into(), Value::Bool(color));
        let cooked = cook(&package, 0, &Options::new(&no_res), &cache);
        assert_eq!(cooked.nodes[&id].error, None);
        let image = cooked.image(id, 0).unwrap();
        for (x, code) in [(0, 0), (12, 191), (18, 255), (20, 64)] {
            let alpha = f32::from(code as u8) / 255.0;
            if color {
                assert_eq!(image.get(x, 6, false), [1.0, alpha, alpha, (1.0 - alpha).mul_add(0.25, alpha)]);
            } else {
                assert_eq!(image.get_gray(x, 6, false), (1.0 - alpha).mul_add(1.0 / 3.0, alpha));
            }
        }
    }
}
