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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedForm {
    pub title: String,
    pub body: Option<String>,
    pub acceptance: Option<String>,
    pub deps_json: Option<String>,
    pub priority: i32,
    pub group: Option<String>,
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
        })
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
}
