use crate::db::{load_all_tasks, open_db};
use crate::model::Task;
use crate::tui::filters::{filtered_indices, Filters};
use crate::tui::screens::edit::EditState;
use crate::tui::screens::{FilterKind, PendingAction, Screen};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use rusqlite::Connection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    List,
    Detail,
}

fn next_status(current: &str) -> &'static str {
    match current {
        "open" => "in_progress",
        "in_progress" => "done",
        "done" => "open",
        _ => "open",
    }
}

pub struct App {
    pub conn: Connection,
    pub tasks: Vec<Task>,
    pub filters: Filters,
    pub cursor: usize,
    pub focus: Pane,
    pub screen: Screen,
    pub pending_chord: Option<char>,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Result<Self> {
        let conn = open_db()?;
        let tasks = load_all_tasks(&conn)?;
        Ok(Self {
            conn,
            tasks,
            filters: Filters::default(),
            cursor: 0,
            focus: Pane::List,
            screen: Screen::Main,
            pending_chord: None,
            should_quit: false,
        })
    }

    pub fn reload(&mut self) -> Result<()> {
        self.tasks = load_all_tasks(&self.conn)?;
        self.clamp_cursor();
        Ok(())
    }

    pub fn visible_indices(&self) -> Vec<usize> {
        filtered_indices(&self.tasks, &self.filters)
    }

    pub fn selected_task(&self) -> Option<&Task> {
        let visible = self.visible_indices();
        visible.get(self.cursor).and_then(|i| self.tasks.get(*i))
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match &self.screen {
            Screen::Main => self.handle_main(key),
            Screen::Help => self.handle_help(key),
            Screen::FilterPrompt { .. } => self.handle_filter_prompt(key),
            Screen::Confirm(_) => self.handle_confirm(key),
            Screen::Edit(_) => self.handle_edit(key),
        }
    }

    fn handle_main(&mut self, key: KeyEvent) {
        if let Some('f') = self.pending_chord {
            self.pending_chord = None;
            match key.code {
                KeyCode::Char('s') => {
                    self.screen = Screen::FilterPrompt {
                        kind: FilterKind::Status,
                        input: self.filters.status.clone().unwrap_or_default(),
                    };
                }
                KeyCode::Char('g') => {
                    self.screen = Screen::FilterPrompt {
                        kind: FilterKind::Group,
                        input: self.filters.group.clone().unwrap_or_default(),
                    };
                }
                KeyCode::Char('p') => {
                    self.screen = Screen::FilterPrompt {
                        kind: FilterKind::Priority,
                        input: self
                            .filters
                            .priority_cap
                            .map(|p| p.to_string())
                            .unwrap_or_default(),
                    };
                }
                KeyCode::Char('r') => {
                    self.filters.ready_only = !self.filters.ready_only;
                    if self.filters.ready_only {
                        self.filters.blocked_only = false;
                    }
                    self.clamp_cursor();
                }
                KeyCode::Char('b') => {
                    self.filters.blocked_only = !self.filters.blocked_only;
                    if self.filters.blocked_only {
                        self.filters.ready_only = false;
                    }
                    self.clamp_cursor();
                }
                KeyCode::Char('c') => {
                    self.filters.clear();
                    self.clamp_cursor();
                }
                _ => {}
            }
            return;
        }

        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) => {
                self.should_quit = true;
                return;
            }
            (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
                return;
            }
            (KeyCode::Char('?'), _) => {
                self.screen = Screen::Help;
                return;
            }
            (KeyCode::Char('/'), _) => {
                self.screen = Screen::FilterPrompt {
                    kind: FilterKind::Text,
                    input: self.filters.text.clone().unwrap_or_default(),
                };
                return;
            }
            (KeyCode::Char('R'), _) => {
                let _ = self.reload();
                return;
            }
            _ => {}
        }

        match self.focus {
            Pane::List => self.handle_list(key),
            Pane::Detail => self.handle_detail(key),
        }
    }

    fn handle_list(&mut self, key: KeyEvent) {
        let visible_len = self.visible_indices().len();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if visible_len > 0 && self.cursor + 1 < visible_len {
                    self.cursor += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.cursor = self.cursor.saturating_sub(1);
            }
            KeyCode::Char('g') => {
                self.cursor = 0;
            }
            KeyCode::Char('G') => {
                self.cursor = visible_len.saturating_sub(1);
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.focus = Pane::Detail;
            }
            KeyCode::Char('f') => {
                self.pending_chord = Some('f');
            }
            KeyCode::Char('s') => {
                self.cycle_status();
            }
            KeyCode::Char('d') => {
                self.mark_done();
            }
            KeyCode::Char('x') => {
                self.open_delete_confirm();
            }
            KeyCode::Char('n') => {
                self.open_add_form();
            }
            KeyCode::Char('e') => {
                self.open_edit_form();
            }
            _ => {}
        }
    }

    fn cycle_status(&mut self) {
        let Some(task) = self.selected_task() else {
            return;
        };
        let id = task.id;
        let next = next_status(&task.status);
        if crate::db::set_status(&self.conn, id, next).is_ok() {
            let _ = self.reload();
            self.preserve_cursor_on(id);
        }
    }

    fn mark_done(&mut self) {
        let Some(task) = self.selected_task() else {
            return;
        };
        let id = task.id;
        if crate::db::set_status(&self.conn, id, "done").is_ok() {
            let _ = self.reload();
            self.preserve_cursor_on(id);
        }
    }

    fn open_delete_confirm(&mut self) {
        let Some(task) = self.selected_task() else {
            return;
        };
        self.screen = Screen::Confirm(PendingAction::DeleteTask {
            id: task.id,
            title: task.title.clone(),
        });
    }

    fn handle_detail(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('h') | KeyCode::Left => {
                self.focus = Pane::List;
            }
            KeyCode::Char('e') => {
                self.open_edit_form();
            }
            _ => {}
        }
    }

    fn open_add_form(&mut self) {
        self.screen = Screen::Edit(Box::new(EditState::for_add()));
    }

    fn open_edit_form(&mut self) {
        if let Some(task) = self.selected_task() {
            self.screen = Screen::Edit(Box::new(EditState::for_edit(task)));
        }
    }

    fn handle_help(&mut self, key: KeyEvent) {
        if matches!(
            key.code,
            KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q')
        ) {
            self.screen = Screen::Main;
        }
    }

    fn handle_confirm(&mut self, key: KeyEvent) {
        let confirmed = matches!(key.code, KeyCode::Char('y') | KeyCode::Enter);
        let cancelled = matches!(
            key.code,
            KeyCode::Char('n') | KeyCode::Char('q') | KeyCode::Esc
        );

        if !confirmed && !cancelled {
            return;
        }

        if confirmed {
            let action = match &self.screen {
                Screen::Confirm(a) => a.clone(),
                _ => return,
            };
            match action {
                PendingAction::DeleteTask { id, .. } => {
                    if crate::db::delete_task(&self.conn, id).is_ok() {
                        let _ = self.reload();
                    }
                }
                PendingAction::DiscardEdits => {
                    // The Edit form already transitioned out of Screen::Edit
                    // when we opened the discard confirm; there's nothing to
                    // persist. Fall through to set Screen::Main.
                }
            }
        }

        self.screen = Screen::Main;
    }

    fn handle_edit(&mut self, key: KeyEvent) {
        match (key.code, key.modifiers) {
            (KeyCode::Tab, _) => {
                if let Screen::Edit(state) = &mut self.screen {
                    state.next_field();
                }
                return;
            }
            (KeyCode::BackTab, _) => {
                if let Screen::Edit(state) = &mut self.screen {
                    state.prev_field();
                }
                return;
            }
            (KeyCode::Esc, _) => {
                // Dirty path lands in Task 9; today Esc always closes.
                self.screen = Screen::Main;
                return;
            }
            _ => {}
        }

        let Screen::Edit(state) = &mut self.screen else {
            return;
        };
        let event = crossterm::event::Event::Key(key);
        use crate::tui::screens::edit::EditField;
        use tui_input::backend::crossterm::EventHandler;
        match state.field {
            EditField::Title => {
                let _ = state.title.handle_event(&event);
            }
            EditField::Priority => {
                let _ = state.priority.handle_event(&event);
            }
            EditField::Group => {
                let _ = state.group.handle_event(&event);
            }
            EditField::Deps => {
                let _ = state.deps.handle_event(&event);
            }
            EditField::Body => {
                state.body.input(event);
            }
            EditField::Acceptance => {
                state.acceptance.input(event);
            }
        }
    }

    fn handle_filter_prompt(&mut self, key: KeyEvent) {
        let Screen::FilterPrompt { kind, input } = &mut self.screen else {
            return;
        };
        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::Main;
                self.pending_chord = None;
            }
            KeyCode::Enter => {
                let value = input.trim().to_string();
                let empty = value.is_empty();
                match kind {
                    FilterKind::Status => {
                        self.filters.status = if empty { None } else { Some(value) };
                    }
                    FilterKind::Group => {
                        self.filters.group = if empty { None } else { Some(value) };
                    }
                    FilterKind::Priority => {
                        self.filters.priority_cap = if empty {
                            None
                        } else {
                            value.parse::<i32>().ok()
                        };
                    }
                    FilterKind::Text => {
                        self.filters.text = if empty { None } else { Some(value) };
                    }
                }
                self.screen = Screen::Main;
                self.clamp_cursor();
            }
            KeyCode::Backspace => {
                input.pop();
            }
            KeyCode::Char(c) => {
                input.push(c);
            }
            _ => {}
        }
    }

    fn clamp_cursor(&mut self) {
        let visible_len = self.visible_indices().len();
        if visible_len == 0 {
            self.cursor = 0;
        } else if self.cursor >= visible_len {
            self.cursor = visible_len - 1;
        }
    }

    /// Set cursor to the visible position of `id` (if still visible),
    /// else clamp to the new visible length.
    fn preserve_cursor_on(&mut self, id: i64) {
        let visible = self.visible_indices();
        if let Some(pos) = visible
            .iter()
            .position(|i| self.tasks.get(*i).map(|t| t.id) == Some(id))
        {
            self.cursor = pos;
        } else {
            self.clamp_cursor();
        }
    }

    pub fn render(&self, frame: &mut Frame) {
        match &self.screen {
            Screen::Edit(state) => crate::tui::screens::edit::render(frame, state),
            _ => {
                crate::tui::screens::main::render(self, frame);
                match &self.screen {
                    Screen::Help => crate::tui::screens::help::render(frame),
                    Screen::FilterPrompt { kind, input } => {
                        crate::tui::screens::filter_prompt::render(frame, kind, input)
                    }
                    Screen::Confirm(action) => {
                        crate::tui::screens::confirm::render(frame, action)
                    }
                    Screen::Main | Screen::Edit(_) => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_schema, insert_task, set_status, NewTask};
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn mem_app() -> App {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        let tasks = load_all_tasks(&conn).unwrap();
        App {
            conn,
            tasks,
            filters: Filters::default(),
            cursor: 0,
            focus: Pane::List,
            screen: Screen::Main,
            pending_chord: None,
            should_quit: false,
        }
    }

    fn mem_app_with(seed: &[(&str, &str, i32)]) -> App {
        let mut app = mem_app();
        for (title, status, priority) in seed {
            let id = insert_task(
                &app.conn,
                NewTask {
                    title,
                    body: None,
                    acceptance: None,
                    deps_json: None,
                    priority: *priority,
                    group_id: None,
                },
            )
            .unwrap();
            if *status != "open" {
                set_status(&app.conn, id, status).unwrap();
            }
        }
        app.reload().unwrap();
        app
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn key_with(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn q_sets_should_quit() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn ctrl_c_sets_should_quit() {
        let mut app = mem_app();
        app.handle_key(key_with(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
    }

    #[test]
    fn question_mark_opens_help() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('?')));
        assert_eq!(app.screen, Screen::Help);
    }

    #[test]
    fn esc_closes_help() {
        let mut app = mem_app();
        app.screen = Screen::Help;
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn j_moves_cursor_down() {
        let mut app = mem_app_with(&[("a", "open", 2), ("b", "open", 2), ("c", "open", 2)]);
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.cursor, 1);
    }

    #[test]
    fn j_clamps_at_end() {
        let mut app = mem_app_with(&[("a", "open", 2), ("b", "open", 2)]);
        app.handle_key(key(KeyCode::Char('j')));
        app.handle_key(key(KeyCode::Char('j')));
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.cursor, 1);
    }

    #[test]
    fn k_moves_up_and_saturates_at_zero() {
        let mut app = mem_app_with(&[("a", "open", 2), ("b", "open", 2)]);
        app.handle_key(key(KeyCode::Char('j')));
        app.handle_key(key(KeyCode::Char('k')));
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn capital_g_jumps_to_last() {
        let mut app = mem_app_with(&[("a", "open", 2), ("b", "open", 2), ("c", "open", 2)]);
        app.handle_key(key(KeyCode::Char('G')));
        assert_eq!(app.cursor, 2);
    }

    #[test]
    fn l_focuses_detail_and_h_focuses_list() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('l')));
        assert_eq!(app.focus, Pane::Detail);
        app.handle_key(key(KeyCode::Char('h')));
        assert_eq!(app.focus, Pane::List);
    }

    #[test]
    fn f_then_r_toggles_ready_filter() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('f')));
        assert_eq!(app.pending_chord, Some('f'));
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.filters.ready_only);
        assert!(app.pending_chord.is_none());
    }

    #[test]
    fn f_then_b_toggles_blocked_filter_and_clears_ready() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.filters.ready_only = true;
        app.handle_key(key(KeyCode::Char('f')));
        app.handle_key(key(KeyCode::Char('b')));
        assert!(app.filters.blocked_only);
        assert!(!app.filters.ready_only);
    }

    #[test]
    fn f_then_c_clears_all_filters() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.filters.ready_only = true;
        app.filters.status = Some("open".into());
        app.handle_key(key(KeyCode::Char('f')));
        app.handle_key(key(KeyCode::Char('c')));
        assert!(app.filters.is_default());
    }

    #[test]
    fn slash_opens_text_filter_prompt() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('/')));
        match &app.screen {
            Screen::FilterPrompt { kind, input } => {
                assert_eq!(*kind, FilterKind::Text);
                assert_eq!(input, "");
            }
            _ => panic!("expected FilterPrompt"),
        }
    }

    #[test]
    fn filter_prompt_typing_accumulates_into_input() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('/')));
        for c in "tmux".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        match &app.screen {
            Screen::FilterPrompt { input, .. } => assert_eq!(input, "tmux"),
            _ => panic!(),
        }
    }

    #[test]
    fn filter_prompt_backspace_pops_last_char() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('/')));
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Char('b')));
        app.handle_key(key(KeyCode::Backspace));
        match &app.screen {
            Screen::FilterPrompt { input, .. } => assert_eq!(input, "a"),
            _ => panic!(),
        }
    }

    #[test]
    fn filter_prompt_enter_applies_text_and_returns_to_main() {
        let mut app = mem_app_with(&[("hello world", "open", 2), ("other", "open", 2)]);
        app.handle_key(key(KeyCode::Char('/')));
        for c in "hello".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Main);
        assert_eq!(app.filters.text.as_deref(), Some("hello"));
        assert_eq!(app.visible_indices().len(), 1);
    }

    #[test]
    fn filter_prompt_esc_cancels() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('/')));
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Main);
        assert!(app.filters.text.is_none());
    }

    #[test]
    fn applying_filter_clamps_out_of_range_cursor() {
        let mut app = mem_app_with(&[("a", "open", 2), ("b", "done", 2)]);
        app.cursor = 1;
        app.handle_key(key(KeyCode::Char('f')));
        app.handle_key(key(KeyCode::Char('r')));
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn selected_task_returns_cursor_task() {
        let app = mem_app_with(&[("a", "open", 2), ("b", "open", 2)]);
        assert_eq!(app.selected_task().unwrap().title, "a");
    }

    #[test]
    fn selected_task_none_when_no_tasks() {
        let app = mem_app();
        assert!(app.selected_task().is_none());
    }

    #[test]
    fn s_cycles_open_to_in_progress() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.selected_task().unwrap().status, "in_progress");
    }

    #[test]
    fn s_cycles_in_progress_to_done() {
        let mut app = mem_app_with(&[("a", "in_progress", 2)]);
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.selected_task().unwrap().status, "done");
    }

    #[test]
    fn s_cycles_done_back_to_open() {
        let mut app = mem_app_with(&[("a", "done", 2)]);
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.selected_task().unwrap().status, "open");
    }

    #[test]
    fn s_cycles_custom_status_to_open() {
        let mut app = mem_app_with(&[("a", "wontfix", 2)]);
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.selected_task().unwrap().status, "open");
    }

    #[test]
    fn s_keeps_cursor_on_same_task_id_after_status_change() {
        let mut app = mem_app_with(&[
            ("first", "open", 2),
            ("second", "open", 2),
            ("third", "open", 2),
        ]);
        app.handle_key(key(KeyCode::Char('j')));
        let id_before = app.selected_task().unwrap().id;
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.selected_task().unwrap().id, id_before);
        assert_eq!(app.selected_task().unwrap().status, "in_progress");
    }

    #[test]
    fn s_on_empty_list_is_a_noop() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('s')));
        assert!(app.selected_task().is_none());
    }

    #[test]
    fn d_marks_open_task_done() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(app.selected_task().unwrap().status, "done");
    }

    #[test]
    fn d_marks_in_progress_task_done() {
        let mut app = mem_app_with(&[("a", "in_progress", 2)]);
        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(app.selected_task().unwrap().status, "done");
    }

    #[test]
    fn d_on_already_done_task_stays_done() {
        let mut app = mem_app_with(&[("a", "done", 2)]);
        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(app.selected_task().unwrap().status, "done");
    }

    #[test]
    fn d_on_empty_list_is_a_noop() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('d')));
        assert!(app.selected_task().is_none());
    }

    #[test]
    fn x_opens_confirm_modal_for_cursor_task() {
        let mut app = mem_app_with(&[("hello", "open", 2)]);
        let id = app.selected_task().unwrap().id;
        app.handle_key(key(KeyCode::Char('x')));
        match &app.screen {
            Screen::Confirm(PendingAction::DeleteTask { id: got_id, title }) => {
                assert_eq!(*got_id, id);
                assert_eq!(title, "hello");
            }
            other => panic!("expected Confirm(DeleteTask), got {:?}", other),
        }
    }

    #[test]
    fn x_on_empty_list_is_a_noop() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('x')));
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn confirm_y_deletes_task_and_returns_to_main() {
        let mut app = mem_app_with(&[("a", "open", 2), ("b", "open", 2)]);
        let id_to_delete = app.selected_task().unwrap().id;
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Char('y')));
        assert_eq!(app.screen, Screen::Main);
        assert_eq!(app.tasks.len(), 1);
        assert!(app.tasks.iter().all(|t| t.id != id_to_delete));
    }

    #[test]
    fn confirm_enter_deletes_task_and_returns_to_main() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Main);
        assert!(app.tasks.is_empty());
    }

    #[test]
    fn confirm_n_cancels_and_keeps_task() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Char('n')));
        assert_eq!(app.screen, Screen::Main);
        assert_eq!(app.tasks.len(), 1);
    }

    #[test]
    fn confirm_esc_cancels_and_keeps_task() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Main);
        assert_eq!(app.tasks.len(), 1);
    }

    #[test]
    fn confirm_q_cancels_and_keeps_task() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Char('q')));
        assert_eq!(app.screen, Screen::Main);
        assert!(!app.should_quit);
        assert_eq!(app.tasks.len(), 1);
    }

    #[test]
    fn confirm_delete_clamps_cursor_when_last_task_removed() {
        let mut app = mem_app_with(&[("a", "open", 2), ("b", "open", 2)]);
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.cursor, 1);
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Char('y')));
        assert_eq!(app.cursor, 0);
        assert_eq!(app.tasks.len(), 1);
    }

    #[test]
    fn n_opens_add_form_with_blank_state() {
        use crate::tui::screens::edit::{EditField, EditMode};
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        match &app.screen {
            Screen::Edit(state) => {
                assert_eq!(state.mode, EditMode::Add);
                assert_eq!(state.field, EditField::Title);
                assert_eq!(state.title.value(), "");
            }
            other => panic!("expected Screen::Edit, got {:?}", other),
        }
    }

    #[test]
    fn e_from_list_opens_edit_form_for_cursor_task() {
        use crate::tui::screens::edit::EditMode;
        let mut app = mem_app_with(&[("hello", "open", 2)]);
        let id = app.selected_task().unwrap().id;
        app.handle_key(key(KeyCode::Char('e')));
        match &app.screen {
            Screen::Edit(state) => {
                assert_eq!(state.mode, EditMode::Edit { id });
                assert_eq!(state.title.value(), "hello");
            }
            other => panic!("expected Screen::Edit, got {:?}", other),
        }
    }

    #[test]
    fn e_from_detail_pane_also_opens_edit_form() {
        use crate::tui::screens::edit::EditMode;
        let mut app = mem_app_with(&[("hello", "open", 2)]);
        let id = app.selected_task().unwrap().id;
        app.handle_key(key(KeyCode::Char('l')));
        assert_eq!(app.focus, Pane::Detail);
        app.handle_key(key(KeyCode::Char('e')));
        match &app.screen {
            Screen::Edit(state) => {
                assert_eq!(state.mode, EditMode::Edit { id });
            }
            other => panic!("expected Screen::Edit, got {:?}", other),
        }
    }

    #[test]
    fn e_on_empty_list_does_not_open_form() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('e')));
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn n_works_on_empty_list() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        assert!(matches!(app.screen, Screen::Edit(_)));
    }

    #[test]
    fn tab_advances_to_next_field_in_edit() {
        use crate::tui::screens::edit::EditField;
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key(KeyCode::Tab));
        if let Screen::Edit(state) = &app.screen {
            assert_eq!(state.field, EditField::Priority);
        } else {
            panic!("expected Edit");
        }
    }

    #[test]
    fn shift_tab_goes_back_one_field() {
        use crate::tui::screens::edit::EditField;
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key_with(KeyCode::BackTab, KeyModifiers::SHIFT));
        if let Screen::Edit(state) = &app.screen {
            assert_eq!(state.field, EditField::Priority);
        } else {
            panic!("expected Edit");
        }
    }

    #[test]
    fn esc_closes_clean_add_form() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn esc_closes_clean_edit_form() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('e')));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn typing_into_title_appends_chars() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        for c in "buy milk".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        if let Screen::Edit(state) = &app.screen {
            assert_eq!(state.title.value(), "buy milk");
        } else {
            panic!();
        }
    }

    #[test]
    fn typing_into_priority_field_works() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Char('3')));
        if let Screen::Edit(state) = &app.screen {
            assert_eq!(state.priority.value(), "3");
        } else {
            panic!();
        }
    }

    #[test]
    fn typing_into_body_textarea_inserts_text() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        for _ in 0..4 {
            app.handle_key(key(KeyCode::Tab));
        }
        for c in "hi".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        if let Screen::Edit(state) = &app.screen {
            assert_eq!(state.body.lines().join("\n"), "hi");
        } else {
            panic!();
        }
    }

    #[test]
    fn enter_in_body_textarea_inserts_newline() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        for _ in 0..4 {
            app.handle_key(key(KeyCode::Tab));
        }
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Char('b')));
        if let Screen::Edit(state) = &app.screen {
            assert_eq!(state.body.lines(), &["a", "b"]);
        } else {
            panic!();
        }
    }

    #[test]
    fn backspace_in_title_deletes_char() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        for c in "abc".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Backspace));
        if let Screen::Edit(state) = &app.screen {
            assert_eq!(state.title.value(), "ab");
        } else {
            panic!();
        }
    }

    #[test]
    fn confirm_modal_renders_with_task_title() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = mem_app_with(&[("buy milk", "open", 2)]);
        app.handle_key(key(KeyCode::Char('x')));
        let backend = TestBackend::new(120, 60);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<Vec<_>>()
            .join("");
        assert!(s.contains("delete task"));
        assert!(s.contains("buy milk"));
        assert!(s.contains("y/Enter"));
        assert!(s.contains("n/Esc"));
    }

    #[test]
    fn main_screen_renders_top_bar_and_footer() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let app = mem_app_with(&[("hello", "open", 2)]);
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<Vec<_>>()
            .join("");
        assert!(s.contains("docket"));
        assert!(s.contains("hello"));
        assert!(s.contains("quit"));
    }

    #[test]
    fn help_screen_renders_on_top_of_main() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = mem_app_with(&[("hello", "open", 2)]);
        app.screen = Screen::Help;
        let backend = TestBackend::new(120, 60);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<Vec<_>>()
            .join("");
        assert!(s.contains("help"));
        assert!(s.contains("Press ? or Esc"));
    }

    #[test]
    fn filter_prompt_renders() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('/')));
        app.handle_key(key(KeyCode::Char('h')));
        app.handle_key(key(KeyCode::Char('i')));
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<Vec<_>>()
            .join("");
        assert!(s.contains("Filter title/body"));
        assert!(s.contains("hi"));
    }
}
