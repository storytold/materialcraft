use materialcraft_formats::{Depth, decode, encode};
use materialcraft_image::{Image, Kind};

#[test]
fn exr_round_trip_keeps_alpha() {
    let img = Image::filled(4, 4, Kind::Color, [0.2, 0.4, 0.6, 0.5]);
    let bytes = encode(&img, "exr", Depth::F32, false).unwrap();
    let back = decode(&bytes, true).unwrap();
    let p = &back.data[0..4];
    assert!((p[0] - 0.2).abs() < 1e-3 && (p[1] - 0.4).abs() < 1e-3 && (p[2] - 0.6).abs() < 1e-3, "{p:?}");
    assert!((p[3] - 0.5).abs() < 1e-3, "alpha after EXR round trip: {}", p[3]);
}
