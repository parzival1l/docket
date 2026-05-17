pub mod confirm;
pub mod edit;
pub mod filter_prompt;
pub mod help;
pub mod main;
pub mod session_picker;

use edit::EditState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterKind {
    Status,
    Group,
    Priority,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingAction {
    DeleteTask { id: i64, title: String },
    DiscardEdits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPickerState {
    pub task_id: i64,
    pub task_title: String,
    pub sessions: Vec<crate::agent_session_info::SessionInfo>,
    pub cursor: usize,
}

#[derive(Debug)]
pub enum Screen {
    Main,
    Help,
    FilterPrompt { kind: FilterKind, input: String },
    Confirm(PendingAction),
    Edit(Box<EditState>),
    SessionPicker(SessionPickerState),
}

impl Clone for Screen {
    fn clone(&self) -> Self {
        match self {
            Screen::Main => Screen::Main,
            Screen::Help => Screen::Help,
            Screen::FilterPrompt { kind, input } => Screen::FilterPrompt {
                kind: kind.clone(),
                input: input.clone(),
            },
            Screen::Confirm(a) => Screen::Confirm(a.clone()),
            Screen::Edit(_) => panic!("Screen::Edit cannot be cloned"),
            Screen::SessionPicker(s) => Screen::SessionPicker(s.clone()),
        }
    }
}

impl PartialEq for Screen {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Screen::Main, Screen::Main) | (Screen::Help, Screen::Help) => true,
            (
                Screen::FilterPrompt { kind: ak, input: ai },
                Screen::FilterPrompt { kind: bk, input: bi },
            ) => ak == bk && ai == bi,
            (Screen::Confirm(a), Screen::Confirm(b)) => a == b,
            (Screen::Edit(a), Screen::Edit(b)) => a.mode == b.mode,
            (Screen::SessionPicker(a), Screen::SessionPicker(b)) => a == b,
            _ => false,
        }
    }
}

impl Default for Screen {
    fn default() -> Self {
        Screen::Main
    }
}
