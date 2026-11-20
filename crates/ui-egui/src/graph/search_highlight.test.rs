use super::*;

fn marked(label: &str, query: &str) -> String {
    label.chars().zip(matched_letters(label, query)).map(|(c, m)| if m { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() }).collect()
}

#[test]
fn matched_words_are_flagged_case_insensitively() {
    assert_eq!(marked("Gaussian Blur", "blur"), "gaussian BLUR");
    assert_eq!(marked("Gaussian Blur", "GAU blu"), "GAUssian BLUr");
}

#[test]
fn nothing_is_flagged_for_an_empty_or_missing_query() {
    assert_eq!(marked("Blur", ""), "blur");
    assert_eq!(marked("Blur", "zzz"), "blur");
}

#[test]
fn flags_cover_every_character_even_when_the_query_is_longer() {
    assert_eq!(matched_letters("Blur", "blur blur blur blur").len(), 4);
    assert_eq!(matched_letters("日本 ノード", "ノ").len(), 6);
}

#[test]
fn the_styled_text_keeps_the_label_and_colours_matches() {
    let base = Color32::GRAY;
    let hi = Color32::RED;
    let job = highlighted_label("Gaussian Blur", "blur", FontId::default(), base, hi);
    assert_eq!(job.text, "Gaussian Blur");
    let colours: Vec<Color32> = job.sections.iter().map(|s| s.format.color).collect();
    assert_eq!(colours, vec![base, hi]);
}
