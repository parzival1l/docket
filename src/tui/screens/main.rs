use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use crate::tui::app::{App, Pane};
use crate::tui::keybindings::Scope;
use crate::tui::widgets::{footer, task_detail, task_list, top_bar};

pub fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    top_bar::render(frame, rows[0], &app.filters);

    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(40), Constraint::Min(20)])
        .split(rows[1]);

    let visible = app.visible_indices();
    task_list::render(
        frame,
        panes[0],
        &app.tasks,
        &visible,
        app.cursor,
        app.focus == Pane::List,
    );
    let clamped = task_detail::render(
        frame,
        panes[1],
        app.selected_task(),
        &app.tasks,
        app.focus == Pane::Detail,
        app.detail_scroll,
    );
    app.detail_scroll = clamped;

    let scope = match app.focus {
        Pane::List => Scope::List,
        Pane::Detail => Scope::Detail,
    };
    footer::render(frame, rows[2], scope);
}
