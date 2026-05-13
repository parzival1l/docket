pub mod confirm;
pub mod edit;
pub mod filter_prompt;
pub mod help;
pub mod main;

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
    #[allow(dead_code)] // constructed in PR-5 Task 9
    DiscardEdits,
}

#[derive(Debug)]
pub enum Screen {
    Main,
    Help,
    FilterPrompt { kind: FilterKind, input: String },
    Confirm(PendingAction),
    Edit(Box<EditState>),
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
            _ => false,
        }
    }
}

impl Default for Screen {
    fn default() -> Self {
        Screen::Main
    }
}
