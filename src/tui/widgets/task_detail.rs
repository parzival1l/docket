use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::model::{fmt_id, Task};
use crate::tui::widgets::markdown;

fn section_header(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))
}

/// Renders the detail pane. `scroll` is the requested vertical offset in lines;
/// the function returns the clamped value actually used so the caller can
/// write it back (prevents runaway PgDn off the bottom).
pub fn render(
    frame: &mut Frame,
    area: Rect,
    task: Option<&Task>,
    all_tasks: &[Task],
    focused: bool,
    scroll: u16,
) -> u16 {
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" detail ");

    let lines: Vec<Line> = match task {
        None => vec![Line::from(Span::styled(
            "(no task selected)",
            Style::default().add_modifier(Modifier::DIM),
        ))],
        Some(t) => {
            let mut out: Vec<Line> = Vec::new();
            out.push(Line::from(vec![
                Span::styled(fmt_id(t.id), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::raw(t.title.clone()),
            ]));
            out.push(Line::from(vec![
                Span::raw("["),
                Span::raw(t.status.clone()),
                Span::raw("]  ["),
                Span::raw(t.kind.clone()),
                Span::raw("]  p"),
                Span::raw(t.priority.to_string()),
            ]));
            if let Some(g) = &t.group {
                out.push(Line::from(format!("group: {}", g)));
            }
            if !t.deps.is_empty() {
                let parts: Vec<String> = t
                    .deps
                    .iter()
                    .map(|d| match all_tasks.iter().find(|x| x.id == *d) {
                        Some(dep) => format!("{} ({})", fmt_id(*d), dep.status),
                        None => format!("{} (missing)", fmt_id(*d)),
                    })
                    .collect();
                out.push(Line::from(format!("deps: {}", parts.join(", "))));
            }
            if !t.agent_sessions.is_empty() {
                out.push(Line::from(format!(
                    "sessions: {}",
                    t.agent_sessions.join(", ")
                )));
            }
            out.push(Line::from(""));
            if let Some(b) = &t.body {
                out.push(section_header("▸ body"));
                out.extend(markdown::render_block(b));
                out.push(Line::from(""));
            }
            if let Some(a) = &t.acceptance {
                out.push(section_header("▸ acceptance"));
                out.extend(markdown::render_block(a));
            }
            out
        }
    };

    // Inner height is the area minus the top and bottom border rows.
    let viewport = area.height.saturating_sub(2);
    let content_height = lines.len() as u16;
    // Allow scrolling until the last line of content reaches the top of the
    // viewport — effectively (content - viewport).max(0). When content fits,
    // clamp is 0 so any positive scroll snaps back to zero on the next frame.
    let max_scroll = content_height.saturating_sub(viewport);
    let clamped = scroll.min(max_scroll);

    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((clamped, 0));
    frame.render_widget(para, area);
    clamped
}
