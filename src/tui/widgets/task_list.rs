use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::model::{fmt_id, Task};

fn status_style(status: &str) -> Style {
    match status {
        "open" => Style::default(),
        "in_progress" => Style::default().fg(Color::Yellow),
        "done" => Style::default().fg(Color::Green).add_modifier(Modifier::DIM),
        _ => Style::default().fg(Color::Magenta),
    }
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    tasks: &[Task],
    visible: &[usize],
    cursor: usize,
    focused: bool,
) {
    let items: Vec<ListItem> = visible
        .iter()
        .map(|i| {
            let t = &tasks[*i];
            let group_suffix = t
                .group
                .as_deref()
                .map(|g| format!(" [{}]", g))
                .unwrap_or_default();
            let line = Line::from(vec![
                Span::raw(format!("{:<6}", fmt_id(t.id))),
                Span::styled(format!("{:<12}", t.status), status_style(&t.status)),
                Span::raw(format!("p{} ", t.priority)),
                Span::raw(t.title.clone()),
                Span::styled(group_suffix, Style::default().add_modifier(Modifier::DIM)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" tasks ");
    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );

    let mut state = ListState::default();
    if !visible.is_empty() {
        state.select(Some(cursor.min(visible.len() - 1)));
    }
    frame.render_stateful_widget(list, area, &mut state);
}
