//! Small markdown-to-ratatui renderer used by the detail pane.
//!
//! Not CommonMark. We support exactly what task body/acceptance text needs:
//! ATX headings, bullet lists (with one level of nesting), fenced code
//! blocks, and three inline forms — `**bold**`, `*italic*`, and `` `code` ``.
//! Unmatched markers fall back to raw text rather than swallowing the rest
//! of the line.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

pub fn render_block(input: &str) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    let mut in_fence = false;

    for raw in input.lines() {
        if raw.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue; // hide the fence markers themselves
        }
        if in_fence {
            out.push(Line::from(Span::styled(
                raw.to_string(),
                Style::default().fg(Color::LightMagenta),
            )));
            continue;
        }

        let trimmed_start = raw.trim_start();

        // Headings (#, ##, ###). Only at column 0 after optional whitespace.
        if let Some(rest) = trimmed_start.strip_prefix("### ") {
            out.push(Line::from(Span::styled(
                rest.to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if let Some(rest) = trimmed_start.strip_prefix("## ") {
            out.push(Line::from(Span::styled(
                rest.to_string(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if let Some(rest) = trimmed_start.strip_prefix("# ") {
            out.push(Line::from(Span::styled(
                rest.to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }

        // Bullets. Nested bullets require ≥2 leading spaces.
        let indent = raw.len() - trimmed_start.len();
        if let Some(rest) = trimmed_start
            .strip_prefix("- ")
            .or_else(|| trimmed_start.strip_prefix("* "))
        {
            let (lead, glyph) = if indent >= 2 {
                ("    ", "◦ ")
            } else {
                ("  ", "• ")
            };
            let mut spans: Vec<Span<'static>> = Vec::new();
            spans.push(Span::raw(lead.to_string()));
            spans.push(Span::styled(
                glyph.to_string(),
                Style::default().add_modifier(Modifier::DIM),
            ));
            spans.extend(inline_spans(rest));
            out.push(Line::from(spans));
            continue;
        }

        if raw.trim().is_empty() {
            out.push(Line::from(""));
            continue;
        }

        // Plain paragraph line — still gets inline markers expanded.
        out.push(Line::from(inline_spans(raw)));
    }

    out
}

/// Walk a single line, emitting spans for inline `**bold**`, `*italic*`, and
/// `` `code` ``. Unmatched markers (no closing token before end-of-line) are
/// emitted as literal characters so the user sees their source rather than
/// losing trailing content.
fn inline_spans(s: &str) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut buf = String::new();

    let flush = |buf: &mut String, spans: &mut Vec<Span<'static>>| {
        if !buf.is_empty() {
            spans.push(Span::raw(std::mem::take(buf)));
        }
    };

    while i < bytes.len() {
        // `code`
        if bytes[i] == b'`' {
            if let Some(end) = find_unescaped(s, i + 1, b'`') {
                flush(&mut buf, &mut spans);
                let inner = &s[i + 1..end];
                spans.push(Span::styled(
                    inner.to_string(),
                    Style::default().fg(Color::LightMagenta),
                ));
                i = end + 1;
                continue;
            }
        }
        // **bold**
        if i + 1 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'*' {
            if let Some(end) = find_double_star(s, i + 2) {
                flush(&mut buf, &mut spans);
                let inner = &s[i + 2..end];
                spans.push(Span::styled(
                    inner.to_string(),
                    Style::default().add_modifier(Modifier::BOLD),
                ));
                i = end + 2;
                continue;
            }
        }
        // *italic* — inner must be non-empty so we don't treat the two stars
        // of a stray `**` as italic-wrapping-nothing.
        if bytes[i] == b'*' {
            if let Some(end) = find_unescaped(s, i + 1, b'*') {
                if end > i + 1 {
                    flush(&mut buf, &mut spans);
                    let inner = &s[i + 1..end];
                    spans.push(Span::styled(
                        inner.to_string(),
                        Style::default().add_modifier(Modifier::ITALIC),
                    ));
                    i = end + 1;
                    continue;
                }
            }
        }

        // Default: accumulate this character into the running plain buffer.
        // Safe to take one byte at a time because we only check ASCII markers.
        buf.push(bytes[i] as char);
        i += 1;
    }

    flush(&mut buf, &mut spans);
    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }
    spans
}

fn find_unescaped(s: &str, from: usize, marker: u8) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == marker {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_double_star(s: &str, from: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = from;
    while i + 1 < bytes.len() {
        if bytes[i] == b'*' && bytes[i + 1] == b'*' {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn heading_one_renders_bold_cyan() {
        let out = render_block("# Title");
        assert_eq!(out.len(), 1);
        let span = &out[0].spans[0];
        assert_eq!(span.content, "Title");
        assert_eq!(span.style.fg, Some(Color::Cyan));
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn heading_two_renders_bold_yellow() {
        let out = render_block("## Section");
        let span = &out[0].spans[0];
        assert_eq!(span.content, "Section");
        assert_eq!(span.style.fg, Some(Color::Yellow));
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn heading_three_renders_bold() {
        let out = render_block("### sub");
        let span = &out[0].spans[0];
        assert_eq!(span.content, "sub");
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn bullet_renders_with_bullet_glyph() {
        let out = render_block("- item");
        assert_eq!(out.len(), 1);
        let joined = line_text(&out[0]);
        assert!(joined.contains("•"), "got {:?}", joined);
        assert!(joined.contains("item"));
        assert!(!joined.contains("- "));
    }

    #[test]
    fn nested_bullet_uses_open_circle() {
        let out = render_block("  - nested");
        let joined = line_text(&out[0]);
        assert!(joined.contains("◦"), "got {:?}", joined);
    }

    #[test]
    fn star_bullet_also_renders() {
        let out = render_block("* alt");
        let joined = line_text(&out[0]);
        assert!(joined.contains("•"));
        assert!(joined.contains("alt"));
        assert!(!joined.contains("*"));
    }

    #[test]
    fn bold_inline_emits_bold_span() {
        let out = render_block("hello **world** end");
        let line = &out[0];
        let bold = line
            .spans
            .iter()
            .find(|s| s.style.add_modifier.contains(Modifier::BOLD))
            .expect("a bold span");
        assert_eq!(bold.content, "world");
        // The literal ** marker is not in the assembled text.
        assert!(!line_text(line).contains("**"));
    }

    #[test]
    fn italic_inline_emits_italic_span() {
        let out = render_block("a *quiet* word");
        let italic = out[0]
            .spans
            .iter()
            .find(|s| s.style.add_modifier.contains(Modifier::ITALIC))
            .expect("an italic span");
        assert_eq!(italic.content, "quiet");
    }

    #[test]
    fn inline_code_emits_styled_span() {
        let out = render_block("use `cargo test`");
        let code = out[0]
            .spans
            .iter()
            .find(|s| s.style.fg == Some(Color::LightMagenta))
            .expect("a code span");
        assert_eq!(code.content, "cargo test");
        assert!(!line_text(&out[0]).contains("`"));
    }

    #[test]
    fn unmatched_marker_falls_back_to_raw_text() {
        let out = render_block("trailing ** marker");
        let joined = line_text(&out[0]);
        assert!(
            joined.contains("**"),
            "unmatched ** should remain in output: {:?}",
            joined
        );
        assert!(!out[0]
            .spans
            .iter()
            .any(|s| s.style.add_modifier.contains(Modifier::BOLD)));
    }

    #[test]
    fn fenced_code_block_preserves_lines_and_hides_fences() {
        let input = "before\n```\nlet x = 1;\nlet y = 2;\n```\nafter";
        let out = render_block(input);
        let texts: Vec<String> = out.iter().map(line_text).collect();
        assert!(texts.iter().any(|t| t == "let x = 1;"));
        assert!(texts.iter().any(|t| t == "let y = 2;"));
        assert!(!texts.iter().any(|t| t.contains("```")));
    }

    #[test]
    fn blank_line_is_preserved() {
        let out = render_block("one\n\ntwo");
        assert_eq!(out.len(), 3);
        assert_eq!(line_text(&out[1]), "");
    }

    #[test]
    fn bullet_with_inline_bold_renders_both() {
        let out = render_block("- task with **emphasis**");
        let joined = line_text(&out[0]);
        assert!(joined.contains("•"));
        assert!(joined.contains("emphasis"));
        assert!(!joined.contains("**"));
    }
}
