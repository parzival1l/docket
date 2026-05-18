#![allow(dead_code)] // consumed by App handlers/renderer in PR-5 Tasks 4-10

use tui_input::Input;
use tui_textarea::TextArea;

use crate::model::{fmt_id, Task, KINDS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMode {
    Add,
    Edit { id: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    Title,
    Priority,
    Kind,
    Group,
    Deps,
    /// Comma-separated linked agent session ids. Only navigable in Edit mode —
    /// linking requires a persisted task id.
    Sessions,
    Body,
    Acceptance,
}

const FIELD_ORDER_ADD: [EditField; 7] = [
    EditField::Title,
    EditField::Priority,
    EditField::Kind,
    EditField::Group,
    EditField::Deps,
    EditField::Body,
    EditField::Acceptance,
];

const FIELD_ORDER_EDIT: [EditField; 8] = [
    EditField::Title,
    EditField::Priority,
    EditField::Kind,
    EditField::Group,
    EditField::Deps,
    EditField::Sessions,
    EditField::Body,
    EditField::Acceptance,
];

pub struct EditState {
    pub mode: EditMode,
    pub field: EditField,
    pub title: Input,
    pub priority: Input,
    pub kind: String,
    pub group: Input,
    pub deps: Input,
    /// Comma-separated linked agent session ids. Editable in Edit mode; the
    /// save flow diffs against `orig_sessions` and calls `link_session` /
    /// `unlink_session` for the additions and removals.
    pub sessions: Input,
    pub body: TextArea<'static>,
    pub acceptance: TextArea<'static>,
    pub error: Option<String>,
    orig_title: String,
    orig_priority: String,
    orig_kind: String,
    orig_group: String,
    orig_deps: String,
    orig_sessions: String,
    orig_body: String,
    orig_acceptance: String,
}

impl std::fmt::Debug for EditState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditState")
            .field("mode", &self.mode)
            .field("field", &self.field)
            .field("title", &self.title.value())
            .field("priority", &self.priority.value())
            .field("kind", &self.kind)
            .field("group", &self.group.value())
            .field("deps", &self.deps.value())
            .field("sessions", &self.sessions.value())
            .field("body", &self.body.lines().join("\n"))
            .field("acceptance", &self.acceptance.lines().join("\n"))
            .field("error", &self.error)
            .finish()
    }
}

impl EditState {
    pub fn for_add() -> Self {
        Self::build(EditMode::Add, "", "", "feature", "", "", "", "", "")
    }

    pub fn for_edit(task: &Task) -> Self {
        let deps_str = task
            .deps
            .iter()
            .map(|d| fmt_id(*d))
            .collect::<Vec<_>>()
            .join(", ");
        let sessions_str = task.agent_sessions.join(", ");
        Self::build(
            EditMode::Edit { id: task.id },
            &task.title,
            &task.priority.to_string(),
            &task.kind,
            task.group.as_deref().unwrap_or(""),
            &deps_str,
            task.body.as_deref().unwrap_or(""),
            task.acceptance.as_deref().unwrap_or(""),
            &sessions_str,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        mode: EditMode,
        title: &str,
        priority: &str,
        kind: &str,
        group: &str,
        deps: &str,
        body: &str,
        acceptance: &str,
        sessions: &str,
    ) -> Self {
        let body_lines: Vec<String> = if body.is_empty() {
            vec![String::new()]
        } else {
            body.lines().map(|s| s.to_string()).collect()
        };
        let acc_lines: Vec<String> = if acceptance.is_empty() {
            vec![String::new()]
        } else {
            acceptance.lines().map(|s| s.to_string()).collect()
        };
        let mut body_ta = TextArea::new(body_lines);
        body_ta.set_placeholder_text("Markdown body (optional)");
        let mut acc_ta = TextArea::new(acc_lines);
        acc_ta.set_placeholder_text("- bullet criteria (optional)");
        Self {
            mode,
            field: EditField::Title,
            title: Input::default().with_value(title.to_string()),
            priority: Input::default().with_value(priority.to_string()),
            kind: kind.to_string(),
            group: Input::default().with_value(group.to_string()),
            deps: Input::default().with_value(deps.to_string()),
            sessions: Input::default().with_value(sessions.to_string()),
            body: body_ta,
            acceptance: acc_ta,
            error: None,
            orig_title: title.to_string(),
            orig_priority: priority.to_string(),
            orig_kind: kind.to_string(),
            orig_group: group.to_string(),
            orig_deps: deps.to_string(),
            orig_sessions: sessions.to_string(),
            orig_body: body.to_string(),
            orig_acceptance: acceptance.to_string(),
        }
    }

    fn field_order(&self) -> &'static [EditField] {
        match self.mode {
            EditMode::Add => &FIELD_ORDER_ADD,
            EditMode::Edit { .. } => &FIELD_ORDER_EDIT,
        }
    }

    /// Parsed, deduplicated session ids from the current input value. Empty
    /// entries are dropped; order is preserved (first occurrence wins).
    pub fn sessions_list(&self) -> Vec<String> {
        parse_sessions(self.sessions.value())
    }

    /// Original session ids the form was opened with — used by the save path
    /// to compute additions vs removals against the linked-session table.
    pub fn orig_sessions_list(&self) -> Vec<String> {
        parse_sessions(&self.orig_sessions)
    }

    /// Cycle to the next kind in the vocabulary. Wraps around.
    pub fn cycle_kind_forward(&mut self) {
        let i = KINDS.iter().position(|k| *k == self.kind).unwrap_or(0);
        self.kind = KINDS[(i + 1) % KINDS.len()].to_string();
    }

    /// Cycle to the previous kind in the vocabulary. Wraps around.
    pub fn cycle_kind_backward(&mut self) {
        let i = KINDS.iter().position(|k| *k == self.kind).unwrap_or(0);
        self.kind = KINDS[(i + KINDS.len() - 1) % KINDS.len()].to_string();
    }

    /// Live, per-field validation. Returns the error for the currently
    /// focused field only (or None if it's valid in isolation). Distinct
    /// from `validate()` which checks all fields at save time.
    pub fn current_field_error(&self) -> Option<String> {
        match self.field {
            EditField::Title => {
                if self.title.value().trim().is_empty() {
                    Some("title is required".into())
                } else {
                    None
                }
            }
            EditField::Priority => {
                let raw = self.priority.value().trim();
                if raw.is_empty() {
                    None
                } else {
                    match raw.parse::<i32>() {
                        Ok(p) if (0..=4).contains(&p) => None,
                        Ok(_) => Some("priority must be 0..=4".into()),
                        Err(_) => Some("priority must be an integer".into()),
                    }
                }
            }
            EditField::Deps => {
                let raw = self.deps.value().trim();
                if raw.is_empty() {
                    None
                } else {
                    crate::model::parse_deps(Some(raw.to_string()))
                        .err()
                        .map(|e| format!("deps: {}", e))
                }
            }
            EditField::Kind
            | EditField::Group
            | EditField::Sessions
            | EditField::Body
            | EditField::Acceptance => None,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.title.value() != self.orig_title
            || self.priority.value() != self.orig_priority
            || self.kind != self.orig_kind
            || self.group.value() != self.orig_group
            || self.deps.value() != self.orig_deps
            || self.sessions.value() != self.orig_sessions
            || self.body.lines().join("\n") != self.orig_body
            || self.acceptance.lines().join("\n") != self.orig_acceptance
    }

    pub fn next_field(&mut self) {
        let order = self.field_order();
        let idx = order.iter().position(|f| *f == self.field).unwrap_or(0);
        self.field = order[(idx + 1) % order.len()];
    }

    pub fn prev_field(&mut self) {
        let order = self.field_order();
        let idx = order.iter().position(|f| *f == self.field).unwrap_or(0);
        self.field = order[(idx + order.len() - 1) % order.len()];
    }
}

fn parse_sessions(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in raw.split(',') {
        let s = part.trim();
        if s.is_empty() {
            continue;
        }
        if !out.iter().any(|e| e == s) {
            out.push(s.to_string());
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedForm {
    pub title: String,
    pub body: Option<String>,
    pub acceptance: Option<String>,
    pub deps_json: Option<String>,
    pub priority: i32,
    pub group: Option<String>,
    pub kind: String,
}

impl EditState {
    /// Pure: reads field text only. Returns the validated form or a single
    /// error message suitable for the modal's status line.
    pub fn validate(&self) -> Result<ValidatedForm, String> {
        let title = self.title.value().trim();
        if title.is_empty() {
            return Err("title is required".into());
        }

        let priority_raw = self.priority.value().trim();
        let priority = if priority_raw.is_empty() {
            2
        } else {
            match priority_raw.parse::<i32>() {
                Ok(p) if (0..=4).contains(&p) => p,
                Ok(_) => return Err("priority must be between 0 and 4".into()),
                Err(_) => return Err("priority must be an integer 0..=4".into()),
            }
        };

        let group_raw = self.group.value().trim();
        let group = if group_raw.is_empty() {
            None
        } else {
            Some(group_raw.to_string())
        };

        let deps_raw = self.deps.value().trim();
        let deps_json = if deps_raw.is_empty() {
            None
        } else {
            crate::model::parse_deps(Some(deps_raw.to_string()))
                .map_err(|e| format!("deps: {}", e))?
        };

        let body_joined = self.body.lines().join("\n");
        let acceptance_joined = self.acceptance.lines().join("\n");
        let body = if body_joined.trim().is_empty() {
            None
        } else {
            Some(body_joined)
        };
        let acceptance = if acceptance_joined.trim().is_empty() {
            None
        } else {
            Some(acceptance_joined)
        };

        Ok(ValidatedForm {
            title: title.to_string(),
            body,
            acceptance,
            deps_json,
            priority,
            group,
            kind: self.kind.clone(),
        })
    }
}

pub fn render(frame: &mut ratatui::Frame, state: &EditState) {
    use ratatui::layout::{Alignment, Constraint, Direction, Layout};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Clear, Paragraph};

    let show_sessions_row = matches!(state.mode, EditMode::Edit { .. });
    let modal_h: u16 = if show_sessions_row { 32 } else { 30 };
    let area = centered_rect(80, modal_h, frame.area());
    frame.render_widget(Clear, area);

    let title_bar = match state.mode {
        EditMode::Add => " new task ".to_string(),
        EditMode::Edit { id } => format!(" edit {} ", crate::model::fmt_id(id)),
    };

    let outer = Block::default()
        .borders(Borders::ALL)
        .title(title_bar)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let sessions_row_h: u16 = if show_sessions_row { 3 } else { 0 };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(sessions_row_h),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    render_input_row(
        frame,
        rows[0],
        "Title",
        "task title",
        &state.title,
        state.field == EditField::Title,
    );
    render_input_row(
        frame,
        rows[1],
        "Priority",
        "0..4 (default 2)",
        &state.priority,
        state.field == EditField::Priority,
    );
    render_kind_row(
        frame,
        rows[2],
        &state.kind,
        state.field == EditField::Kind,
    );
    render_input_row(
        frame,
        rows[3],
        "Group",
        "optional, e.g. v0.1",
        &state.group,
        state.field == EditField::Group,
    );
    render_input_row(
        frame,
        rows[4],
        "Deps",
        "T-3, T-5 (optional)",
        &state.deps,
        state.field == EditField::Deps,
    );
    if show_sessions_row {
        render_input_row(
            frame,
            rows[5],
            "Sessions",
            "session-id-1, session-id-2",
            &state.sessions,
            state.field == EditField::Sessions,
        );
    }
    render_textarea_row(
        frame,
        rows[6],
        "Body",
        &state.body,
        state.field == EditField::Body,
    );
    render_textarea_row(
        frame,
        rows[7],
        "Acceptance",
        &state.acceptance,
        state.field == EditField::Acceptance,
    );

    let hint = Paragraph::new(Line::from(vec![
        Span::styled(
            "Tab",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" next  ·  "),
        Span::styled(
            "Shift+Tab",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" prev  ·  "),
        Span::styled(
            "Ctrl+S",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" save  ·  "),
        Span::styled(
            "Esc",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" cancel"),
    ]))
    .alignment(Alignment::Left);
    frame.render_widget(hint, rows[8]);

    let display_err = state.error.clone().or_else(|| state.current_field_error());
    if let Some(err) = display_err {
        let p = Paragraph::new(Span::styled(
            err,
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
        frame.render_widget(p, rows[9]);
    }
}

fn render_kind_row(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    kind: &str,
    focused: bool,
) {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Paragraph};

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(12), Constraint::Min(10)])
        .split(area);

    let lbl_style = if focused {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    frame.render_widget(Paragraph::new("Kind".to_string()).style(lbl_style), cols[0]);

    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);

    // Render as "  <  feature  >       (←/→ to change)" when focused;
    // just the value otherwise. The arrows are the affordance.
    let value_style = if focused {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let line = if focused {
        Line::from(vec![
            Span::styled("◀ ", Style::default().fg(Color::Cyan)),
            Span::styled(kind.to_string(), value_style),
            Span::styled(" ▶", Style::default().fg(Color::Cyan)),
            Span::styled(
                "   (←/→ to change)",
                Style::default().fg(Color::DarkGray),
            ),
        ])
    } else {
        Line::from(Span::styled(kind.to_string(), value_style))
    };
    frame.render_widget(Paragraph::new(line).block(block), cols[1]);
}

fn render_input_row(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    label: &str,
    placeholder: &str,
    input: &Input,
    focused: bool,
) {
    use ratatui::layout::{Constraint, Direction, Layout, Position};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::Span;
    use ratatui::widgets::{Block, Borders, Paragraph};

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(12), Constraint::Min(10)])
        .split(area);

    let lbl_style = if focused {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    frame.render_widget(Paragraph::new(label.to_string()).style(lbl_style), cols[0]);

    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner_width = cols[1].width.saturating_sub(2) as usize;
    let scroll = input.visual_scroll(inner_width);

    let p = if input.value().is_empty() && !placeholder.is_empty() {
        Paragraph::new(Span::styled(
            placeholder.to_string(),
            Style::default().fg(Color::DarkGray),
        ))
        .block(block)
    } else {
        Paragraph::new(input.value())
            .scroll((0, scroll as u16))
            .block(block)
    };
    frame.render_widget(p, cols[1]);

    if focused {
        // `tui_input` doesn't draw its own caret — surface the terminal cursor
        // inside the bordered cell so single-line fields blink the same way
        // TextArea does for Body/Acceptance.
        let cursor_col = input.visual_cursor().saturating_sub(scroll) as u16;
        let x = cols[1].x + 1 + cursor_col.min(inner_width.saturating_sub(1) as u16);
        let y = cols[1].y + 1;
        frame.set_cursor_position(Position::new(x, y));
    }
}

fn render_textarea_row(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    label: &str,
    textarea: &TextArea<'static>,
    focused: bool,
) {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::widgets::{Block, Borders, Paragraph};

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(12), Constraint::Min(10)])
        .split(area);

    let lbl_style = if focused {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    frame.render_widget(Paragraph::new(label.to_string()).style(lbl_style), cols[0]);

    let mut ta = textarea.clone();
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    ta.set_block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style),
    );
    frame.render_widget(&ta, cols[1]);
}

fn centered_rect(
    percent_x: u16,
    height: u16,
    r: ratatui::layout::Rect,
) -> ratatui::layout::Rect {
    use ratatui::layout::{Constraint, Direction, Layout};
    let v_pad = r.height.saturating_sub(height) / 2;
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(v_pad),
            Constraint::Length(height),
            Constraint::Length(v_pad),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_add_starts_blank_on_title() {
        let s = EditState::for_add();
        assert_eq!(s.mode, EditMode::Add);
        assert_eq!(s.field, EditField::Title);
        assert_eq!(s.title.value(), "");
        assert!(!s.is_dirty());
    }

    #[test]
    fn for_edit_populates_all_fields_from_task() {
        let task = Task {
            id: 7,
            title: "hello".into(),
            body: Some("line1\nline2".into()),
            acceptance: Some("- check x".into()),
            deps: vec![3, 5],
            status: "open".into(),
            priority: 1,
            group: Some("v0.1".into()),
            kind: "feature".into(),
            created_at: "t".into(),
            updated_at: "t".into(),
            agent_sessions: vec!["sess-a".into(), "sess-b".into()],
        };
        let s = EditState::for_edit(&task);
        assert_eq!(s.mode, EditMode::Edit { id: 7 });
        assert_eq!(s.title.value(), "hello");
        assert_eq!(s.priority.value(), "1");
        assert_eq!(s.group.value(), "v0.1");
        assert_eq!(s.deps.value(), "T-3, T-5");
        assert_eq!(s.sessions.value(), "sess-a, sess-b");
        assert_eq!(s.body.lines().join("\n"), "line1\nline2");
        assert_eq!(s.acceptance.lines().join("\n"), "- check x");
        assert!(!s.is_dirty());
    }

    #[test]
    fn sessions_field_is_navigable_only_in_edit_mode() {
        // Add mode: Deps -> Body (skips Sessions).
        let mut s = EditState::for_add();
        s.field = EditField::Deps;
        s.next_field();
        assert_eq!(s.field, EditField::Body);

        // Edit mode: Deps -> Sessions -> Body.
        let task = Task {
            id: 7,
            title: "hello".into(),
            body: None,
            acceptance: None,
            deps: vec![],
            status: "open".into(),
            priority: 2,
            group: None,
            kind: "feature".into(),
            created_at: "t".into(),
            updated_at: "t".into(),
            agent_sessions: vec![],
        };
        let mut s = EditState::for_edit(&task);
        s.field = EditField::Deps;
        s.next_field();
        assert_eq!(s.field, EditField::Sessions);
        s.next_field();
        assert_eq!(s.field, EditField::Body);
    }

    #[test]
    fn sessions_list_parses_dedupes_and_trims() {
        let mut s = EditState::for_add();
        s.sessions = Input::default().with_value("a, b ,a, , c".into());
        assert_eq!(s.sessions_list(), vec!["a", "b", "c"]);
    }

    #[test]
    fn dirty_after_changing_sessions() {
        let task = Task {
            id: 1,
            title: "t".into(),
            body: None,
            acceptance: None,
            deps: vec![],
            status: "open".into(),
            priority: 2,
            group: None,
            kind: "feature".into(),
            created_at: "t".into(),
            updated_at: "t".into(),
            agent_sessions: vec!["a".into()],
        };
        let mut s = EditState::for_edit(&task);
        assert!(!s.is_dirty());
        s.sessions = Input::default().with_value("a, b".into());
        assert!(s.is_dirty());
    }

    #[test]
    fn next_field_wraps() {
        let mut s = EditState::for_add();
        s.field = EditField::Acceptance;
        s.next_field();
        assert_eq!(s.field, EditField::Title);
    }

    #[test]
    fn prev_field_wraps() {
        let mut s = EditState::for_add();
        s.field = EditField::Title;
        s.prev_field();
        assert_eq!(s.field, EditField::Acceptance);
    }

    #[test]
    fn dirty_after_changing_title() {
        let mut s = EditState::for_add();
        s.title = Input::default().with_value("changed".into());
        assert!(s.is_dirty());
    }

    #[test]
    fn validate_rejects_empty_title() {
        let s = EditState::for_add();
        let err = s.validate().unwrap_err();
        assert!(err.contains("title"));
    }

    #[test]
    fn validate_defaults_priority_to_two_when_blank() {
        let mut s = EditState::for_add();
        s.title = Input::default().with_value("ok".into());
        assert_eq!(s.validate().unwrap().priority, 2);
    }

    #[test]
    fn validate_accepts_priority_zero_through_four() {
        for p in 0..=4 {
            let mut s = EditState::for_add();
            s.title = Input::default().with_value("ok".into());
            s.priority = Input::default().with_value(p.to_string());
            assert_eq!(s.validate().unwrap().priority, p);
        }
    }

    #[test]
    fn validate_rejects_priority_out_of_range() {
        let mut s = EditState::for_add();
        s.title = Input::default().with_value("ok".into());
        s.priority = Input::default().with_value("5".into());
        assert!(s.validate().unwrap_err().contains("0 and 4"));
    }

    #[test]
    fn validate_rejects_non_numeric_priority() {
        let mut s = EditState::for_add();
        s.title = Input::default().with_value("ok".into());
        s.priority = Input::default().with_value("foo".into());
        assert!(s.validate().unwrap_err().contains("integer"));
    }

    #[test]
    fn validate_empty_group_yields_none() {
        let mut s = EditState::for_add();
        s.title = Input::default().with_value("ok".into());
        assert!(s.validate().unwrap().group.is_none());
    }

    #[test]
    fn validate_non_empty_group_yields_some() {
        let mut s = EditState::for_add();
        s.title = Input::default().with_value("ok".into());
        s.group = Input::default().with_value("v0.1".into());
        assert_eq!(s.validate().unwrap().group.as_deref(), Some("v0.1"));
    }

    #[test]
    fn validate_parses_deps_into_json() {
        let mut s = EditState::for_add();
        s.title = Input::default().with_value("ok".into());
        s.deps = Input::default().with_value("T-3, 7".into());
        assert_eq!(s.validate().unwrap().deps_json.as_deref(), Some("[3,7]"));
    }

    #[test]
    fn validate_rejects_invalid_deps() {
        let mut s = EditState::for_add();
        s.title = Input::default().with_value("ok".into());
        s.deps = Input::default().with_value("not-a-task".into());
        assert!(s.validate().unwrap_err().contains("deps"));
    }

    #[test]
    fn validate_empty_body_yields_none() {
        let mut s = EditState::for_add();
        s.title = Input::default().with_value("ok".into());
        assert!(s.validate().unwrap().body.is_none());
    }

    #[test]
    fn current_field_error_blank_title() {
        let s = EditState::for_add();
        assert_eq!(
            s.current_field_error().as_deref(),
            Some("title is required")
        );
    }

    #[test]
    fn current_field_error_priority_out_of_range() {
        let mut s = EditState::for_add();
        s.field = EditField::Priority;
        s.priority = Input::default().with_value("9".into());
        assert!(s
            .current_field_error()
            .unwrap()
            .contains("0..=4"));
    }

    #[test]
    fn current_field_error_priority_non_numeric() {
        let mut s = EditState::for_add();
        s.field = EditField::Priority;
        s.priority = Input::default().with_value("foo".into());
        assert!(s
            .current_field_error()
            .unwrap()
            .contains("integer"));
    }

    #[test]
    fn current_field_error_priority_empty_is_ok() {
        let mut s = EditState::for_add();
        s.field = EditField::Priority;
        assert!(s.current_field_error().is_none());
    }

    #[test]
    fn current_field_error_deps_invalid() {
        let mut s = EditState::for_add();
        s.field = EditField::Deps;
        s.deps = Input::default().with_value("garbage".into());
        assert!(s.current_field_error().unwrap().contains("deps"));
    }

    #[test]
    fn current_field_error_group_is_always_ok() {
        let mut s = EditState::for_add();
        s.field = EditField::Group;
        s.group = Input::default().with_value("anything".into());
        assert!(s.current_field_error().is_none());
    }

    #[test]
    fn current_field_error_body_is_always_ok() {
        let mut s = EditState::for_add();
        s.field = EditField::Body;
        assert!(s.current_field_error().is_none());
    }
}
