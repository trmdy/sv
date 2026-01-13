use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::error::{Error, Result};
use crate::task::{TaskDetails, TaskRecord, TaskStore};

use super::cache::RenderCache;
use super::model;
use super::view;

const NARROW_WIDTH: u16 = 90;
const EVENT_POLL_MS: u64 = 120;
const WATCH_DEBOUNCE_MS: u64 = 200;

enum LoadRequest {
    Reload,
    Details(String),
}

enum UiMsg {
    DataLoaded(Vec<TaskRecord>),
    LoadError(String),
    DetailsLoaded(String, TaskDetails),
    DetailsError(String, String),
    WatchError(String),
}

#[derive(Default, Clone, Copy)]
struct Viewport {
    width: u16,
    height: u16,
}

pub struct AppState {
    pub(crate) tasks: Vec<TaskRecord>,
    pub(crate) filtered: Vec<usize>,
    pub(crate) selected: Option<usize>,
    pub(crate) filter: String,
    pub(crate) filter_active: bool,
    pub(crate) status_filter: Option<String>,
    detail_cache: HashMap<String, TaskDetails>,
    pending_details: HashSet<String>,
    status_message: Option<String>,
    watch_error: Option<String>,
    viewport: Viewport,
    pub(crate) show_detail: bool,
    pub(crate) cache: RenderCache,
    config: crate::config::TasksConfig,
}

impl AppState {
    fn new(store: &TaskStore) -> Self {
        Self {
            tasks: Vec::new(),
            filtered: Vec::new(),
            selected: None,
            filter: String::new(),
            filter_active: false,
            status_filter: None,
            detail_cache: HashMap::new(),
            pending_details: HashSet::new(),
            status_message: None,
            watch_error: None,
            viewport: Viewport::default(),
            show_detail: false,
            cache: RenderCache::new(),
            config: store.config().clone(),
        }
    }

    fn update_viewport(&mut self, width: u16, height: u16) {
        let changed = self.viewport.width != width || self.viewport.height != height;
        self.viewport = Viewport { width, height };
        if changed {
            self.cache.invalidate_on_resize();
            if width >= NARROW_WIDTH {
                self.show_detail = true;
            }
        }
    }

    pub(crate) fn is_narrow(&self) -> bool {
        self.viewport.width > 0 && self.viewport.width < NARROW_WIDTH
    }

    pub(crate) fn selected_task(&self) -> Option<&TaskRecord> {
        self.selected.and_then(|idx| self.tasks.get(idx))
    }

    pub(crate) fn selected_details(&self) -> Option<&TaskDetails> {
        let task = self.selected_task()?;
        self.detail_cache.get(&task.id)
    }

    pub(crate) fn status_line(&self) -> Option<String> {
        if let Some(message) = self.status_message.as_ref() {
            return Some(message.clone());
        }
        if let Some(error) = self.watch_error.as_ref() {
            return Some(error.clone());
        }
        if !self.filter.is_empty() {
            return Some(format!("filter: {}", self.filter));
        }
        None
    }

    fn apply_filter(&mut self, previous_id: Option<String>) {
        self.filtered = model::filter_task_indices(
            &self.tasks,
            &self.filter,
            self.status_filter.as_deref(),
        );
        self.selected = model::select_by_id(
            &self.tasks,
            &self.filtered,
            previous_id.as_deref(),
        );
    }

    fn move_selection(&mut self, delta: isize, req_tx: &Sender<LoadRequest>) {
        if self.filtered.is_empty() {
            self.selected = None;
            return;
        }
        let current_pos = self
            .selected
            .and_then(|idx| self.filtered.iter().position(|candidate| *candidate == idx))
            .unwrap_or(0);
        let max = self.filtered.len().saturating_sub(1);
        let next = (current_pos as isize + delta).clamp(0, max as isize) as usize;
        self.selected = Some(self.filtered[next]);
        self.queue_detail_load(req_tx);
    }

    fn queue_detail_load(&mut self, req_tx: &Sender<LoadRequest>) {
        let Some(task) = self.selected_task() else {
            return;
        };
        if self.detail_cache.contains_key(&task.id) || self.pending_details.contains(&task.id) {
            return;
        }
        if req_tx.send(LoadRequest::Details(task.id.clone())).is_ok() {
            self.pending_details.insert(task.id.clone());
        }
    }

    fn set_status_filter(&mut self, status: Option<String>) {
        self.status_filter = status;
    }
}

pub fn run(store: TaskStore) -> Result<()> {
    let (ui_tx, ui_rx) = mpsc::channel();
    let (req_tx, req_rx) = mpsc::channel();

    spawn_loader(store.clone(), req_rx, ui_tx.clone());
    spawn_watch(store.clone(), req_tx.clone(), ui_tx);

    if req_tx.send(LoadRequest::Reload).is_err() {
        return Err(Error::OperationFailed(
            "failed to start task loader".to_string(),
        ));
    }

    let mut app = AppState::new(&store);
    run_terminal(&mut app, ui_rx, req_tx)
}

fn run_terminal(app: &mut AppState, ui_rx: Receiver<UiMsg>, req_tx: Sender<LoadRequest>) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let size = terminal.size()?;
    app.update_viewport(size.width, size.height);

    let result = run_loop(&mut terminal, app, ui_rx, req_tx);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut AppState,
    ui_rx: Receiver<UiMsg>,
    req_tx: Sender<LoadRequest>,
) -> Result<()> {
    let mut dirty = true;
    loop {
        while let Ok(msg) = ui_rx.try_recv() {
            handle_ui_msg(app, msg, &req_tx);
            dirty = true;
        }

        if dirty {
            terminal.draw(|frame| {
                app.update_viewport(frame.size().width, frame.size().height);
                view::render(frame, app);
            })?;
            dirty = false;
        }

        if event::poll(Duration::from_millis(EVENT_POLL_MS))? {
            match event::read()? {
                Event::Key(key) => {
                    if handle_key(app, key, &req_tx) {
                        break;
                    }
                    dirty = true;
                }
                Event::Resize(width, height) => {
                    app.update_viewport(width, height);
                    dirty = true;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn handle_ui_msg(app: &mut AppState, msg: UiMsg, req_tx: &Sender<LoadRequest>) {
    match msg {
        UiMsg::DataLoaded(mut tasks) => {
            model::sort_tasks(&mut tasks, &app.config);
            let previous_id = app.selected_task().map(|task| task.id.clone());
            app.tasks = tasks;
            app.detail_cache.clear();
            app.pending_details.clear();
            app.cache.invalidate_on_resize();
            app.status_message = None;
            app.apply_filter(previous_id);
            app.queue_detail_load(req_tx);
        }
        UiMsg::LoadError(err) => {
            app.status_message = Some(format!("load error: {err}"));
        }
        UiMsg::DetailsLoaded(id, details) => {
            app.pending_details.remove(&id);
            app.cache.invalidate_task(&id);
            app.detail_cache.insert(id, details);
        }
        UiMsg::DetailsError(id, err) => {
            app.pending_details.remove(&id);
            app.status_message = Some(format!("detail error: {err}"));
        }
        UiMsg::WatchError(err) => {
            app.watch_error = Some(format!("watch error: {err}"));
        }
    }
}

fn handle_key(app: &mut AppState, key: KeyEvent, req_tx: &Sender<LoadRequest>) -> bool {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return true;
    }

    if app.filter_active {
        match key.code {
            KeyCode::Esc => {
                app.filter.clear();
                app.filter_active = false;
            }
            KeyCode::Enter => app.filter_active = false,
            KeyCode::Backspace => {
                app.filter.pop();
            }
            KeyCode::Char(ch) => {
                if !ch.is_control() {
                    app.filter.push(ch);
                }
            }
            _ => {}
        }
        let previous = app.selected_task().map(|task| task.id.clone());
        app.apply_filter(previous);
        app.queue_detail_load(req_tx);
        return false;
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => true,
        KeyCode::Char('j') | KeyCode::Down => {
            app.move_selection(1, req_tx);
            false
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.move_selection(-1, req_tx);
            false
        }
        KeyCode::Char('/') => {
            app.filter_active = true;
            false
        }
        KeyCode::Char('r') => {
            let _ = req_tx.send(LoadRequest::Reload);
            false
        }
        KeyCode::Char('o') => {
            app.set_status_filter(Some("open".to_string()));
            let previous = app.selected_task().map(|task| task.id.clone());
            app.apply_filter(previous);
            app.queue_detail_load(req_tx);
            false
        }
        KeyCode::Char('p') => {
            app.set_status_filter(Some("in_progress".to_string()));
            let previous = app.selected_task().map(|task| task.id.clone());
            app.apply_filter(previous);
            app.queue_detail_load(req_tx);
            false
        }
        KeyCode::Char('c') => {
            app.set_status_filter(Some("closed".to_string()));
            let previous = app.selected_task().map(|task| task.id.clone());
            app.apply_filter(previous);
            app.queue_detail_load(req_tx);
            false
        }
        KeyCode::Char('a') => {
            app.set_status_filter(None);
            let previous = app.selected_task().map(|task| task.id.clone());
            app.apply_filter(previous);
            app.queue_detail_load(req_tx);
            false
        }
        KeyCode::Enter => {
            if app.is_narrow() {
                app.show_detail = !app.show_detail;
            }
            false
        }
        _ => false,
    }
}

fn spawn_loader(store: TaskStore, req_rx: Receiver<LoadRequest>, ui_tx: Sender<UiMsg>) {
    thread::spawn(move || {
        while let Ok(req) = req_rx.recv() {
            match req {
                LoadRequest::Reload => match store.list(None) {
                    Ok(tasks) => {
                        let _ = ui_tx.send(UiMsg::DataLoaded(tasks));
                    }
                    Err(err) => {
                        let _ = ui_tx.send(UiMsg::LoadError(err.to_string()));
                    }
                },
                LoadRequest::Details(id) => match store.details(&id) {
                    Ok(details) => {
                        let _ = ui_tx.send(UiMsg::DetailsLoaded(id, details));
                    }
                    Err(err) => {
                        let _ = ui_tx.send(UiMsg::DetailsError(id, err.to_string()));
                    }
                },
            }
        }
    });
}

fn spawn_watch(store: TaskStore, req_tx: Sender<LoadRequest>, ui_tx: Sender<UiMsg>) {
    let tasks_dir = store.tasks_dir();
    let shared_dir = store.storage().shared_dir();

    if !tasks_dir.exists() && !shared_dir.exists() {
        return;
    }

    thread::spawn(move || {
        let (event_tx, event_rx) = mpsc::channel();
        let watcher: notify::Result<RecommendedWatcher> =
            notify::recommended_watcher(move |res| {
                let _ = event_tx.send(res);
            });

        let mut watcher = match watcher {
            Ok(watcher) => watcher,
            Err(err) => {
                let _ = ui_tx.send(UiMsg::WatchError(err.to_string()));
                return;
            }
        };

        if tasks_dir.exists() {
            let _ = watcher.watch(&tasks_dir, RecursiveMode::NonRecursive);
        }
        if shared_dir.exists() {
            let _ = watcher.watch(&shared_dir, RecursiveMode::NonRecursive);
        }

        let debounce = Duration::from_millis(WATCH_DEBOUNCE_MS);
        let mut pending: Option<Instant> = None;

        loop {
            let timeout = pending
                .map(|deadline| deadline.saturating_duration_since(Instant::now()))
                .unwrap_or(Duration::from_secs(3600));
            match event_rx.recv_timeout(timeout) {
                Ok(Ok(_)) => {
                    pending = Some(Instant::now() + debounce);
                }
                Ok(Err(err)) => {
                    let _ = ui_tx.send(UiMsg::WatchError(err.to_string()));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if pending.is_some() {
                        pending = None;
                        if req_tx.send(LoadRequest::Reload).is_err() {
                            break;
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });
}
