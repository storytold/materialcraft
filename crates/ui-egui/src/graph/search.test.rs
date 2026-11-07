use super::*;

#[test]
fn escape_clears_text_and_only_closes_an_empty_box() {
    assert!(escape_clears("blur"));
    assert!(escape_clears(" "));
    assert!(!escape_clears(""));
}
