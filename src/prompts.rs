use crate::model::{fmt_id, Task};

pub const PROMPT_TDD: &str = include_str!("../templates/tdd-pursuit.md");
pub const PROMPT_CREATE_TASK: &str = include_str!("../templates/create-task.md");
pub const PROMPT_COMMIT: &str = include_str!("../templates/commit.md");
pub const PROMPT_PR: &str = include_str!("../templates/pr.md");

pub fn assemble_prompt(t: &Task) -> String {
    let body = t.body.as_deref().unwrap_or("(no body)");
    let acceptance = t.acceptance.as_deref().unwrap_or("(no acceptance criteria)");
    format!(
        "# Task {}: {}\n\n## Body\n{}\n\n## Acceptance\n{}\n\n{}",
        fmt_id(t.id),
        t.title,
        body,
        acceptance,
        PROMPT_TDD
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(body: Option<&str>, acceptance: Option<&str>) -> Task {
        Task {
            id: 7,
            title: "do thing".into(),
            body: body.map(String::from),
            acceptance: acceptance.map(String::from),
            deps: vec![],
            status: "open".into(),
            priority: 2,
            group: None,
            created_at: "t".into(),
            updated_at: "t".into(),
        }
    }

    #[test]
    fn assemble_prompt_includes_title_and_id() {
        let s = assemble_prompt(&make_task(Some("b"), Some("a")));
        assert!(s.starts_with("# Task T-7: do thing"));
    }

    #[test]
    fn assemble_prompt_substitutes_missing_body() {
        let s = assemble_prompt(&make_task(None, Some("a")));
        assert!(s.contains("## Body\n(no body)"));
    }

    #[test]
    fn assemble_prompt_substitutes_missing_acceptance() {
        let s = assemble_prompt(&make_task(Some("b"), None));
        assert!(s.contains("## Acceptance\n(no acceptance criteria)"));
    }

    #[test]
    fn assemble_prompt_appends_tdd_template() {
        let s = assemble_prompt(&make_task(Some("b"), Some("a")));
        assert!(s.contains(PROMPT_TDD.trim_start()));
    }
}
