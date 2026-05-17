use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::keybindings::{Scope, COMMAND_REGISTRY};

pub fn render(frame: &mut Frame) {
    let area = centered_rect(60, 85, frame.area());
    frame.render_widget(Clear, area);

    let mut lines: Vec<Line> = Vec::new();
    for scope in [
        Scope::Global,
        Scope::List,
        Scope::Detail,
        Scope::FilterPrompt,
        Scope::Confirm,
        Scope::Edit,
        Scope::SessionPicker,
    ] {
        let scope_label = match scope {
            Scope::Global => "Global",
            Scope::List => "List",
            Scope::Detail => "Detail",
            Scope::Help => "Help",
            Scope::FilterPrompt => "Filter prompt",
            Scope::Confirm => "Confirm",
            Scope::Edit => "Edit form",
            Scope::SessionPicker => "Open session picker",
        };
        lines.push(Line::from(Span::styled(
            scope_label.to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        for b in COMMAND_REGISTRY.iter().filter(|b| b.scope == scope) {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{:<14}", b.keys),
                    Style::default().fg(Color::Yellow),
                ),
                Span::raw(b.label),
            ]));
        }
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        "Press ? or Esc to close",
        Style::default().add_modifier(Modifier::DIM),
    )));

    let para = Paragraph::new(lines)
        .alignment(Alignment::Left)
        .block(Block::default().borders(Borders::ALL).title(" help "));
    frame.render_widget(para, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
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
