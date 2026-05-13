#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    List,
    Detail,
    Help,
    FilterPrompt,
    Confirm,
    Edit,
}

#[derive(Debug, Clone, Copy)]
pub struct Binding {
    pub keys: &'static str,
    pub label: &'static str,
    pub scope: Scope,
    pub footer: bool,
}

pub const COMMAND_REGISTRY: &[Binding] = &[
    Binding { keys: "?", label: "help", scope: Scope::Global, footer: true },
    Binding { keys: "q", label: "quit", scope: Scope::Global, footer: true },
    Binding { keys: "/", label: "filter text", scope: Scope::Global, footer: true },
    Binding { keys: "R", label: "reload", scope: Scope::Global, footer: false },
    Binding { keys: "j/k", label: "nav", scope: Scope::List, footer: true },
    Binding { keys: "g/G", label: "top/bottom", scope: Scope::List, footer: false },
    Binding { keys: "l", label: "detail", scope: Scope::List, footer: true },
    Binding { keys: "n", label: "new", scope: Scope::List, footer: true },
    Binding { keys: "e", label: "edit", scope: Scope::List, footer: true },
    Binding { keys: "s", label: "cycle status", scope: Scope::List, footer: true },
    Binding { keys: "d", label: "done", scope: Scope::List, footer: true },
    Binding { keys: "x", label: "delete", scope: Scope::List, footer: true },
    Binding { keys: "f s", label: "filter status", scope: Scope::List, footer: false },
    Binding { keys: "f g", label: "filter group", scope: Scope::List, footer: false },
    Binding { keys: "f p", label: "filter priority", scope: Scope::List, footer: false },
    Binding { keys: "f r", label: "ready", scope: Scope::List, footer: true },
    Binding { keys: "f b", label: "blocked", scope: Scope::List, footer: false },
    Binding { keys: "f c", label: "clear filters", scope: Scope::List, footer: true },
    Binding { keys: "h", label: "list", scope: Scope::Detail, footer: true },
    Binding { keys: "e", label: "edit", scope: Scope::Detail, footer: true },
    Binding { keys: "PgUp/PgDn", label: "scroll", scope: Scope::Detail, footer: true },
    Binding { keys: "?/Esc", label: "close help", scope: Scope::Help, footer: true },
    Binding { keys: "Enter", label: "apply", scope: Scope::FilterPrompt, footer: true },
    Binding { keys: "Esc", label: "cancel", scope: Scope::FilterPrompt, footer: true },
    Binding { keys: "y/Enter", label: "confirm", scope: Scope::Confirm, footer: true },
    Binding { keys: "n/Esc", label: "cancel", scope: Scope::Confirm, footer: true },
    Binding { keys: "Tab/S-Tab", label: "next/prev field", scope: Scope::Edit, footer: true },
    Binding { keys: "Ctrl+S", label: "save", scope: Scope::Edit, footer: true },
    Binding { keys: "Esc", label: "cancel", scope: Scope::Edit, footer: true },
];

pub fn footer_for(scope: Scope) -> Vec<Binding> {
    COMMAND_REGISTRY
        .iter()
        .filter(|b| b.footer && (b.scope == Scope::Global || b.scope == scope))
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_quit_and_help() {
        assert!(COMMAND_REGISTRY.iter().any(|b| b.keys == "q"));
        assert!(COMMAND_REGISTRY.iter().any(|b| b.keys == "?"));
    }

    #[test]
    fn registry_contains_mutation_keys() {
        assert!(COMMAND_REGISTRY
            .iter()
            .any(|b| b.keys == "s" && b.scope == Scope::List));
        assert!(COMMAND_REGISTRY
            .iter()
            .any(|b| b.keys == "d" && b.scope == Scope::List));
        assert!(COMMAND_REGISTRY
            .iter()
            .any(|b| b.keys == "x" && b.scope == Scope::List));
    }

    #[test]
    fn footer_for_list_includes_globals_and_list_keys() {
        let fb = footer_for(Scope::List);
        let labels: Vec<&str> = fb.iter().map(|b| b.label).collect();
        assert!(labels.contains(&"quit"));
        assert!(labels.contains(&"help"));
        assert!(labels.contains(&"nav"));
        assert!(labels.contains(&"detail"));
        assert!(labels.contains(&"clear filters"));
        assert!(labels.contains(&"cycle status"));
        assert!(labels.contains(&"done"));
        assert!(labels.contains(&"delete"));
    }

    #[test]
    fn footer_for_detail_includes_globals_and_detail_keys() {
        let fb = footer_for(Scope::Detail);
        let labels: Vec<&str> = fb.iter().map(|b| b.label).collect();
        assert!(labels.contains(&"quit"));
        assert!(labels.contains(&"list"));
        assert!(labels.contains(&"scroll"));
    }

    #[test]
    fn footer_for_filter_prompt_includes_apply_and_cancel() {
        let fb = footer_for(Scope::FilterPrompt);
        let labels: Vec<&str> = fb.iter().map(|b| b.label).collect();
        assert!(labels.contains(&"apply"));
        assert!(labels.contains(&"cancel"));
    }

    #[test]
    fn footer_for_confirm_includes_confirm_and_cancel() {
        let fb = footer_for(Scope::Confirm);
        let labels: Vec<&str> = fb.iter().map(|b| b.label).collect();
        assert!(labels.contains(&"confirm"));
        assert!(labels.contains(&"cancel"));
    }

    #[test]
    fn registry_contains_edit_form_keys() {
        assert!(COMMAND_REGISTRY
            .iter()
            .any(|b| b.keys == "Ctrl+S" && b.scope == Scope::Edit));
        assert!(COMMAND_REGISTRY
            .iter()
            .any(|b| b.keys == "n" && b.scope == Scope::List));
        assert!(COMMAND_REGISTRY
            .iter()
            .any(|b| b.keys == "e" && b.scope == Scope::List));
        assert!(COMMAND_REGISTRY
            .iter()
            .any(|b| b.keys == "e" && b.scope == Scope::Detail));
    }

    #[test]
    fn footer_for_edit_includes_save_and_cancel() {
        let fb = footer_for(Scope::Edit);
        let labels: Vec<&str> = fb.iter().map(|b| b.label).collect();
        assert!(labels.contains(&"save"));
        assert!(labels.contains(&"cancel"));
        assert!(labels.contains(&"next/prev field"));
    }
}
