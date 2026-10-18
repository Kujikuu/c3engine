//! A code editor for scripts: Rhai syntax colouring, a line-number gutter that marks lines
//! with problems, and helpers to move the cursor or insert at it.

use crate::theme;
use c3_script::{Diagnostic, Severity};
use eframe::egui::{self, Color32, FontId, TextFormat, text::LayoutJob};

const KEYWORD: Color32 = Color32::from_rgb(198, 120, 221);
const STRING: Color32 = Color32::from_rgb(152, 195, 121);
const NUMBER: Color32 = Color32::from_rgb(209, 154, 102);
const COMMENT: Color32 = Color32::from_gray(110);
/// Calls of the engine's script functions.
const API_CALL: Color32 = Color32::from_rgb(97, 175, 239);
/// Calls of the script's own functions.
const CALL: Color32 = Color32::from_rgb(229, 192, 123);
const EDITOR_FILL: Color32 = Color32::from_gray(24);
const GUTTER_FILL: Color32 = Color32::from_gray(31);

const KEYWORDS: &[&str] = &[
    "fn", "let", "const", "if", "else", "while", "loop", "for", "in", "do", "until", "return",
    "break", "continue", "switch", "true", "false", "throw", "try", "catch", "private", "this",
    "import", "export", "as", "global",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    Plain,
    Keyword,
    String,
    Number,
    Comment,
    ApiCall,
    Call,
}

impl Token {
    fn color(self) -> Color32 {
        match self {
            Token::Plain => theme::TEXT,
            Token::Keyword => KEYWORD,
            Token::String => STRING,
            Token::Number => NUMBER,
            Token::Comment => COMMENT,
            Token::ApiCall => API_CALL,
            Token::Call => CALL,
        }
    }
}

/// Whether `name` is one of the engine's script functions.
fn is_api(name: &str) -> bool {
    c3_script::API
        .iter()
        .any(|(sig, _)| sig.split('(').next() == Some(name))
}

/// Splits `text` into coloured runs (byte ranges).
fn tokens(text: &str) -> Vec<(std::ops::Range<usize>, Token)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut plain = 0;
    let push = |out: &mut Vec<_>, plain: &mut usize, start: usize, end: usize, t: Token| {
        if *plain < start {
            out.push((*plain..start, Token::Plain));
        }
        out.push((start..end, t));
        *plain = end;
    };
    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        if text[i..].starts_with("//") {
            i = text[i..].find('\n').map_or(text.len(), |n| i + n);
            push(&mut out, &mut plain, start, i, Token::Comment);
        } else if text[i..].starts_with("/*") {
            i = text[i + 2..]
                .find("*/")
                .map_or(text.len(), |n| i + 2 + n + 2);
            push(&mut out, &mut plain, start, i, Token::Comment);
        } else if c == b'"' || c == b'`' || c == b'\'' {
            i += 1;
            while i < bytes.len() && bytes[i] != c {
                i += if bytes[i] == b'\\' { 2 } else { 1 };
            }
            i = (i + 1).min(bytes.len());
            push(&mut out, &mut plain, start, i, Token::String);
        } else if c.is_ascii_digit() {
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.')
            {
                i += 1;
            }
            push(&mut out, &mut plain, start, i, Token::Number);
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &text[start..i];
            let called = text[i..].trim_start_matches([' ', '\t']).starts_with('(');
            let token = if KEYWORDS.contains(&word) {
                Token::Keyword
            } else if called && is_api(word) {
                Token::ApiCall
            } else if called {
                Token::Call
            } else {
                continue;
            };
            push(&mut out, &mut plain, start, i, token);
        } else {
            // Step over one character (it may be several bytes).
            i += text[i..].chars().next().map_or(1, char::len_utf8);
        }
    }
    if plain < text.len() {
        out.push((plain..text.len(), Token::Plain));
    }
    out
}

/// `text` laid out with syntax colours, unwrapped.
fn highlight(text: &str, font: &FontId) -> LayoutJob {
    let mut job = LayoutJob::default();
    for (range, token) in tokens(text) {
        job.append(
            &text[range],
            0.0,
            TextFormat::simple(font.clone(), token.color()),
        );
    }
    job.wrap.max_width = f32::INFINITY;
    job
}

/// Shows `text` as an editable script with a gutter; returns whether it changed.
/// Lines named in `problems` are marked in the gutter (hover for the message).
pub fn code_editor(
    ui: &mut egui::Ui,
    id: egui::Id,
    text: &mut String,
    problems: &[Diagnostic],
) -> bool {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let row_height = ui.fonts_mut(|f| f.row_height(&font));
    let mut changed = false;
    egui::Frame::new().fill(EDITOR_FILL).show(ui, |ui| {
        egui::ScrollArea::both()
            .id_salt(id.with("scroll"))
            .auto_shrink(false)
            .show(ui, |ui| {
                // The gutter runs down the whole view, even below a short script.
                let height = ui.available_height();
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    // A text ending in a newline has an empty last line the cursor can be on.
                    let lines = text.split('\n').count();
                    gutter(ui, lines, row_height, height, &font, problems);
                    let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, _w: f32| {
                        ui.fonts_mut(|f| f.layout_job(highlight(buf.as_str(), &font)))
                    };
                    let edit = egui::TextEdit::multiline(text)
                        .id(id)
                        .code_editor()
                        .frame(egui::Frame::NONE)
                        .margin(egui::Margin::symmetric(8, 0))
                        .desired_width(f32::INFINITY)
                        .desired_rows(30)
                        .layouter(&mut layouter);
                    changed = ui.add(edit).changed();
                });
            });
    });
    changed
}

/// Line numbers, the lines with problems in their colour.
fn gutter(
    ui: &mut egui::Ui,
    lines: usize,
    row_height: f32,
    min_height: f32,
    font: &FontId,
    problems: &[Diagnostic],
) {
    let digits = lines.to_string().len().max(3);
    let width = ui.fonts_mut(|f| f.glyph_width(font, '0')) * digits as f32 + 16.0;
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(width, (row_height * lines as f32).max(min_height)),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, GUTTER_FILL);
    let mut hovered = None;
    for line in 1..=lines {
        let problem = problems.iter().find(|p| p.line == line);
        let color = match problem.map(|p| p.severity) {
            Some(Severity::Error) => theme::DANGER,
            Some(Severity::Warning) => theme::WARNING,
            None => theme::TEXT_DIM.gamma_multiply(0.7),
        };
        let top = rect.top() + row_height * (line - 1) as f32;
        if problem.is_some() {
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(rect.left(), top),
                    egui::vec2(3.0, row_height),
                ),
                0.0,
                color,
            );
        }
        painter.text(
            egui::pos2(rect.right() - 8.0, top),
            egui::Align2::RIGHT_TOP,
            line.to_string(),
            font.clone(),
            color,
        );
        if let Some(p) = problem
            && resp
                .hover_pos()
                .is_some_and(|h| (top..top + row_height).contains(&h.y))
        {
            hovered = Some(p.message.clone());
        }
    }
    if let Some(message) = hovered {
        resp.on_hover_text(message);
    }
}

/// Puts the cursor of editor `id` at the start of 1-based `line` and focuses it.
pub fn go_to_line(ctx: &egui::Context, id: egui::Id, text: &str, line: usize) {
    let index: usize = text
        .split_inclusive('\n')
        .take(line.saturating_sub(1))
        .map(|l| l.chars().count())
        .sum();
    set_cursor(ctx, id, index);
}

/// Inserts `snippet` at the cursor of editor `id` (or the end), leaving the cursor after it.
pub fn insert_at_cursor(ctx: &egui::Context, id: egui::Id, text: &mut String, snippet: &str) {
    let chars = text.chars().count();
    let at = egui::TextEdit::load_state(ctx, id)
        .and_then(|s| s.cursor.char_range())
        .map_or(chars, |r| r.as_sorted_char_range().start.0.min(chars));
    let byte = text.char_indices().nth(at).map_or(text.len(), |(b, _)| b);
    text.insert_str(byte, snippet);
    set_cursor(ctx, id, at + snippet.chars().count());
}

fn set_cursor(ctx: &egui::Context, id: egui::Id, index: usize) {
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    let cursor = egui::text::CCursor::new(index);
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(cursor)));
    state.store(ctx, id);
    ctx.memory_mut(|m| m.request_focus(id));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_cover_the_text_in_order() {
        let text = "fn talk() { // hi\n  text(\"a \\\" b\"); give(12); x }";
        let toks = tokens(text);
        let mut at = 0;
        for (r, _) in &toks {
            assert_eq!(r.start, at);
            at = r.end;
        }
        assert_eq!(at, text.len());
        let kind = |s: &str| {
            toks.iter()
                .find(|(r, _)| &text[r.clone()] == s)
                .map(|t| t.1)
        };
        assert_eq!(kind("fn"), Some(Token::Keyword));
        assert_eq!(kind("talk"), Some(Token::Call));
        assert_eq!(kind("text"), Some(Token::ApiCall));
        assert_eq!(kind("\"a \\\" b\""), Some(Token::String));
        assert_eq!(kind("12"), Some(Token::Number));
        assert_eq!(kind("// hi"), Some(Token::Comment));
        let text = "give(global::A)";
        let toks = tokens(text);
        assert!(toks.contains(&(5..11, Token::Keyword)), "{toks:?}");
    }
}
