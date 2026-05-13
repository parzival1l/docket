use crate::db::{load_all_tasks, open_db};
use crate::model::Task;
use crate::tui::filters::{filtered_indices, Filters};
#[allow(unused_imports)] // PendingAction is referenced starting in Task 6
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
            _ => {}
        }
    }

    fn handle_detail(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Char('h') | KeyCode::Left) {
            self.focus = Pane::List;
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

    fn handle_confirm(&mut self, _key: KeyEvent) {
        // Real handler lands in Task 7.
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

    pub fn render(&self, frame: &mut Frame) {
        crate::tui::screens::main::render(self, frame);
        match &self.screen {
            Screen::Help => crate::tui::screens::help::render(frame),
            Screen::FilterPrompt { kind, input } => {
                crate::tui::screens::filter_prompt::render(frame, kind, input)
            }
            Screen::Confirm(action) => crate::tui::screens::confirm::render(frame, action),
            Screen::Main => {}
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
