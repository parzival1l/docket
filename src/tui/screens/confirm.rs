use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::model::fmt_id;
use crate::tui::screens::PendingAction;

pub fn render(frame: &mut Frame, action: &PendingAction) {
    let (title, prompt) = match action {
        PendingAction::DeleteTask { id, title } => (
            " delete task ".to_string(),
            format!("Delete {} \"{}\"?", fmt_id(*id), title),
        ),
        PendingAction::DiscardEdits => (
            " discard edits ".to_string(),
            "Discard unsaved changes?".to_string(),
        ),
    };

    let area = centered_rect(60, frame.area());
    frame.render_widget(Clear, area);

    let lines = vec![
        Line::from(Span::styled(
            prompt,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "This cannot be undone.",
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::DIM),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "y/Enter",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" confirm  ·  "),
            Span::styled(
                "n/Esc",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" cancel"),
        ]),
    ];

    let para = Paragraph::new(lines)
        .alignment(Alignment::Left)
        .block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(para, area);
}

fn centered_rect(percent_x: u16, r: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(9),
            Constraint::Percentage(40),
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
