use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::keybindings::{footer_for, Scope};

pub fn render(frame: &mut Frame, area: Rect, scope: Scope) {
    let bindings = footer_for(scope);
    let mut spans: Vec<Span> = Vec::new();
    for (i, b) in bindings.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(
                " · ",
                Style::default().add_modifier(Modifier::DIM),
            ));
        }
        spans.push(Span::styled(
            b.keys,
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::raw(b.label));
    }
    let para = Paragraph::new(Line::from(spans));
    frame.render_widget(para, area);
}
