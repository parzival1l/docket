pub mod confirm;
pub mod filter_prompt;
pub mod help;
pub mod main;

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Main,
    Help,
    FilterPrompt { kind: FilterKind, input: String },
    Confirm(PendingAction),
}

impl Default for Screen {
    fn default() -> Self {
        Screen::Main
    }
}
