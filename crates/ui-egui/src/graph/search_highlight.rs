//! Which letters of a result label the search words matched, so the popup can emphasise them.

use egui::{Color32, FontId, Stroke, text::LayoutJob};

/// Matched letters are underlined as well as recoloured, so they stand out on the accent-coloured selected row too.
const UNDERLINE_WIDTH: f32 = 1.0;

/// One flag per character of `label`: true where a search word appears in it (ASCII case-insensitive).
pub(super) fn matched_letters(label: &str, query: &str) -> Vec<bool> {
    let chars: Vec<char> = label.chars().map(|c| c.to_ascii_lowercase()).collect();
    let mut flags = vec![false; chars.len()];
    for word in query.split_whitespace() {
        let word: Vec<char> = word.chars().map(|c| c.to_ascii_lowercase()).collect();
        let Some(at) = chars.windows(word.len().max(1)).position(|w| w == word.as_slice()) else { continue };
        for flag in flags.iter_mut().skip(at).take(word.len()) {
            *flag = true;
        }
    }
    flags
}

/// The label as styled text, with matched letters underlined in `highlight` and the rest in `base`.
pub(super) fn highlighted_label(label: &str, query: &str, font: FontId, base: Color32, highlight: Color32) -> LayoutJob {
    let flags = matched_letters(label, query);
    let mut job = LayoutJob::default();
    let mut run = String::new();
    let mut run_matched = false;
    let push = |job: &mut LayoutJob, run: &mut String, matched: bool| {
        if !run.is_empty() {
            let (color, underline) = if matched { (highlight, Stroke::new(UNDERLINE_WIDTH, highlight)) } else { (base, Stroke::NONE) };
            job.append(run, 0.0, egui::TextFormat { font_id: font.clone(), color, underline, ..Default::default() });
            run.clear();
        }
    };
    for (c, matched) in label.chars().zip(flags) {
        if matched != run_matched {
            push(&mut job, &mut run, run_matched);
            run_matched = matched;
        }
        run.push(c);
    }
    push(&mut job, &mut run, run_matched);
    job
}

#[cfg(test)]
#[path = "search_highlight.test.rs"]
mod tests;
