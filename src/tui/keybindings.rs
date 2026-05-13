#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    List,
    Detail,
    Help,
    FilterPrompt,
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
    Binding { keys: "f s", label: "filter status", scope: Scope::List, footer: false },
    Binding { keys: "f g", label: "filter group", scope: Scope::List, footer: false },
    Binding { keys: "f p", label: "filter priority", scope: Scope::List, footer: false },
    Binding { keys: "f r", label: "ready", scope: Scope::List, footer: true },
    Binding { keys: "f b", label: "blocked", scope: Scope::List, footer: false },
    Binding { keys: "f c", label: "clear filters", scope: Scope::List, footer: true },
    Binding { keys: "h", label: "list", scope: Scope::Detail, footer: true },
    Binding { keys: "PgUp/PgDn", label: "scroll", scope: Scope::Detail, footer: true },
    Binding { keys: "?/Esc", label: "close help", scope: Scope::Help, footer: true },
    Binding { keys: "Enter", label: "apply", scope: Scope::FilterPrompt, footer: true },
    Binding { keys: "Esc", label: "cancel", scope: Scope::FilterPrompt, footer: true },
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
    fn footer_for_list_includes_globals_and_list_keys() {
        let fb = footer_for(Scope::List);
        let labels: Vec<&str> = fb.iter().map(|b| b.label).collect();
        assert!(labels.contains(&"quit"));
        assert!(labels.contains(&"help"));
        assert!(labels.contains(&"nav"));
        assert!(labels.contains(&"detail"));
        assert!(labels.contains(&"clear filters"));
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
}
