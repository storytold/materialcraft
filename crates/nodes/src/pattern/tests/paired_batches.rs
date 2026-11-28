use super::*;
use craft_texture::{Image, Kind};
use materialcraft_core::OutputFormat;

// Three disjoint portions of a procedural input address the active quadrant levels.
pub(super) fn field_property(depth: u32, offset: [f32; 2], frame: bool, repeated: bool) -> Value {
    let mut graph = FunctionGraph::default();
    let pos = graph.add("get_float2", &[], Some(Value::Text("$pos".into())));
    let scale = graph.add_constant(Value::Float2([(1 << depth) as f32 / 1024.0; 2]));
    let scaled = graph.add("mul", &[pos, scale], None);
    let origin = graph.add_constant(Value::Float2(offset));
    let uv = graph.add("add", &[scaled, origin], None);
    let uv = if repeated {
        let number = graph.add("get_float1", &[], Some(Value::Text("tree_index".into())));
        let quarter = graph.add_constant(Value::Float(0.25));
        let row = graph.add("mul", &[number, quarter], None);
        let zero = graph.add_constant(Value::Float(0.0));
        let bank = graph.add("vector2", &[zero, row], None);
        graph.add("add", &[uv, bank], None)
    } else {
        uv
    };
    let sample = graph.add("samplecol", &[uv], Some(Value::Int2([0, 0])));
    let result = if frame {
        let center = graph.add("swizzle2", &[sample], Some(Value::Int2([0, 1])));
        graph.add("sub", &[center, pos], None)
    } else {
        graph.add("swizzle1", &[sample], Some(Value::Int(2)))
    };
    graph.output = Some(result);
    Value::Function(Box::new(graph))
}

fn pair_copy_counts(pair: u32) -> [u32; 64] {
    std::array::from_fn(|index| {
        let group = index as u32;
        match (pair, group) {
            (60, 60) => 81,
            (60, 61) => 100,
            (60, 0..48) => 40 + (26 * group + 63) % 83,
            (60, _) => 45 + (9 * group + 8) % 37,
            (_, g) if g == pair => 75,
            (_, g) if g == pair + 1 => 106,
            (_, 0..48) => 27 + (30 * group + 64) % 67,
            _ => 45 + (15 * group + 11) % 37,
        }
    })
}

fn stamp_field(group_counts: &[u32; 64], markers: &[(u32, u32, bool)]) -> Image {
    let tiny = f32::EPSILON * 0.25;
    let mut selected = BTreeMap::new();
    for (j, &(negative, positive, _)) in markers.iter().enumerate() {
        let x = (16.5 + 2.0 * j as f32) / 128.0;
        selected.insert(negative, [x, 24.5 / 128.0, -1.0, 1.0]);
        selected.insert(positive, [x, 24.5 / 128.0, tiny, 1.0]);
    }
    Image::generate(1024, 1024, Kind::Color, |x, y| {
        let (depth, x, y) = match (x, y) {
            (0..512, 0..512) => (9, x, y),
            (512..768, 0..256) => (8, x - 512, y),
            (0..128, 512..640) => (7, x, y - 512),
            _ => return [0.0; 4],
        };
        let mut visit = 0;
        for bit in 0..depth {
            visit |= (((x >> bit) & 1) | (((y >> bit) & 1) << 1)) << (2 * bit);
        }
        // A parent is followed by four children, each with four grandchildren.
        let draw = match depth {
            7 => visit * 21,
            8 => (visit / 4) * 21 + 1 + (visit % 4) * 5,
            _ => (visit / 16) * 21 + 2 + ((visit / 4) % 4) * 5 + visit % 4,
        };
        if let Some(marker) = selected.get(&draw) {
            return *marker;
        }
        let subtree = draw / 1344;
        let group = subtree / 4;
        let extras = group_counts[group as usize];
        let quota = extras / 4 + u32::from((subtree % 4 + group) % 4 < extras % 4);
        let x = if draw % 1344 < quota { 0.0 } else { 64.5 / 128.0 };
        [x, 112.5 / 128.0, 0.25, 1.0]
    })
}

fn check_batch_markers(group_counts: &[u32; 64], seed: u32, axes: i32, markers: &[(u32, u32, bool)]) {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    let mut parent = graph.root.unwrap();
    for depth in 1..=9 {
        let child = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[(parent - 1) as usize].children.fill(Some(child));
        if depth >= 7 {
            let offset = match depth {
                7 => [0.0, 0.5],
                8 => [0.5, 0.0],
                _ => [0.0; 2],
            };
            let mut params = additive_square(0.25, [0.0; 2], [0.0; 2]);
            params.insert("frameoffset".into(), field_property(depth, offset, true, false));
            params.insert("opacity".into(), field_property(depth, offset, false, false));
            graph.nodes[(child - 1) as usize].parameters = params;
        }
        parent = child;
    }

    let definition = find("fx_map").unwrap();
    let params = resolve_params(
        definition,
        &BTreeMap::from([
            ("pattern".into(), Value::PatternGraph(Box::new(graph))),
            ("roughness".into(), Value::Float(0.0)),
            ("color_mode".into(), Value::Bool(false)),
            ("background".into(), Value::Color([1.0; 4])),
        ]),
    );
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(axes))]);
    let inputs = [None, Some(stamp_field(group_counts, markers))];
    let context = CookContext {
        w: 128,
        h: 128,
        seed,
        format: OutputFormat::F32,
        tile: true,
        inputs: &inputs,
        input_formats: &[OutputFormat::F32; 2],
        params: &params,
        resource: &|_| None,
        vars: &variables,
    };
    let image = (definition.cook)(&context).unwrap().remove(0);
    for (j, &(_, _, negative_first)) in markers.iter().enumerate() {
        let expected = if negative_first { f32::EPSILON * 0.25 } else { 0.0 };
        assert_eq!(image.get_gray(16 + 2 * j as i64, 24, false), expected, "seed {seed}, marker {j}");
    }
}

#[test]
fn full_pairs_defer_individual_copies_after_two_batches() {
    // Each pair of numbered stamps overlaps at one pixel. The final bit is set when the negative stamp draws first.
    let pair58 = [(322546, 46225, true), (322547, 1168, true), (322548, 2154, true), (322559, 818, true), (5359, 322551, false), (10731, 11628, false)];
    let pair60 = [
        (90032, 332682, false),
        (90033, 281378, false),
        (90034, 302451, false),
        (90047, 303240, false),
        (90046, 303080, false),
        (90045, 303435, false),
        (90044, 302501, false),
        (333311, 212967, true),
        (333298, 89032, true),
    ];
    let pair62 = [
        (344050, 272241, true),
        (344051, 258965, true),
        (344052, 262618, true),
        (344063, 262962, true),
        (263423, 332391, false),
        (263418, 253025, true),
        (263407, 257852, true),
        (268779, 333188, false),
    ];
    // With 96 extra copies in every group, the last twelve copies of group 60 draw later.
    let all_pairs = [
        (327923, 88021, true),
        (327924, 91851, true),
        (327925, 91651, true),
        (327935, 93255, true),
        (344042, 259673, true),
        (279529, 324712, false),
        (85920, 267859, false),
        (42912, 267222, false),
        (10643, 214976, false),
        (10644, 91692, false),
    ];
    let preceding_subtrees = [
        (274168, 129135, true),
        (274169, 326233, false),
        (274170, 323621, false),
        (274175, 327236, false),
        (295667, 219008, true),
        (295668, 172042, true),
        (295669, 172046, true),
        (295679, 172049, true),
        (317174, 47924, true),
        (317175, 24, true),
        (317176, 9, true),
        (317183, 34, true),
        (338676, 269607, true),
        (338677, 258061, true),
        (338678, 258081, true),
        (338687, 258057, true),
    ];
    let mut crossing = [32; 64];
    for (group, count) in [(50, 91), (54, 96), (58, 93), (62, 95)] {
        crossing[group] = count;
    }
    let second_pair_markers = [
        (279537, 138231, true),
        (279583, 94160, true),
        (87397, 15706, true),
        (90047, 978, true),
        (92773, 303597, false),
        (95423, 213678, false),
        (95461, 94418, true),
    ];
    let mut second_pair = [32; 64];
    second_pair[50] = 75;
    second_pair[51] = 107;
    let penultimate_prefix_markers = [
        (327930, 327964, false),
        (327931, 328139, false),
        (327934, 327958, false),
        (327935, 328054, false),
        (327967, 263018, true),
        (327973, 280124, true),
        (329279, 332822, false),
        (329317, 331280, false),
        (330623, 331042, false),
        (330661, 311138, true),
    ];
    let mut penultimate_prefix = [32; 64];
    penultimate_prefix[60] = 90;
    let combined_prefixes_markers = [
        (279537, 144436, true),
        (279583, 328198, false),
        (327930, 92890, true),
        (327931, 93598, true),
        (327934, 92755, true),
        (327935, 93864, true),
        (327967, 262553, true),
        (87397, 7121, true),
        (90047, 33270, true),
        (92773, 173881, false),
    ];
    let mut combined_prefixes = [32; 64];
    combined_prefixes[50] = 75;
    combined_prefixes[51] = 107;
    combined_prefixes[60] = 90;
    let fourth_pair_markers = [
        (301041, 217929, true),
        (301087, 180528, true),
        (173413, 18515, true),
        (176063, 11958, true),
        (178789, 262684, false),
        (181439, 265280, false),
        (181477, 179464, true),
    ];
    let mut fourth_pair = [32; 64];
    fourth_pair[54] = 75;
    fourth_pair[55] = 107;
    for (group_counts, seed, axes, markers) in [
        (pair_copy_counts(58), 653180, 1, pair58.as_slice()),
        (pair_copy_counts(60), 1763066, 1, pair60.as_slice()),
        (pair_copy_counts(62), 653180, 1, pair62.as_slice()),
        ([96; 64], 206407, 3, all_pairs.as_slice()),
        (crossing, 214261, 1, preceding_subtrees.as_slice()),
        (fourth_pair, 222453, 1, fourth_pair_markers.as_slice()),
        (second_pair, 220419, 1, second_pair_markers.as_slice()),
        (penultimate_prefix, 220472, 1, penultimate_prefix_markers.as_slice()),
        (combined_prefixes, 220525, 1, combined_prefixes_markers.as_slice()),
    ] {
        check_batch_markers(&group_counts, seed, axes, markers);
    }
}

#[test]
fn seven_batches_can_end_in_the_preceding_quadrant() {
    for (first, seed, markers) in [
        (8, 225641, [(80617, 251488, false), (80618, 313557, false), (80619, 2103, true), (80620, 208828, false), (80639, 321776, false), (80761, 2703, true)]),
        (
            24,
            225664,
            [(166633, 271610, false), (166634, 336460, false), (166635, 127058, true), (166636, 321100, false), (166655, 3532, true), (166777, 3002, true)],
        ),
        (
            40,
            225687,
            [(252649, 290337, false), (252650, 74284, true), (252651, 214430, true), (252652, 2247, true), (252671, 87708, true), (252793, 343869, false)],
        ),
    ] {
        let mut group_counts = [32; 64];
        group_counts[first..first + 7].fill(87);
        check_batch_markers(&group_counts, seed, 1, &markers);
    }
}

#[test]
fn deferred_copies_can_fill_the_receiving_quadrant() {
    let mut group_counts = [32; 64];
    group_counts[32] = 116;
    group_counts[54] = 90;
    group_counts[55] = 80;
    let markers = [
        (301065, 265134, true),
        (301220, 181946, true),
        (177441, 189607, false),
        (182783, 207671, false),
        (182817, 319146, false),
        (185471, 208185, false),
        (209663, 185762, true),
        (306431, 308842, false),
        (306465, 185374, true),
    ];
    check_batch_markers(&group_counts, 225301, 1, &markers);
}

#[test]
fn full_middle_pairs_continue_their_separate_subtrees() {
    for (pairs, seed, markers) in [
        (
            [[32, 32], [101, 90], [32, 32]],
            226537,
            [(279551, 102866, true), (280895, 256149, true), (283583, 288357, false), (284961, 255831, true), (288993, 302446, false), (290337, 222622, true)],
        ),
        (
            [[32, 32], [32, 32], [78, 101]],
            226568,
            [(301055, 181298, true), (302399, 342071, false), (305087, 328846, false), (306465, 341076, false), (310497, 332538, false), (311841, 54464, true)],
        ),
        (
            [[88, 90], [101, 90], [78, 101]],
            226599,
            [(279551, 97145, true), (280895, 256381, true), (283583, 332807, false), (284961, 258500, true), (288993, 310660, false), (290337, 222196, true)],
        ),
    ] {
        let mut group_counts = [32; 64];
        for (first, pair) in [48, 52, 56].into_iter().zip(pairs) {
            group_counts[first..first + 2].copy_from_slice(&pair);
        }
        check_batch_markers(&group_counts, seed, 1, &markers);
    }
}

#[test]
fn penultimate_pair_splits_when_the_partial_subtrees_fill_a_batch() {
    for (pair, seed, markers) in [
        (
            [79, 91],
            227173,
            [(322559, 6703, true), (323903, 86579, true), (326591, 276861, true), (327969, 267935, true), (332001, 288482, true), (333345, 270405, true)],
        ),
        (
            [85, 97],
            227204,
            [(322559, 16047, true), (323903, 277749, true), (326591, 94356, true), (327969, 258216, true), (332001, 267414, true), (333345, 272173, true)],
        ),
        (
            [101, 128],
            227235,
            [(322559, 14801, true), (323903, 98883, true), (326591, 279431, true), (327969, 167744, true), (332001, 214799, true), (333345, 129478, true)],
        ),
        (
            [79, 150],
            227266,
            [(322559, 22151, true), (323903, 91701, true), (326591, 91253, true), (331967, 283167, true), (333345, 129341, true), (87397, 309051, false)],
        ),
    ] {
        let mut group_counts = [32; 64];
        group_counts[60..62].copy_from_slice(&pair);
        check_batch_markers(&group_counts, seed, 1, &markers);
    }
}

#[test]
fn final_pair_continues_into_the_next_subtree_when_the_receiver_is_short() {
    for (prefix, preceding, pair, seed, markers) in [
        (
            80,
            32,
            [100, 71],
            228901,
            [(344060, 300640, true), (344061, 214437, true), (344062, 264313, true), (258046, 126810, true), (258072, 253595, true), (260735, 127893, true)],
        ),
        (
            81,
            32,
            [75, 96],
            228932,
            [(344060, 113385, true), (344061, 40986, true), (344062, 310665, true), (258046, 259178, false), (258072, 255856, true), (260735, 319019, false)],
        ),
        (
            82,
            32,
            [71, 100],
            228963,
            [(344060, 182379, true), (344061, 328265, true), (344062, 41546, true), (258046, 41088, true), (258072, 334314, false), (260735, 255892, true)],
        ),
        (
            50,
            91,
            [110, 67],
            228994,
            [(344054, 283285, true), (344055, 259617, true), (344056, 330090, true), (344058, 126934, true), (344060, 288415, true), (344063, 42392, true)],
        ),
    ] {
        let mut group_counts = [32; 64];
        group_counts[48] = prefix;
        group_counts[60] = preceding;
        group_counts[62..64].copy_from_slice(&pair);
        check_batch_markers(&group_counts, seed, 1, &markers);
    }
}

#[test]
fn second_quadrant_fills_a_batch_with_the_first_overflowing_pair() {
    for (head, pair, last_prefix, seed, markers) in [
        (
            80,
            [91, 81],
            66,
            239183,
            [(279544, 219111, true), (279545, 177140, true), (279547, 264248, true), (279550, 18082, true), (86047, 293310, false), (91385, 268863, false)],
        ),
        (
            84,
            [82, 87],
            84,
            239214,
            [(279547, 337858, false), (279548, 238689, true), (279550, 188473, true), (86047, 33592, true), (88046, 272116, false), (91561, 332595, false)],
        ),
        (
            72,
            [92, 89],
            97,
            239245,
            [(279535, 198075, true), (279536, 12379, true), (279538, 73395, true), (279541, 37478, true), (279544, 204365, true), (279549, 29131, true)],
        ),
        (
            67,
            [94, 90],
            73,
            239276,
            [(279532, 122270, true), (279533, 151260, true), (279535, 65152, true), (279538, 35090, true), (279541, 198163, true), (279546, 258888, true)],
        ),
    ] {
        let mut group_counts = [32; 64];
        group_counts[16] = head;
        group_counts[50..52].copy_from_slice(&pair);
        group_counts[62] = last_prefix;
        check_batch_markers(&group_counts, seed, 1, &markers);
    }
}

#[test]
fn three_final_pairs_share_the_penultimate_batch_with_middle_subtrees() {
    for (head, middle, last_head, pairs, seed, markers) in [
        (
            83,
            79,
            49,
            [[85, 84], [84, 86], [84, 87]],
            246191,
            [(13460, 191490, false), (13480, 253590, false), (14803, 320991, false), (212523, 37343, true), (215059, 7974, true), (266111, 10751, true)],
        ),
        (
            83,
            74,
            61,
            [[90, 81], [88, 87], [87, 85]],
            246220,
            [(13460, 321900, false), (13480, 290313, false), (14803, 60605, false), (212523, 221710, false), (215059, 67593, true), (266111, 268976, false)],
        ),
        (
            86,
            67,
            60,
            [[94, 79], [94, 83], [93, 81]],
            246249,
            [(13460, 145396, false), (13480, 136182, false), (14803, 58999, false), (212523, 29195, true), (215059, 113808, true), (266111, 339580, false)],
        ),
        (
            81,
            75,
            46,
            [[99, 83], [102, 80], [96, 87]],
            246278,
            [(13460, 150313, false), (13480, 84657, false), (14803, 24014, false), (212523, 7900, true), (215059, 60885, true), (266111, 317978, false)],
        ),
    ] {
        let mut group_counts = [32; 64];
        group_counts[0] = head;
        group_counts[16] = middle;
        group_counts[48] = last_head;
        for (group, pair) in [58, 60, 62].into_iter().zip(pairs) {
            group_counts[group..group + 2].copy_from_slice(&pair);
        }
        check_batch_markers(&group_counts, seed, 1, &markers);
    }
}

#[test]
fn eight_middle_group_batches_carry_into_the_first_subtrees() {
    for (head, eighth, receiver, middle, next, pair, last, seed, markers) in [
        (
            107,
            84,
            83,
            82,
            83,
            [79, 89],
            84,
            249179,
            [(4031, 302217, false), (4010, 192880, false), (40445, 66669, false), (259454, 316884, false), (260735, 298594, false), (212350, 87587, true)],
        ),
        (
            111,
            89,
            79,
            82,
            77,
            [84, 87],
            86,
            249210,
            [(4031, 284859, false), (4010, 245490, false), (40445, 243861, false), (259454, 312946, false), (260735, 321439, false), (212350, 30837, true)],
        ),
        (
            111,
            98,
            64,
            83,
            63,
            [91, 90],
            92,
            249241,
            [(4031, 159042, false), (4010, 208901, false), (40445, 199211, false), (259454, 343227, false), (260735, 315060, false), (212350, 335049, false)],
        ),
        (
            107,
            118,
            52,
            85,
            39,
            [99, 90],
            101,
            249272,
            [(4031, 169499, false), (4010, 175761, false), (40445, 226717, false), (259454, 312824, false), (260735, 340420, false), (212350, 24792, true)],
        ),
    ] {
        let mut group_counts = [32; 64];
        group_counts[0] = head;
        group_counts[24..31].fill(84);
        group_counts[31] = eighth;
        group_counts[32] = receiver;
        group_counts[33..40].fill(middle);
        group_counts[52] = next;
        group_counts[54..56].copy_from_slice(&pair);
        group_counts[60..62].copy_from_slice(&[84, 57]);
        group_counts[62] = last;
        check_batch_markers(&group_counts, seed, 1, &markers);
    }
}

#[test]
fn ten_batches_split_the_later_horizontal_subtrees() {
    for (head, following, middle, pair_start, later, end, last, seed, markers) in [
        (
            120,
            86,
            55,
            84,
            54,
            72,
            78,
            261084,
            [(303743, 213233, true), (303763, 342691, false), (303726, 212560, true), (303705, 3691, true), (259584, 327565, false), (260714, 281791, false)],
        ),
        (
            109,
            86,
            62,
            89,
            73,
            79,
            62,
            261115,
            [(303743, 261223, true), (303763, 326473, false), (303726, 213819, true), (303705, 2845, true), (259584, 286937, false), (260714, 281276, false)],
        ),
        (
            121,
            86,
            76,
            82,
            62,
            82,
            81,
            261146,
            [(303743, 268210, true), (303763, 319251, false), (303726, 260297, true), (303705, 1829, true), (259584, 1908, true), (260714, 302967, false)],
        ),
        (
            121,
            89,
            58,
            93,
            77,
            77,
            64,
            261177,
            [(303743, 288302, true), (303763, 339410, false), (303726, 305649, false), (303705, 1704, true), (259584, 335480, false), (260714, 281475, false)],
        ),
    ] {
        let mut groups = [61; 64];
        groups[0] = head;
        groups[1..8].fill(following);
        groups[52] = middle;
        groups[54] = pair_start;
        groups[56] = later;
        groups[58..60].copy_from_slice(&[80, 70]);
        groups[60] = end;
        groups[62] = last;
        check_batch_markers(&groups, seed, 1, &markers);
    }
}

#[test]
fn middle_batches_share_the_tail_of_the_final_horizontal_pair() {
    for (receiver, middle_tail, first, first_end, following, penultimate, last, last_end, seed, markers) in [
        (
            134,
            0,
            84,
            85,
            83,
            70,
            58,
            110,
            269119,
            [(172031, 56696, true), (284927, 174951, true), (344046, 315870, true), (90047, 2990, true), (40339, 171460, false), (263570, 282032, false)],
        ),
        (
            114,
            1,
            86,
            83,
            80,
            81,
            83,
            86,
            269156,
            [(172031, 188653, false), (284927, 342769, false), (344046, 6790, true), (90047, 282513, false), (40339, 170078, false), (263570, 281121, false)],
        ),
        (
            118,
            3,
            79,
            90,
            75,
            82,
            84,
            96,
            269193,
            [(172031, 202722, false), (284927, 262240, true), (344046, 28604, true), (90047, 282568, false), (40339, 171613, false), (263570, 281461, false)],
        ),
        (
            131,
            16,
            70,
            99,
            58,
            83,
            101,
            93,
            269230,
            [(172031, 320872, false), (284927, 333904, false), (344046, 285274, true), (90047, 283471, false), (40339, 170463, false), (258067, 283047, false)],
        ),
    ] {
        let mut groups = [61; 64];
        groups[16] = receiver;
        groups[24..32].fill(84);
        groups[31] += middle_tail;
        groups[48..50].copy_from_slice(&[first, first_end]);
        groups[52] = following;
        groups[60] = penultimate;
        groups[62..64].copy_from_slice(&[last, last_end]);
        check_batch_markers(&groups, seed, 1, &markers);
    }
}
