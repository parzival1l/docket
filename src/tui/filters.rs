use std::collections::HashMap;

use crate::model::Task;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    #[default]
    Active,
    Done,
    Backlog,
}

impl ViewMode {
    pub fn next(self) -> Self {
        match self {
            ViewMode::Active => ViewMode::Done,
            ViewMode::Done => ViewMode::Backlog,
            ViewMode::Backlog => ViewMode::Active,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ViewMode::Active => "active",
            ViewMode::Done => "done",
            ViewMode::Backlog => "backlog",
        }
    }

    fn allows_status(self, status: &str) -> bool {
        match self {
            ViewMode::Active => status == "open" || status == "in_progress",
            ViewMode::Done => status == "done",
            ViewMode::Backlog => status == "backlog",
        }
    }
}

#[derive(Default, Clone)]
pub struct Filters {
    pub view: ViewMode,
    pub status: Option<String>,
    pub group: Option<String>,
    pub priority_cap: Option<i32>,
    pub ready_only: bool,
    pub blocked_only: bool,
    pub text: Option<String>,
}

impl Filters {
    #[allow(dead_code)] // used in tests and future top-bar logic
    pub fn is_default(&self) -> bool {
        self.view == ViewMode::Active
            && self.status.is_none()
            && self.group.is_none()
            && self.priority_cap.is_none()
            && !self.ready_only
            && !self.blocked_only
            && self.text.is_none()
    }

    #[allow(dead_code)] // exercised in tests; remains for future callers
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Returns indices into `tasks` (preserving order) for tasks matching every
/// active filter. `ready` and `blocked` are mutually exclusive in practice —
/// if both are set, no task can satisfy both, so the result is empty.
pub fn filtered_indices(tasks: &[Task], f: &Filters) -> Vec<usize> {
    let by_id: HashMap<i64, &Task> = tasks.iter().map(|t| (t.id, t)).collect();
    let text_needle = f.text.as_deref().map(str::to_lowercase);

    tasks
        .iter()
        .enumerate()
        .filter(|(_, t)| f.view.allows_status(&t.status))
        .filter(|(_, t)| f.status.as_deref().is_none_or(|s| t.status == s))
        .filter(|(_, t)| {
            f.group
                .as_deref()
                .is_none_or(|g| t.group.as_deref() == Some(g))
        })
        .filter(|(_, t)| f.priority_cap.is_none_or(|cap| t.priority <= cap))
        .filter(|(_, t)| {
            if !f.ready_only {
                return true;
            }
            t.status == "open"
                && t.deps
                    .iter()
                    .all(|d| by_id.get(d).is_none_or(|x| x.status == "done"))
        })
        .filter(|(_, t)| {
            if !f.blocked_only {
                return true;
            }
            t.status == "open"
                && t.deps
                    .iter()
                    .any(|d| by_id.get(d).is_some_and(|x| x.status != "done"))
        })
        .filter(|(_, t)| {
            let Some(ref needle) = text_needle else {
                return true;
            };
            let title_lc = t.title.to_lowercase();
            let body_lc = t.body.as_deref().unwrap_or("").to_lowercase();
            title_lc.contains(needle) || body_lc.contains(needle)
        })
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: i64, title: &str, status: &str, priority: i32, deps: Vec<i64>) -> Task {
        Task {
            id,
            title: title.into(),
            body: None,
            acceptance: None,
            deps,
            status: status.into(),
            priority,
            group: None,
            kind: "feature".into(),
            created_at: "t".into(),
            updated_at: "t".into(),
            agent_sessions: vec![],
        }
    }

    #[test]
    fn default_excludes_done_and_backlog() {
        let tasks = vec![
            task(1, "a", "open", 2, vec![]),
            task(2, "b", "done", 0, vec![]),
            task(3, "c", "backlog", 0, vec![]),
            task(4, "d", "in_progress", 0, vec![]),
        ];
        let got = filtered_indices(&tasks, &Filters::default());
        assert_eq!(got, vec![0, 3]);
    }

    #[test]
    fn done_view_shows_only_done() {
        let tasks = vec![
            task(1, "a", "open", 2, vec![]),
            task(2, "b", "done", 0, vec![]),
            task(3, "c", "backlog", 0, vec![]),
        ];
        let f = Filters {
            view: ViewMode::Done,
            ..Default::default()
        };
        assert_eq!(filtered_indices(&tasks, &f), vec![1]);
    }

    #[test]
    fn backlog_view_shows_only_backlog() {
        let tasks = vec![
            task(1, "a", "open", 2, vec![]),
            task(2, "b", "done", 0, vec![]),
            task(3, "c", "backlog", 0, vec![]),
        ];
        let f = Filters {
            view: ViewMode::Backlog,
            ..Default::default()
        };
        assert_eq!(filtered_indices(&tasks, &f), vec![2]);
    }

    #[test]
    fn view_mode_next_cycles_three_states() {
        assert_eq!(ViewMode::Active.next(), ViewMode::Done);
        assert_eq!(ViewMode::Done.next(), ViewMode::Backlog);
        assert_eq!(ViewMode::Backlog.next(), ViewMode::Active);
    }

    #[test]
    fn status_filter_matches_exact() {
        let tasks = vec![
            task(1, "a", "open", 2, vec![]),
            task(2, "b", "in_progress", 2, vec![]),
            task(3, "c", "done", 2, vec![]),
        ];
        let f = Filters {
            status: Some("open".into()),
            ..Default::default()
        };
        // Active view includes open + in_progress; status filter narrows to open.
        assert_eq!(filtered_indices(&tasks, &f), vec![0]);
    }

    #[test]
    fn status_filter_done_requires_done_view() {
        // status="done" alone returns nothing because the default Active view
        // already excludes done; user must Tab to the done view first.
        let tasks = vec![
            task(1, "a", "open", 2, vec![]),
            task(2, "b", "done", 2, vec![]),
        ];
        let f = Filters {
            status: Some("done".into()),
            ..Default::default()
        };
        assert_eq!(filtered_indices(&tasks, &f), Vec::<usize>::new());
        let f = Filters {
            view: ViewMode::Done,
            status: Some("done".into()),
            ..Default::default()
        };
        assert_eq!(filtered_indices(&tasks, &f), vec![1]);
    }

    #[test]
    fn priority_cap_keeps_le() {
        let tasks = vec![
            task(1, "a", "open", 0, vec![]),
            task(2, "b", "open", 2, vec![]),
            task(3, "c", "open", 4, vec![]),
        ];
        let f = Filters {
            priority_cap: Some(2),
            ..Default::default()
        };
        assert_eq!(filtered_indices(&tasks, &f), vec![0, 1]);
    }

    #[test]
    fn ready_only_in_done_view_returns_nothing() {
        // ready_only requires status == "open"; the done view rules that out.
        let tasks = vec![task(1, "a", "done", 2, vec![])];
        let f = Filters {
            view: ViewMode::Done,
            ready_only: true,
            ..Default::default()
        };
        assert_eq!(filtered_indices(&tasks, &f), Vec::<usize>::new());
    }

    #[test]
    fn ready_only_excludes_open_with_unmet_deps() {
        // T-1 done; T-2 open w/ deps=[1] (ready); T-3 open w/ deps=[1,4] (blocked by T-4);
        // T-4 open w/ deps=[5] (blocked by T-5); T-5 open w/ no deps (ready);
        // T-6 open w/ deps=[5] (blocked by T-5).
        let tasks = vec![
            task(1, "a", "done", 2, vec![]),
            task(2, "b", "open", 2, vec![1]),
            task(3, "c", "open", 2, vec![1, 4]),
            task(4, "d", "open", 2, vec![5]),
            task(5, "e", "open", 2, vec![]),
            task(6, "f", "open", 2, vec![5]),
        ];
        let f = Filters {
            ready_only: true,
            ..Default::default()
        };
        let got = filtered_indices(&tasks, &f);
        assert_eq!(got, vec![1, 4]);
    }

    #[test]
    fn ready_only_treats_missing_dep_as_satisfied() {
        let tasks = vec![task(1, "lone", "open", 2, vec![99])];
        let f = Filters {
            ready_only: true,
            ..Default::default()
        };
        assert_eq!(filtered_indices(&tasks, &f), vec![0]);
    }

    #[test]
    fn blocked_only_keeps_open_with_unmet_deps() {
        let tasks = vec![
            task(1, "a", "open", 2, vec![]),
            task(2, "b", "open", 2, vec![1]),
            task(3, "c", "done", 2, vec![]),
        ];
        let f = Filters {
            blocked_only: true,
            ..Default::default()
        };
        assert_eq!(filtered_indices(&tasks, &f), vec![1]);
    }

    #[test]
    fn text_filter_matches_title_case_insensitive() {
        let tasks = vec![
            task(1, "Add tmux delivery", "open", 2, vec![]),
            task(2, "Schema migration", "open", 2, vec![]),
        ];
        let f = Filters {
            text: Some("TMUX".into()),
            ..Default::default()
        };
        assert_eq!(filtered_indices(&tasks, &f), vec![0]);
    }

    #[test]
    fn text_filter_matches_body() {
        let mut t = task(1, "title", "open", 2, vec![]);
        t.body = Some("Lorem ipsum dolor".into());
        let f = Filters {
            text: Some("ipsum".into()),
            ..Default::default()
        };
        assert_eq!(filtered_indices(&[t], &f), vec![0]);
    }

    #[test]
    fn is_default_initially() {
        assert!(Filters::default().is_default());
    }

    #[test]
    fn is_default_false_after_setting_status() {
        let mut f = Filters::default();
        f.status = Some("open".into());
        assert!(!f.is_default());
    }

    #[test]
    fn clear_resets_all() {
        let mut f = Filters {
            status: Some("open".into()),
            ready_only: true,
            text: Some("x".into()),
            ..Default::default()
        };
        f.clear();
        assert!(f.is_default());
    }
}
