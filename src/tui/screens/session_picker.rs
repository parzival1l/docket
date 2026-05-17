use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::agent_session_info::{relative_ago, SessionInfo};
use crate::model::fmt_id;
use crate::tui::screens::SessionPickerState;

pub fn render(frame: &mut Frame, state: &SessionPickerState) {
    let n = state.sessions.len();
    let area = centered_rect(76, frame.area(), n);
    frame.render_widget(Clear, area);

    let mut lines: Vec<Line> = Vec::new();

    // Header: task context, then the prompt.
    lines.push(Line::from(vec![
        Span::styled(
            fmt_id(state.task_id),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            state.task_title.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        format!(
            "This task was worked on in {} sessions. Pick one to resume:",
            n
        ),
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    let now = chrono::Utc::now();
    for (i, info) in state.sessions.iter().enumerate() {
        let selected = i == state.cursor;
        let marker = if selected { "▶ " } else { "  " };
        let primary_style = if selected {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        };
        let dim_style = if selected {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        // Title line: marker + title (or fallback to id).
        let title_text = info
            .title
            .clone()
            .unwrap_or_else(|| short_id(&info.id).to_string());
        lines.push(Line::from(vec![
            Span::styled(marker, primary_style),
            Span::styled(title_text, primary_style),
        ]));

        // Meta line: id-prefix · turns · last_active (or linked_at fallback).
        let meta = build_meta_line(info, now);
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(meta, dim_style),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            "j/k",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" navigate  ·  "),
        Span::styled(
            "Enter",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" resume in tmux  ·  "),
        Span::styled(
            "Esc",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" cancel"),
    ]));

    let para = Paragraph::new(lines)
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" resume agent session ")
                .border_style(Style::default().fg(Color::Cyan)),
        );
    frame.render_widget(para, area);
}

fn build_meta_line(info: &SessionInfo, now: chrono::DateTime<chrono::Utc>) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(short_id(&info.id).to_string());
    if let Some(t) = info.turns {
        parts.push(format!("{} turns", t));
    }
    if let Some(last) = info.last_active {
        parts.push(format!("active {}", relative_ago(last, now)));
    } else if info.title.is_none() && info.turns.is_none() {
        // No transcript on disk — call that out and fall back to linked time.
        parts.push("no transcript".into());
        if let Some(linked) = info.linked_at {
            parts.push(format!("linked {}", relative_ago(linked, now)));
        }
    }
    parts.join("  ·  ")
}

/// First 8 chars — enough to disambiguate when full uuids would crowd the row.
fn short_id(id: &str) -> &str {
    let end = id
        .char_indices()
        .nth(8)
        .map(|(i, _)| i)
        .unwrap_or(id.len());
    &id[..end]
}

fn centered_rect(percent_x: u16, r: Rect, n_sessions: usize) -> Rect {
    // 3 header rows + (2 per session) + blank + hint + 2 borders.
    let body_rows = (n_sessions as u16).saturating_mul(2) + 3 + 1 + 1;
    let height = (body_rows + 2).max(9).min(r.height.saturating_sub(2));
    let pad_y = r.height.saturating_sub(height) / 2;
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(pad_y),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
