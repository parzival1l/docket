use crate::cli::start::TmuxDelivery;
use crate::db::{load_all_tasks, open_db};
use crate::model::Task;
use crate::tui::filters::{filtered_indices, Filters};
use crate::tui::screens::edit::{EditMode, EditState};
use crate::tui::screens::{FilterKind, PendingAction, Screen, SessionPickerState};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use rusqlite::Connection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    List,
    Detail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartRequest {
    pub id: i64,
    pub delivery: TmuxDelivery,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSessionRequest {
    pub task_id: i64,
    pub session_id: String,
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
    pub pending_start: Option<StartRequest>,
    pub pending_open_session: Option<OpenSessionRequest>,
    /// Vertical scroll offset for the detail pane, in lines. Reset to zero
    /// whenever the selected task changes (cursor move, view cycle, reload).
    pub detail_scroll: u16,
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
            pending_start: None,
            pending_open_session: None,
            detail_scroll: 0,
        })
    }

    pub fn reload(&mut self) -> Result<()> {
        self.tasks = load_all_tasks(&self.conn)?;
        self.clamp_cursor();
        self.detail_scroll = 0;
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
            Screen::SessionPicker(_) => self.handle_session_picker(key),
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
                KeyCode::Char('b') => {
                    self.filters.blocked_only = !self.filters.blocked_only;
                    if self.filters.blocked_only {
                        self.filters.ready_only = false;
                    }
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
            (KeyCode::Char('s'), m) if m.contains(KeyModifiers::CONTROL) => {
                if self.focus == Pane::List {
                    self.request_start(TmuxDelivery::ForceSpawn);
                }
                return;
            }
            (KeyCode::Tab, _) => {
                if self.focus == Pane::List {
                    self.filters.view = self.filters.view.next();
                    self.clamp_cursor();
                    self.detail_scroll = 0;
                }
                return;
            }
            (KeyCode::Char('O'), _) => {
                self.request_open_session();
                return;
            }
            _ => {}
        }

        match self.focus {
            Pane::List => self.handle_list(key),
            Pane::Detail => self.handle_detail(key),
        }
    }

    /// Try to open one of the agent sessions linked to the currently-selected
    /// task. 0 sessions → silent noop; 1 → dispatch directly; 2+ → open the
    /// session-picker modal. Enriches each session id with title / turns /
    /// last-activity from Claude's local transcript when available.
    fn request_open_session(&mut self) {
        let Some(task) = self.selected_task() else {
            return;
        };
        let task_id = task.id;
        let task_title = task.title.clone();

        // Pull (id, linked_at) pairs from the db so the fallback display has
        // a "linked X ago" timestamp for ids without transcripts on disk.
        let links = crate::db::task_session_links(&self.conn, task_id).unwrap_or_default();
        let infos: Vec<crate::agent_session_info::SessionInfo> = links
            .into_iter()
            .map(|(sid, linked_at_str)| {
                let linked_at = chrono::DateTime::parse_from_rfc3339(&linked_at_str)
                    .ok()
                    .map(|dt| dt.with_timezone(&chrono::Utc));
                crate::agent_session_info::inspect(&sid, linked_at)
            })
            .collect();

        match infos.len() {
            0 => {}
            1 => {
                self.pending_open_session = Some(OpenSessionRequest {
                    task_id,
                    session_id: infos.into_iter().next().unwrap().id,
                });
                self.should_quit = true;
            }
            _ => {
                self.screen = Screen::SessionPicker(SessionPickerState {
                    task_id,
                    task_title,
                    sessions: infos,
                    cursor: 0,
                });
            }
        }
    }

    fn handle_session_picker(&mut self, key: KeyEvent) {
        let Screen::SessionPicker(state) = &mut self.screen else {
            return;
        };
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.screen = Screen::Main;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !state.sessions.is_empty() && state.cursor + 1 < state.sessions.len() {
                    state.cursor += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                state.cursor = state.cursor.saturating_sub(1);
            }
            KeyCode::Char('g') => {
                state.cursor = 0;
            }
            KeyCode::Char('G') => {
                state.cursor = state.sessions.len().saturating_sub(1);
            }
            KeyCode::Enter => {
                let session_id = state
                    .sessions
                    .get(state.cursor)
                    .map(|s| s.id.clone())
                    .unwrap_or_default();
                let task_id = state.task_id;
                if !session_id.is_empty() {
                    self.pending_open_session = Some(OpenSessionRequest {
                        task_id,
                        session_id,
                    });
                    self.should_quit = true;
                }
            }
            _ => {}
        }
    }

    fn handle_list(&mut self, key: KeyEvent) {
        let visible_len = self.visible_indices().len();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if visible_len > 0 && self.cursor + 1 < visible_len {
                    self.cursor += 1;
                    self.detail_scroll = 0;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let prev = self.cursor;
                self.cursor = self.cursor.saturating_sub(1);
                if self.cursor != prev {
                    self.detail_scroll = 0;
                }
            }
            KeyCode::Char('g') => {
                self.cursor = 0;
                self.detail_scroll = 0;
            }
            KeyCode::Char('G') => {
                self.cursor = visible_len.saturating_sub(1);
                self.detail_scroll = 0;
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

    fn request_start(&mut self, delivery: TmuxDelivery) {
        if let Some(task) = self.selected_task() {
            self.pending_start = Some(StartRequest {
                id: task.id,
                delivery,
            });
            self.should_quit = true;
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
        // Page size for PgUp/PgDn; the real viewport height isn't known until
        // render, so we use a sane default. Render clamps the offset so any
        // overshoot self-corrects on the next frame.
        const PAGE: u16 = 10;
        match key.code {
            KeyCode::Char('h') | KeyCode::Left => {
                self.focus = Pane::List;
            }
            KeyCode::Char('e') => {
                self.open_edit_form();
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.detail_scroll = self.detail_scroll.saturating_add(1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.detail_scroll = self.detail_scroll.saturating_sub(1);
            }
            KeyCode::PageDown => {
                self.detail_scroll = self.detail_scroll.saturating_add(PAGE);
            }
            KeyCode::PageUp => {
                self.detail_scroll = self.detail_scroll.saturating_sub(PAGE);
            }
            KeyCode::Char('g') => {
                self.detail_scroll = 0;
            }
            KeyCode::Char('G') => {
                self.detail_scroll = u16::MAX;
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

    fn save_edit(&mut self) {
        let Screen::Edit(state) = &mut self.screen else {
            return;
        };
        let validated = match state.validate() {
            Ok(v) => v,
            Err(msg) => {
                state.error = Some(msg);
                return;
            }
        };
        let mode = state.mode;

        let group_id = match validated.group.as_deref() {
            None => None,
            Some(name) => match crate::db::get_or_create_group(&self.conn, name) {
                Ok(id) => Some(id),
                Err(e) => {
                    state.error = Some(format!("group: {}", e));
                    return;
                }
            },
        };

        let (orig_sessions, new_sessions) = match mode {
            EditMode::Edit { .. } => (state.orig_sessions_list(), state.sessions_list()),
            EditMode::Add => (Vec::new(), Vec::new()),
        };

        let result = match mode {
            EditMode::Add => crate::db::insert_task(
                &self.conn,
                crate::db::NewTask {
                    title: &validated.title,
                    body: validated.body.as_deref(),
                    acceptance: validated.acceptance.as_deref(),
                    deps_json: validated.deps_json.as_deref(),
                    priority: validated.priority,
                    group_id,
                    kind: &validated.kind,
                    status: "open",
                },
            ),
            EditMode::Edit { id } => crate::db::update_task(
                &self.conn,
                id,
                crate::db::TaskUpdate {
                    title: &validated.title,
                    body: validated.body.as_deref(),
                    acceptance: validated.acceptance.as_deref(),
                    deps_json: validated.deps_json.as_deref(),
                    priority: validated.priority,
                    group_id,
                    kind: &validated.kind,
                },
            )
            .map(|_| id),
        };

        match result {
            Ok(id) => {
                if let EditMode::Edit { id: tid } = mode {
                    if let Err(e) = self.sync_session_links(tid, &orig_sessions, &new_sessions) {
                        if let Screen::Edit(state) = &mut self.screen {
                            state.error = Some(format!("sessions: {}", e));
                        }
                        let _ = self.reload();
                        return;
                    }
                }
                let _ = self.reload();
                self.preserve_cursor_on(id);
                self.screen = Screen::Main;
            }
            Err(e) => {
                if let Screen::Edit(state) = &mut self.screen {
                    state.error = Some(format!("db: {}", e));
                }
            }
        }
    }

    /// Apply additions and removals so the `agent_sessions` table reflects
    /// the user's edited list. Adds are upserts (`link_session` moves a
    /// session id from another task if it was previously linked there);
    /// removals only fire for ids that were on the original list and are
    /// no longer present.
    fn sync_session_links(
        &self,
        task_id: i64,
        orig: &[String],
        new: &[String],
    ) -> anyhow::Result<()> {
        for sid in new {
            if !orig.iter().any(|o| o == sid) {
                crate::db::link_session(&self.conn, task_id, sid)?;
            }
        }
        for sid in orig {
            if !new.iter().any(|n| n == sid) {
                crate::db::unlink_session(&self.conn, sid)?;
            }
        }
        Ok(())
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
            (KeyCode::Char('s'), m) if m.contains(KeyModifiers::CONTROL) => {
                self.save_edit();
                return;
            }
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
                let dirty = matches!(&self.screen, Screen::Edit(state) if state.is_dirty());
                if dirty {
                    self.screen = Screen::Confirm(PendingAction::DiscardEdits);
                } else {
                    self.screen = Screen::Main;
                }
                return;
            }
            _ => {}
        }

        let Screen::Edit(state) = &mut self.screen else {
            return;
        };
        // Clear any stale save-attempt error as soon as the user resumes
        // typing; live per-field error takes over for ongoing feedback.
        state.error = None;
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
            EditField::Kind => match key.code {
                KeyCode::Right | KeyCode::Char('l') => state.cycle_kind_forward(),
                KeyCode::Left | KeyCode::Char('h') => state.cycle_kind_backward(),
                _ => {}
            },
            EditField::Group => {
                let _ = state.group.handle_event(&event);
            }
            EditField::Deps => {
                let _ = state.deps.handle_event(&event);
            }
            EditField::Sessions => {
                let _ = state.sessions.handle_event(&event);
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

    pub fn render(&mut self, frame: &mut Frame) {
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
                    Screen::SessionPicker(state) => {
                        crate::tui::screens::session_picker::render(frame, state)
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
            pending_start: None,
            pending_open_session: None,
            detail_scroll: 0,
        }
    }

    /// Seed a task with the given linked session ids. Returns the task id.
    fn seed_task_with_sessions(app: &mut App, title: &str, sessions: &[&str]) -> i64 {
        let id = insert_task(
            &app.conn,
            NewTask {
                title,
                body: None,
                acceptance: None,
                deps_json: None,
                priority: 2,
                group_id: None,
                kind: "feature",
                status: "open",
            },
        )
        .unwrap();
        for s in sessions {
            crate::db::link_session(&app.conn, id, s).unwrap();
        }
        app.reload().unwrap();
        id
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
                    kind: "feature",
                    status: "open",
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
    fn pgdn_in_detail_increments_scroll() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::PageDown));
        assert!(
            app.detail_scroll > 0,
            "PgDn in detail pane must advance detail_scroll"
        );
    }

    #[test]
    fn pgup_in_detail_decrements_scroll_saturating() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::PageDown));
        app.handle_key(key(KeyCode::PageDown));
        app.handle_key(key(KeyCode::PageUp));
        app.handle_key(key(KeyCode::PageUp));
        app.handle_key(key(KeyCode::PageUp));
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn j_k_in_detail_pane_scrolls_one_line() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.detail_scroll, 1);
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.detail_scroll, 2);
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.detail_scroll, 1);
    }

    #[test]
    fn g_in_detail_resets_scroll_to_zero() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::PageDown));
        app.handle_key(key(KeyCode::Char('g')));
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn cursor_change_resets_detail_scroll() {
        let mut app = mem_app_with(&[("a", "open", 2), ("b", "open", 2)]);
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::PageDown));
        assert!(app.detail_scroll > 0);
        app.handle_key(key(KeyCode::Char('h'))); // back to list
        app.handle_key(key(KeyCode::Char('j'))); // move cursor down
        assert_eq!(
            app.detail_scroll, 0,
            "Moving the cursor must reset detail scroll so the new task starts at the top"
        );
    }

    #[test]
    fn h_in_detail_does_not_scroll() {
        // Regression guard: h is "back to list", not "scroll left".
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('l')));
        app.handle_key(key(KeyCode::Char('h')));
        assert_eq!(app.focus, Pane::List);
        assert_eq!(app.detail_scroll, 0);
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
    fn f_then_b_toggles_blocked_filter_and_clears_ready() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.filters.ready_only = true;
        app.handle_key(key(KeyCode::Char('f')));
        app.handle_key(key(KeyCode::Char('b')));
        assert!(app.filters.blocked_only);
        assert!(!app.filters.ready_only);
    }

    #[test]
    fn tab_cycles_view_active_done_backlog_active() {
        use crate::tui::filters::ViewMode;
        let mut app = mem_app();
        assert_eq!(app.filters.view, ViewMode::Active);
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.filters.view, ViewMode::Done);
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.filters.view, ViewMode::Backlog);
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.filters.view, ViewMode::Active);
    }

    #[test]
    fn default_view_hides_backlog_and_shows_in_progress() {
        let app = mem_app_with(&[
            ("active", "open", 2),
            ("running", "in_progress", 2),
            ("shipped", "done", 2),
            ("later", "backlog", 2),
        ]);
        let titles: Vec<&str> = app
            .visible_indices()
            .iter()
            .map(|i| app.tasks[*i].title.as_str())
            .collect();
        assert_eq!(titles, vec!["active", "running"]);
    }

    #[test]
    fn backlog_view_after_two_tabs_shows_only_backlog() {
        let mut app = mem_app_with(&[
            ("active", "open", 2),
            ("shipped", "done", 2),
            ("later", "backlog", 2),
        ]);
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Tab));
        let titles: Vec<&str> = app
            .visible_indices()
            .iter()
            .map(|i| app.tasks[*i].title.as_str())
            .collect();
        assert_eq!(titles, vec!["later"]);
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
        // Default view already excludes "done"; cycling to backlog leaves a
        // single visible task ("b" status=backlog) so a cursor parked at the
        // second slot must clamp back to 0.
        let mut app = mem_app_with(&[("a", "open", 2), ("b", "backlog", 2)]);
        app.cursor = 1;
        // Active view only shows "a" — cursor already needs clamping.
        app.clamp_cursor();
        app.cursor = 1;
        // Tab × 2 → backlog view, "b" is the only entry.
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Tab));
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

    /// Look up a task's status by title — tests that mutate status often
    /// push a task out of the current view, so cursor-relative access
    /// returns None.
    fn status_of(app: &App, title: &str) -> String {
        app.tasks
            .iter()
            .find(|t| t.title == title)
            .map(|t| t.status.clone())
            .unwrap_or_else(|| format!("(no task titled {})", title))
    }

    #[test]
    fn s_cycles_open_to_in_progress() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(status_of(&app, "a"), "in_progress");
    }

    #[test]
    fn s_cycles_in_progress_to_done() {
        let mut app = mem_app_with(&[("a", "in_progress", 2)]);
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(status_of(&app, "a"), "done");
    }

    #[test]
    fn s_cycles_done_back_to_open() {
        // "done" sits in the Done view, not Active. Tab once so the cursor
        // can land on it, then `s` cycles it forward to open.
        let mut app = mem_app_with(&[("a", "done", 2)]);
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(status_of(&app, "a"), "open");
    }

    #[test]
    fn s_cycles_custom_status_to_open() {
        // "wontfix" doesn't belong to any of the three views, so the cursor
        // can't reach it from the TUI today; this exercises the underlying
        // next_status() mapping directly via set_status.
        let mut app = mem_app_with(&[("a", "open", 2)]);
        let id = app.selected_task().unwrap().id;
        crate::db::set_status(&app.conn, id, "wontfix").unwrap();
        app.reload().unwrap();
        // Manually invoke the cycle path by hopping into Done view (still
        // doesn't include wontfix). Instead, assert next_status mapping:
        assert_eq!(next_status("wontfix"), "open");
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
        assert_eq!(status_of(&app, "a"), "done");
    }

    #[test]
    fn d_marks_in_progress_task_done() {
        let mut app = mem_app_with(&[("a", "in_progress", 2)]);
        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(status_of(&app, "a"), "done");
    }

    #[test]
    fn d_on_already_done_task_stays_done() {
        let mut app = mem_app_with(&[("a", "done", 2)]);
        app.handle_key(key(KeyCode::Tab)); // hop to Done view to reach the task
        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(status_of(&app, "a"), "done");
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
        // Tab order: Title → Priority → Kind → Group → Deps → Body.
        for _ in 0..5 {
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
        for _ in 0..5 {
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
    fn ctrl_s_save_creates_new_task_in_add_mode() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        for c in "from form".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key_with(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert_eq!(app.screen, Screen::Main);
        assert_eq!(app.tasks.len(), 1);
        assert_eq!(app.tasks[0].title, "from form");
        assert_eq!(app.tasks[0].priority, 2);
    }

    #[test]
    fn ctrl_s_save_updates_existing_task_in_edit_mode() {
        let mut app = mem_app_with(&[("orig", "open", 2)]);
        app.handle_key(key(KeyCode::Char('e')));
        for _ in 0.."orig".len() {
            app.handle_key(key(KeyCode::Backspace));
        }
        for c in "renamed".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key_with(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert_eq!(app.screen, Screen::Main);
        assert_eq!(app.tasks.len(), 1);
        assert_eq!(app.tasks[0].title, "renamed");
    }

    #[test]
    fn ctrl_s_save_with_invalid_title_keeps_form_open_with_error() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key_with(KeyCode::Char('s'), KeyModifiers::CONTROL));
        match &app.screen {
            Screen::Edit(state) => {
                assert!(state.error.as_deref().unwrap().contains("title"));
            }
            _ => panic!("expected Edit"),
        }
    }

    #[test]
    fn ctrl_s_save_priority_out_of_range_keeps_form_open() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        for c in "ok".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Char('9')));
        app.handle_key(key_with(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(matches!(app.screen, Screen::Edit(_)));
        assert_eq!(app.tasks.len(), 0);
    }

    #[test]
    fn esc_on_dirty_form_opens_discard_confirm() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Confirm(PendingAction::DiscardEdits));
    }

    #[test]
    fn discard_confirm_y_returns_to_main_and_drops_edits() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Char('y')));
        assert_eq!(app.screen, Screen::Main);
        assert!(app.tasks.is_empty());
    }

    #[test]
    fn capital_s_no_longer_starts_task() {
        // Shift+S used to spawn a start; users typed it accidentally. Ctrl+S
        // is now the only start key, so plain Shift+S must be inert in list
        // scope.
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key_with(KeyCode::Char('S'), KeyModifiers::SHIFT));
        assert!(!app.should_quit);
        assert!(app.pending_start.is_none());
    }

    #[test]
    fn ctrl_s_in_list_sets_pending_start_with_force_spawn_and_quits() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        let id = app.selected_task().unwrap().id;
        app.handle_key(key_with(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
        assert_eq!(
            app.pending_start,
            Some(StartRequest {
                id,
                delivery: TmuxDelivery::ForceSpawn
            })
        );
    }

    #[test]
    fn ctrl_s_in_list_on_empty_is_a_noop() {
        let mut app = mem_app();
        app.handle_key(key_with(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(!app.should_quit);
        assert!(app.pending_start.is_none());
    }

    #[test]
    fn edit_form_renders_with_field_labels() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
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
        assert!(s.contains("new task"));
        assert!(s.contains("Title"));
        assert!(s.contains("Priority"));
        assert!(s.contains("Body"));
        assert!(s.contains("Acceptance"));
        assert!(s.contains("Ctrl+S"));
    }

    #[test]
    fn edit_form_renders_typed_title_text() {
        // Regression: tui-input rows were 2 tall; the bordered block ate
        // both rows leaving zero rows for content, so typed text never
        // appeared on screen even though the Input state was correct.
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        for c in "hello".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
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
        assert!(s.contains("hello"), "expected typed title in rendered buffer");
    }

    #[test]
    fn edit_form_renders_priority_placeholder_when_empty() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
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
        assert!(s.contains("0..4"), "priority placeholder should show");
        assert!(s.contains("T-3, T-5"), "deps placeholder should show");
    }

    #[test]
    fn edit_form_live_error_shows_when_priority_invalid() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        // Give title a value so it doesn't dominate the live-error slot.
        for c in "ok".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Tab)); // → Priority
        app.handle_key(key(KeyCode::Char('9')));
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
        assert!(s.contains("0..=4"), "live error should mention 0..=4");
    }

    #[test]
    fn typing_after_save_error_clears_it() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        // Trigger save with blank title to set state.error.
        app.handle_key(key_with(KeyCode::Char('s'), KeyModifiers::CONTROL));
        if let Screen::Edit(state) = &app.screen {
            assert!(state.error.is_some());
        } else {
            panic!();
        }
        // Now type a character — the stale save error should clear.
        app.handle_key(key(KeyCode::Char('h')));
        if let Screen::Edit(state) = &app.screen {
            assert!(state.error.is_none());
        } else {
            panic!();
        }
    }

    #[test]
    fn edit_form_renders_validation_error() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key_with(KeyCode::Char('s'), KeyModifiers::CONTROL));
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
        assert!(s.contains("title"));
    }

    #[test]
    fn discard_confirm_n_returns_to_main_today() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Char('n')));
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn ctrl_s_save_in_add_mode_creates_with_group_and_deps() {
        let mut app = mem_app_with(&[("dep", "open", 2)]);
        let dep_id = app.selected_task().unwrap().id;
        app.handle_key(key(KeyCode::Char('n')));
        for c in "child".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        // Tab order: Title → Priority → Kind → Group → Deps.
        // Step from Title to Group (3 tabs), enter the group name, then to
        // Deps (1 tab).
        for _ in 0..3 {
            app.handle_key(key(KeyCode::Tab));
        }
        for c in "v0.2".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Tab));
        for c in format!("T-{}", dep_id).chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key_with(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert_eq!(app.screen, Screen::Main);
        let child = app
            .tasks
            .iter()
            .find(|t| t.title == "child")
            .expect("child created");
        assert_eq!(child.group.as_deref(), Some("v0.2"));
        assert_eq!(child.deps, vec![dep_id]);
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
        let mut app = mem_app_with(&[("hello", "open", 2)]);
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
        assert!(s.contains("active"), "top bar should show current view chip");
    }

    #[test]
    fn top_bar_chip_updates_after_tab_cycles_to_backlog() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = mem_app_with(&[("hi", "backlog", 2)]);
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Tab));
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let s: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<Vec<_>>()
            .join("");
        assert!(s.contains("backlog"));
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

    // ---------- kind handling ----------

    fn seed_task_with_kind(app: &mut App, title: &str, kind: &str) -> i64 {
        let id = insert_task(
            &app.conn,
            NewTask {
                title,
                body: None,
                acceptance: None,
                deps_json: None,
                priority: 2,
                group_id: None,
                kind,
                status: "open",
            },
        )
        .unwrap();
        app.reload().unwrap();
        id
    }

    #[test]
    fn edit_modal_cycles_kind_with_arrow_keys_and_save_persists_it() {
        let mut app = mem_app();
        let id = seed_task_with_kind(&mut app, "alpha", "feature");

        // Open the edit form on the seeded task.
        app.handle_key(key(KeyCode::Char('e')));

        // Tab from Title → Priority → Kind.
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Tab));

        // Cycle Right once: feature (index 1 in KINDS) should move to chore.
        app.handle_key(key(KeyCode::Right));

        // Save.
        app.handle_key(key_with(KeyCode::Char('s'), KeyModifiers::CONTROL));

        assert_eq!(app.screen, Screen::Main);
        let task = app.tasks.iter().find(|t| t.id == id).expect("task missing");
        assert_eq!(
            task.kind, "chore",
            "Right arrow on Kind field should cycle feature → chore and save must persist it"
        );
    }

    #[test]
    #[ignore]
    fn dump_edit_modal_for_human_inspection() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = mem_app();
        seed_task_with_kind(&mut app, "alpha", "feature");
        app.handle_key(key(KeyCode::Char('e')));
        // Focus the Kind row so the dropdown affordance is visible.
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Tab));

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let area = buf.area();
        eprintln!("---- edit modal (Kind focused) ----");
        for y in 0..area.height {
            let mut row = String::new();
            for x in 0..area.width {
                row.push_str(buf[(x, y)].symbol());
            }
            eprintln!("{}", row);
        }
    }

    #[test]
    fn edit_modal_renders_kind_field_with_current_value() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = mem_app();
        seed_task_with_kind(&mut app, "alpha", "chore");
        app.handle_key(key(KeyCode::Char('e')));

        let backend = TestBackend::new(120, 60);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let area = buf.area();
        let mut rows: Vec<String> = Vec::new();
        for y in 0..area.height {
            let mut row = String::new();
            for x in 0..area.width {
                row.push_str(buf[(x, y)].symbol());
            }
            rows.push(row);
        }
        let dump = rows.join("\n");
        assert!(
            dump.contains("Kind"),
            "edit modal should render a `Kind` label; got:\n{}",
            dump
        );
        assert!(
            dump.contains("chore"),
            "edit modal should show the task's current kind `chore`; got:\n{}",
            dump
        );
    }

    #[test]
    fn editing_a_task_preserves_its_kind() {
        let mut app = mem_app();
        let id = seed_task_with_kind(&mut app, "the bug", "bug");

        // Open the edit form; bump the title (form has to be dirty enough to
        // save) but never touch kind.
        app.handle_key(key(KeyCode::Char('e')));
        app.handle_key(key(KeyCode::Char('!')));
        app.handle_key(key_with(KeyCode::Char('s'), KeyModifiers::CONTROL));

        let task = app.tasks.iter().find(|t| t.id == id).expect("task missing");
        assert_eq!(
            task.kind, "bug",
            "TUI edit must preserve kind, not silently reset it to feature"
        );
    }

    /// Render the full TUI and slice each row down to just the list pane
    /// (its `Constraint::Length(40)` plus a tiny buffer for the border).
    fn render_list_pane() -> impl Fn(&mut App) -> String {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        move |app: &mut App| {
            let backend = TestBackend::new(120, 60);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal.draw(|f| app.render(f)).unwrap();
            let buf = terminal.backend().buffer().clone();
            let area = buf.area();
            let mut rows: Vec<String> = Vec::new();
            let list_width = 40u16.min(area.width);
            for y in 0..area.height {
                let mut row = String::new();
                for x in 0..list_width {
                    row.push_str(buf[(x, y)].symbol());
                }
                rows.push(row);
            }
            rows.join("\n")
        }
    }

    #[test]
    fn task_list_renders_kind_inline() {
        let mut app = mem_app();
        // Title intentionally free of any vocabulary word so a hit can only
        // come from the kind cell, not a title substring.
        seed_task_with_kind(&mut app, "alpha", "bug");

        let list = render_list_pane()(&mut app);
        assert!(
            list.contains("bug") && !list.contains("[bug]"),
            "task list row should surface kind shorthand `bug` without brackets; got:\n{}",
            list
        );
    }

    #[test]
    fn task_list_renders_kind_as_three_letter_shorthand() {
        let mut app = mem_app();
        seed_task_with_kind(&mut app, "alpha", "feature");
        seed_task_with_kind(&mut app, "beta", "chore");

        let list = render_list_pane()(&mut app);
        assert!(
            list.contains("fea") && !list.contains("[feature]"),
            "feature task should render as `fea` shorthand, no brackets; got:\n{}",
            list
        );
        assert!(
            list.contains("cho") && !list.contains("[chore]"),
            "chore task should render as `cho` shorthand, no brackets; got:\n{}",
            list
        );
    }

    // ---------- agent_sessions / open-session picker ----------

    #[test]
    fn capital_o_with_no_sessions_is_a_noop() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('O')));
        assert!(app.pending_open_session.is_none());
        assert!(!app.should_quit);
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn capital_o_with_one_session_dispatches_directly_and_quits() {
        let mut app = mem_app();
        let id = seed_task_with_sessions(&mut app, "alpha", &["sess-only"]);
        app.handle_key(key(KeyCode::Char('O')));
        assert!(app.should_quit, "single-session O should quit the TUI");
        assert_eq!(
            app.pending_open_session,
            Some(OpenSessionRequest {
                task_id: id,
                session_id: "sess-only".into(),
            })
        );
    }

    #[test]
    fn capital_o_with_multiple_sessions_opens_picker_modal() {
        let mut app = mem_app();
        let id = seed_task_with_sessions(&mut app, "alpha", &["s1", "s2", "s3"]);
        app.handle_key(key(KeyCode::Char('O')));
        match &app.screen {
            Screen::SessionPicker(state) => {
                assert_eq!(state.task_id, id);
                assert_eq!(state.sessions.len(), 3);
                assert_eq!(state.cursor, 0);
            }
            other => panic!("expected SessionPicker, got {:?}", other),
        }
        assert!(!app.should_quit, "picker should NOT quit immediately");
    }

    #[test]
    fn session_picker_j_k_moves_cursor() {
        let mut app = mem_app();
        seed_task_with_sessions(&mut app, "alpha", &["s1", "s2", "s3"]);
        app.handle_key(key(KeyCode::Char('O')));
        app.handle_key(key(KeyCode::Char('j')));
        if let Screen::SessionPicker(state) = &app.screen {
            assert_eq!(state.cursor, 1);
        } else {
            panic!("expected SessionPicker");
        }
        app.handle_key(key(KeyCode::Char('k')));
        if let Screen::SessionPicker(state) = &app.screen {
            assert_eq!(state.cursor, 0);
        } else {
            panic!();
        }
    }

    #[test]
    fn session_picker_j_clamps_at_last_session() {
        let mut app = mem_app();
        seed_task_with_sessions(&mut app, "alpha", &["s1", "s2"]);
        app.handle_key(key(KeyCode::Char('O')));
        app.handle_key(key(KeyCode::Char('j')));
        app.handle_key(key(KeyCode::Char('j')));
        app.handle_key(key(KeyCode::Char('j')));
        if let Screen::SessionPicker(state) = &app.screen {
            assert_eq!(state.cursor, 1);
        } else {
            panic!();
        }
    }

    #[test]
    fn session_picker_enter_dispatches_focused_session_and_quits() {
        let mut app = mem_app();
        let id = seed_task_with_sessions(&mut app, "alpha", &["s1", "s2", "s3"]);
        app.handle_key(key(KeyCode::Char('O')));
        app.handle_key(key(KeyCode::Char('j'))); // cursor → 1 → "s2"
        app.handle_key(key(KeyCode::Enter));
        assert!(app.should_quit);
        assert_eq!(
            app.pending_open_session,
            Some(OpenSessionRequest {
                task_id: id,
                session_id: "s2".into(),
            })
        );
    }

    #[test]
    fn session_picker_esc_cancels_returns_to_main() {
        let mut app = mem_app();
        seed_task_with_sessions(&mut app, "alpha", &["s1", "s2"]);
        app.handle_key(key(KeyCode::Char('O')));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Main);
        assert!(app.pending_open_session.is_none());
        assert!(!app.should_quit);
    }

    #[test]
    fn capital_o_on_empty_list_is_a_noop() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('O')));
        assert!(app.pending_open_session.is_none());
        assert!(!app.should_quit);
    }

    #[test]
    fn edit_modal_renders_sessions_line_for_task_with_sessions() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = mem_app();
        seed_task_with_sessions(&mut app, "alpha", &["sess-abc-123"]);
        app.handle_key(key(KeyCode::Char('e')));

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
        assert!(
            s.contains("Sessions"),
            "edit modal should surface a `Sessions` label; got:\n{}",
            s
        );
        assert!(
            s.contains("sess-abc-123"),
            "edit modal should list the linked session id; got:\n{}",
            s
        );
    }

    #[test]
    fn session_picker_modal_renders_task_id_and_session_ids() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = mem_app();
        let id = seed_task_with_sessions(&mut app, "widget", &["s-one", "s-two"]);
        app.handle_key(key(KeyCode::Char('O')));

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<Vec<_>>()
            .join("");
        assert!(
            s.contains(&crate::model::fmt_id(id)),
            "picker should show task id; got:\n{}",
            s
        );
        assert!(s.contains("s-one"), "picker should list session s-one");
        assert!(s.contains("s-two"), "picker should list session s-two");
        assert!(
            s.contains("resume"),
            "picker title should reference resuming a session; got:\n{}",
            s
        );
        assert!(
            s.contains("Pick one to resume"),
            "header copy should prompt the user; got:\n{}",
            s
        );
        assert!(
            s.to_lowercase().contains("no transcript"),
            "test sessions have no transcript on disk — picker should say so; got:\n{}",
            s
        );
    }

    #[test]
    fn task_detail_renders_body_without_raw_hash_markers() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = mem_app();
        // Seed a task with markdown in body. The detail pane used to print
        // literal `## body` and the body's own `##` lines verbatim; now they
        // should render as styled section text without hash characters.
        let id = insert_task(
            &app.conn,
            NewTask {
                title: "alpha",
                body: Some("## Setup\n\n- step one with **bold**\n- step two"),
                acceptance: Some("- pass `cargo test`"),
                deps_json: None,
                priority: 2,
                group_id: None,
                kind: "feature",
                status: "open",
            },
        )
        .unwrap();
        app.reload().unwrap();
        let _ = id;

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let s: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<Vec<_>>()
            .join("");
        // No literal `## body` / `## acceptance` headers anymore — they were
        // the eyesore the markdown rewrite is meant to fix.
        assert!(
            !s.contains("## body"),
            "detail should not show literal `## body`; got:\n{}",
            s
        );
        assert!(
            !s.contains("## acceptance"),
            "detail should not show literal `## acceptance`; got:\n{}",
            s
        );
        assert!(
            !s.contains("## Setup"),
            "user-authored `## Setup` should be rendered as a heading, not raw markdown; got:\n{}",
            s
        );
        // The actual content survives.
        assert!(s.contains("Setup"));
        assert!(s.contains("step one"));
        assert!(s.contains("step two"));
        // Bullet glyph replaces the leading `-`.
        assert!(s.contains("•"), "bullets should render as `•`; got:\n{}", s);
        // Bold marker is consumed.
        assert!(!s.contains("**"));
    }

    #[test]
    fn task_detail_renders_kind() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = mem_app();
        seed_task_with_kind(&mut app, "alpha", "chore");

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
        assert!(
            s.contains("chore"),
            "detail pane should surface kind `chore`; got:\n{}",
            s
        );
    }
}
