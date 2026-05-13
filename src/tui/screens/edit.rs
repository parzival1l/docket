#![allow(dead_code)] // consumed by App handlers/renderer in PR-5 Tasks 4-10

use tui_input::Input;
use tui_textarea::TextArea;

use crate::model::{fmt_id, Task};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMode {
    Add,
    Edit { id: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    Title,
    Priority,
    Group,
    Deps,
    Body,
    Acceptance,
}

pub const FIELD_ORDER: [EditField; 6] = [
    EditField::Title,
    EditField::Priority,
    EditField::Group,
    EditField::Deps,
    EditField::Body,
    EditField::Acceptance,
];

pub struct EditState {
    pub mode: EditMode,
    pub field: EditField,
    pub title: Input,
    pub priority: Input,
    pub group: Input,
    pub deps: Input,
    pub body: TextArea<'static>,
    pub acceptance: TextArea<'static>,
    pub error: Option<String>,
    orig_title: String,
    orig_priority: String,
    orig_group: String,
    orig_deps: String,
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
            .field("group", &self.group.value())
            .field("deps", &self.deps.value())
            .field("body", &self.body.lines().join("\n"))
            .field("acceptance", &self.acceptance.lines().join("\n"))
            .field("error", &self.error)
            .finish()
    }
}

impl EditState {
    pub fn for_add() -> Self {
        Self::build(EditMode::Add, "", "", "", "", "", "")
    }

    pub fn for_edit(task: &Task) -> Self {
        let deps_str = task
            .deps
            .iter()
            .map(|d| fmt_id(*d))
            .collect::<Vec<_>>()
            .join(", ");
        Self::build(
            EditMode::Edit { id: task.id },
            &task.title,
            &task.priority.to_string(),
            task.group.as_deref().unwrap_or(""),
            &deps_str,
            task.body.as_deref().unwrap_or(""),
            task.acceptance.as_deref().unwrap_or(""),
        )
    }

    fn build(
        mode: EditMode,
        title: &str,
        priority: &str,
        group: &str,
        deps: &str,
        body: &str,
        acceptance: &str,
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
        Self {
            mode,
            field: EditField::Title,
            title: Input::default().with_value(title.to_string()),
            priority: Input::default().with_value(priority.to_string()),
            group: Input::default().with_value(group.to_string()),
            deps: Input::default().with_value(deps.to_string()),
            body: TextArea::new(body_lines),
            acceptance: TextArea::new(acc_lines),
            error: None,
            orig_title: title.to_string(),
            orig_priority: priority.to_string(),
            orig_group: group.to_string(),
            orig_deps: deps.to_string(),
            orig_body: body.to_string(),
            orig_acceptance: acceptance.to_string(),
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.title.value() != self.orig_title
            || self.priority.value() != self.orig_priority
            || self.group.value() != self.orig_group
            || self.deps.value() != self.orig_deps
            || self.body.lines().join("\n") != self.orig_body
            || self.acceptance.lines().join("\n") != self.orig_acceptance
    }

    pub fn next_field(&mut self) {
        let idx = FIELD_ORDER
            .iter()
            .position(|f| *f == self.field)
            .unwrap_or(0);
        self.field = FIELD_ORDER[(idx + 1) % FIELD_ORDER.len()];
    }

    pub fn prev_field(&mut self) {
        let idx = FIELD_ORDER
            .iter()
            .position(|f| *f == self.field)
            .unwrap_or(0);
        self.field = FIELD_ORDER[(idx + FIELD_ORDER.len() - 1) % FIELD_ORDER.len()];
    }
}

pub fn render(_frame: &mut ratatui::Frame, _state: &EditState) {
    // Real renderer lands in Task 10.
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
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let s = EditState::for_edit(&task);
        assert_eq!(s.mode, EditMode::Edit { id: 7 });
        assert_eq!(s.title.value(), "hello");
        assert_eq!(s.priority.value(), "1");
        assert_eq!(s.group.value(), "v0.1");
        assert_eq!(s.deps.value(), "T-3, T-5");
        assert_eq!(s.body.lines().join("\n"), "line1\nline2");
        assert_eq!(s.acceptance.lines().join("\n"), "- check x");
        assert!(!s.is_dirty());
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
}
