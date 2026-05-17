use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::model::{fmt_id, Task};

pub fn render(
    frame: &mut Frame,
    area: Rect,
    task: Option<&Task>,
    all_tasks: &[Task],
    focused: bool,
) {
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
            out.push(Line::from(""));
            if let Some(b) = &t.body {
                out.push(Line::from(Span::styled(
                    "## body",
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                for line in b.lines() {
                    out.push(Line::from(line.to_string()));
                }
                out.push(Line::from(""));
            }
            if let Some(a) = &t.acceptance {
                out.push(Line::from(Span::styled(
                    "## acceptance",
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                for line in a.lines() {
                    out.push(Line::from(line.to_string()));
                }
            }
            out
        }
    };

    let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    frame.render_widget(para, area);
}
