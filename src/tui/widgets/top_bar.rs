use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::filters::Filters;

pub fn render(frame: &mut Frame, area: Rect, filters: &Filters) {
    let mut spans: Vec<Span> = vec![Span::styled(
        " docket ",
        Style::default().add_modifier(Modifier::BOLD),
    )];

    let chip_style = Style::default().fg(Color::Black).bg(Color::Cyan);

    let mut add_chip = |label: String| {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(format!(" {} ", label), chip_style));
    };

    if let Some(s) = &filters.status {
        add_chip(format!("status:{}", s));
    }
    if let Some(g) = &filters.group {
        add_chip(format!("group:{}", g));
    }
    if let Some(p) = filters.priority_cap {
        add_chip(format!("p≤{}", p));
    }
    if filters.ready_only {
        add_chip("ready".into());
    }
    if filters.blocked_only {
        add_chip("blocked".into());
    }
    if let Some(t) = &filters.text {
        add_chip(format!("/{}", t));
    }

    let para = Paragraph::new(Line::from(spans));
    frame.render_widget(para, area);
}
