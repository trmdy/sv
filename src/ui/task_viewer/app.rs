use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::actor;
use crate::error::{Error, Result};
use crate::task::{TaskDetails, TaskRecord, TaskStore};

use super::actions::{self, ActionOutcome, EditTaskInput, NewTaskInput};
use super::cache::RenderCache;
use super::editor::{
    EditorAction, EditorFieldId, EditorKind, EditorMode, EditorState, MultiTaskPicker,
    MultiTaskPickerAction, PriorityAction, PriorityPicker, StatusPicker, StatusPickerAction,
    TaskOption, TaskPicker, TaskPickerAction,
};
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
    DataLoaded(
        Vec<TaskRecord>,
        HashSet<String>,
        Option<String>,
        HashMap<String, String>,
    ),
    LoadError(String),
    DetailsLoaded(String, TaskDetails),
    DetailsError(String, String),
    WatchError(String),
}

#[derive(Clone, Copy)]
pub(crate) enum StatusKind {
    Error,
    Info,
}

#[derive(Clone, Copy)]
pub(crate) enum StatusPickerMode {
    Filter,
    Change,
}

pub(crate) struct StatusPickerState {
    pub(crate) picker: StatusPicker,
    pub(crate) mode: StatusPickerMode,
}

#[derive(Default, Clone, Copy)]
struct Viewport {
    width: u16,
    height: u16,
}

pub struct AppState {
    pub(crate) tasks: Vec<TaskRecord>,
    pub(crate) task_depths: Vec<usize>,
    pub(crate) filtered: Vec<usize>,
    pub(crate) selected: Option<usize>,
    pub(crate) filter: String,
    pub(crate) filter_active: bool,
    pub(crate) status_filter: Option<String>,
    pub(crate) blocked_ids: HashSet<String>,
    pub(crate) editor: Option<EditorState>,
    pub(crate) priority_picker: Option<PriorityPicker>,
    pub(crate) editor_priority_picker: Option<PriorityPicker>,
    pub(crate) parent_picker: Option<TaskPicker>,
    pub(crate) children_picker: Option<MultiTaskPicker>,
    pub(crate) status_picker: Option<StatusPickerState>,
    pub(crate) info_message: Option<String>,
    detail_cache: HashMap<String, TaskDetails>,
    pending_details: HashSet<String>,
    status_message: Option<String>,
    watch_error: Option<String>,
    viewport: Viewport,
    pub(crate) show_detail: bool,
    pub(crate) cache: RenderCache,
    config: crate::config::TasksConfig,
    store: TaskStore,
    actor: Option<String>,
}

impl AppState {
    fn new(store: TaskStore, actor: Option<String>) -> Self {
        Self {
            tasks: Vec::new(),
            task_depths: Vec::new(),
            filtered: Vec::new(),
            selected: None,
            filter: String::new(),
            filter_active: false,
            status_filter: None,
            blocked_ids: HashSet::new(),
            editor: None,
            priority_picker: None,
            editor_priority_picker: None,
            parent_picker: None,
            children_picker: None,
            status_picker: None,
            info_message: None,
            detail_cache: HashMap::new(),
            pending_details: HashSet::new(),
            status_message: None,
            watch_error: None,
            viewport: Viewport::default(),
            show_detail: false,
            cache: RenderCache::new(),
            config: store.config().clone(),
            store,
            actor,
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

    pub(crate) fn status_line(&self) -> Option<(String, StatusKind)> {
        if let Some(message) = self.status_message.as_ref() {
            return Some((message.clone(), StatusKind::Error));
        }
        if let Some(error) = self.watch_error.as_ref() {
            return Some((error.clone(), StatusKind::Error));
        }
        if let Some(info) = self.info_message.as_ref() {
            return Some((info.clone(), StatusKind::Info));
        }
        if !self.filter.is_empty() {
            return Some((format!("filter: {}", self.filter), StatusKind::Info));
        }
        None
    }

    pub(crate) fn footer_hint(&self) -> String {
        if self.status_picker.is_some() {
            return "j/k move  enter apply  esc cancel".to_string();
        }
        if self.parent_picker.is_some() {
            return "type to filter  j/k move  enter apply  esc cancel".to_string();
        }
        if self.children_picker.is_some() {
            return "type to filter  j/k move  space toggle  enter apply  esc cancel".to_string();
        }
        if self.editor_priority_picker.is_some() {
            return "j/k move  enter apply  esc cancel".to_string();
        }
        if let Some(editor) = self.editor.as_ref() {
            if editor.confirming() {
                return "y confirm  backspace edit  esc cancel".to_string();
            }
            let body_active = matches!(editor.active_field_id(), Some(EditorFieldId::Body));
            return match editor.mode() {
                EditorMode::Normal => {
                    "enter edit  tab next  shift+tab prev  ctrl+enter confirm  esc cancel"
                        .to_string()
                }
                EditorMode::Insert => {
                    if body_active {
                        "enter newline  tab next  ctrl+u clear  ctrl+enter confirm  esc cancel"
                            .to_string()
                    } else {
                        "enter/tab next  ctrl+u clear  ctrl+enter confirm  esc cancel".to_string()
                    }
                }
            };
        }
        if self.priority_picker.is_some() {
            return "j/k move  enter apply  esc cancel".to_string();
        }
        if self.filter_active {
            return "type filter  backspace delete  tab status  enter done  esc clear".to_string();
        }
        "j/k move  n new  e edit  p priority  s status  / filter  r reload  q quit".to_string()
    }

    pub(crate) fn task_count_summary(&self) -> String {
        let open_status = self.config.default_status.as_str();
        let closed_statuses = &self.config.closed_statuses;
        let mut open = 0usize;
        let mut ready = 0usize;
        let mut closed = 0usize;
        for task in &self.tasks {
            if task.status == open_status {
                open += 1;
                if !self.blocked_ids.contains(&task.id) {
                    ready += 1;
                }
            }
            if closed_statuses.iter().any(|status| status == &task.status) {
                closed += 1;
            }
        }
        format!("open: {open}  ready: {ready}  closed: {closed}")
    }

    pub(crate) fn is_task_ready(&self, task: &TaskRecord) -> bool {
        task.status == self.config.default_status && !self.blocked_ids.contains(&task.id)
    }

    fn task_picker_options(&self, exclude_id: Option<&str>) -> Vec<TaskOption> {
        let mut options: Vec<TaskOption> = self
            .tasks
            .iter()
            .filter(|task| Some(task.id.as_str()) != exclude_id)
            .map(|task| TaskOption {
                id: task.id.clone(),
                title: task.title.clone(),
            })
            .collect();
        options.sort_by(|left, right| left.id.cmp(&right.id));
        options
    }

    fn status_options(&self, include_all: bool) -> Vec<String> {
        let mut options = Vec::new();
        if include_all {
            options.push("all".to_string());
        }
        options.extend(self.config.statuses.iter().cloned());
        options
    }

    fn apply_filter(&mut self, previous_id: Option<String>) {
        self.filtered =
            model::filter_task_indices(&self.tasks, &self.filter, self.status_filter.as_deref());
        self.selected = model::select_by_id(&self.tasks, &self.filtered, previous_id.as_deref());
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

    fn set_error(&mut self, message: String) {
        self.status_message = Some(message);
        self.info_message = None;
    }

    fn set_info(&mut self, message: String) {
        self.info_message = Some(message);
        self.status_message = None;
    }

    fn apply_outcome(&mut self, outcome: ActionOutcome, req_tx: &Sender<LoadRequest>) {
        if outcome.changed {
            let _ = req_tx.send(LoadRequest::Reload);
        }
        self.set_info(outcome.message);
    }

    fn list_jump(&self) -> isize {
        let mut height = self.viewport.height.saturating_sub(4);
        if self.filter_active || !self.filter.is_empty() || self.status_filter.is_some() {
            height = height.saturating_sub(2);
        }
        let jump = (height / 2).max(1);
        jump as isize
    }
}

pub fn run(store: TaskStore) -> Result<()> {
    let actor = actor::resolve_actor_optional(Some(store.storage().workspace_root()), None)?;
    let (ui_tx, ui_rx) = mpsc::channel();
    let (req_tx, req_rx) = mpsc::channel();

    spawn_loader(store.clone(), req_rx, ui_tx.clone());
    spawn_watch(store.clone(), req_tx.clone(), ui_tx);

    if req_tx.send(LoadRequest::Reload).is_err() {
        return Err(Error::OperationFailed(
            "failed to start task loader".to_string(),
        ));
    }

    let mut app = AppState::new(store, actor);
    run_terminal(&mut app, ui_rx, req_tx)
}

fn run_terminal(
    app: &mut AppState,
    ui_rx: Receiver<UiMsg>,
    req_tx: Sender<LoadRequest>,
) -> Result<()> {
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
        UiMsg::DataLoaded(mut tasks, blocked_ids, blocked_error, parent_by_child) => {
            model::sort_tasks(&mut tasks, &app.config, &blocked_ids);
            let (tasks, depths) = model::nest_tasks(tasks, &parent_by_child);
            let previous_id = app.selected_task().map(|task| task.id.clone());
            app.tasks = tasks;
            app.task_depths = depths;
            app.blocked_ids = blocked_ids;
            app.detail_cache.clear();
            app.pending_details.clear();
            app.cache.invalidate_on_resize();
            app.status_message = blocked_error;
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

    if let Some(mut state) = app.status_picker.take() {
        let action = state.picker.handle_key(key);
        match action {
            StatusPickerAction::None => {
                app.status_picker = Some(state);
            }
            StatusPickerAction::Cancel => {
                app.status_picker = None;
            }
            StatusPickerAction::Confirm => {
                let selected = state.picker.selected_status().to_string();
                match state.mode {
                    StatusPickerMode::Filter => {
                        app.status_picker = None;
                        if selected.eq_ignore_ascii_case("all") {
                            app.set_status_filter(None);
                        } else {
                            app.set_status_filter(Some(selected));
                        }
                        let previous = app.selected_task().map(|task| task.id.clone());
                        app.apply_filter(previous);
                        app.queue_detail_load(req_tx);
                    }
                    StatusPickerMode::Change => {
                        let Some(task_id) = app.selected_task().map(|task| task.id.clone()) else {
                            app.set_error("no task selected".to_string());
                            return false;
                        };
                        app.status_picker = None;
                        match actions::change_status(
                            &app.store,
                            app.actor.clone(),
                            &task_id,
                            &selected,
                        ) {
                            Ok(outcome) => app.apply_outcome(outcome, req_tx),
                            Err(err) => app.set_error(err.to_string()),
                        }
                    }
                }
            }
        }
        return false;
    }

    if app.parent_picker.is_some() {
        let mut picker = app.parent_picker.take().unwrap();
        let action = picker.handle_key(key);
        match action {
            TaskPickerAction::None => {
                app.parent_picker = Some(picker);
            }
            TaskPickerAction::Cancel => {
                app.parent_picker = None;
            }
            TaskPickerAction::Confirm => {
                let selected = picker.selected_option().map(|option| option.id.clone());
                app.parent_picker = None;
                if let (Some(editor), Some(value)) = (app.editor.as_mut(), selected) {
                    editor.set_field_value(EditorFieldId::Parent, value);
                }
            }
        }
        return false;
    }

    if app.children_picker.is_some() {
        let mut picker = app.children_picker.take().unwrap();
        let action = picker.handle_key(key);
        match action {
            MultiTaskPickerAction::None => {
                app.children_picker = Some(picker);
            }
            MultiTaskPickerAction::Cancel => {
                app.children_picker = None;
            }
            MultiTaskPickerAction::Confirm => {
                let selected = picker.selected_ids();
                app.children_picker = None;
                if let Some(editor) = app.editor.as_mut() {
                    editor.set_field_value(EditorFieldId::Children, selected.join(", "));
                }
            }
        }
        return false;
    }

    if app.editor_priority_picker.is_some() {
        let mut picker = app.editor_priority_picker.take().unwrap();
        let action = picker.handle_key(key);
        match action {
            PriorityAction::None => {
                app.editor_priority_picker = Some(picker);
            }
            PriorityAction::Cancel => {
                app.editor_priority_picker = None;
            }
            PriorityAction::Confirm => {
                let selected = picker.selected_priority().to_string();
                app.editor_priority_picker = None;
                if let Some(editor) = app.editor.as_mut() {
                    editor.set_field_value(EditorFieldId::Priority, selected);
                }
            }
        }
        return false;
    }

    if app.editor.is_some() {
        let mut editor = app.editor.take().unwrap();
        let kind = editor.kind();
        let task_id = editor.task_id().map(|value| value.to_string());
        let action = editor.handle_key(key);
        match action {
            EditorAction::None => {
                app.editor = Some(editor);
            }
            EditorAction::Cancel => {
                app.editor = None;
                app.set_info("cancelled".to_string());
            }
            EditorAction::OpenPriorityPicker => {
                let current = editor.field_value(EditorFieldId::Priority);
                app.editor_priority_picker = Some(PriorityPicker::new(current));
                app.editor = Some(editor);
            }
            EditorAction::OpenParentPicker => {
                let exclude = editor.task_id();
                let mut picker = TaskPicker::new(app.task_picker_options(exclude));
                let current = editor.field_value(EditorFieldId::Parent).trim();
                if !current.is_empty() {
                    picker.set_query(current.to_string());
                }
                app.parent_picker = Some(picker);
                app.editor = Some(editor);
            }
            EditorAction::OpenChildrenPicker => {
                let exclude = editor.task_id();
                let selected_ids = parse_task_list(editor.field_value(EditorFieldId::Children));
                let picker = MultiTaskPicker::new(app.task_picker_options(exclude), &selected_ids);
                app.children_picker = Some(picker);
                app.editor = Some(editor);
            }
            EditorAction::Submit => match editor.build_submit() {
                Ok(submit) => {
                    let outcome = match kind {
                        EditorKind::NewTask => actions::create_task(
                            &app.store,
                            app.actor.clone(),
                            NewTaskInput {
                                title: submit.title,
                                priority: submit.priority,
                                parent: submit.parent,
                                children: submit.children,
                                body: submit.body,
                            },
                        ),
                        EditorKind::EditTask => {
                            if let Some(task_id) = task_id {
                                actions::edit_task(
                                    &app.store,
                                    app.actor.clone(),
                                    &task_id,
                                    EditTaskInput {
                                        title: submit.title,
                                        priority: submit.priority,
                                        parent: submit.parent,
                                        children: submit.children,
                                        body: submit.body,
                                    },
                                )
                            } else {
                                Err(Error::OperationFailed(
                                    "missing task id for edit".to_string(),
                                ))
                            }
                        }
                    };

                    match outcome {
                        Ok(outcome) => {
                            app.editor = None;
                            app.apply_outcome(outcome, req_tx);
                        }
                        Err(err) => {
                            editor.set_error(err.to_string());
                            app.editor = Some(editor);
                        }
                    }
                }
                Err(err) => {
                    editor.set_error(err);
                    app.editor = Some(editor);
                }
            },
        }
        return false;
    }

    if app.priority_picker.is_some() {
        let mut picker = app.priority_picker.take().unwrap();
        let action = picker.handle_key(key);
        match action {
            PriorityAction::None => {
                app.priority_picker = Some(picker);
            }
            PriorityAction::Cancel => {
                app.priority_picker = None;
            }
            PriorityAction::Confirm => {
                let Some(task_id) = app.selected_task().map(|task| task.id.clone()) else {
                    app.set_error("no task selected".to_string());
                    return false;
                };
                let selected = picker.selected_priority().to_string();
                app.priority_picker = None;
                match actions::change_priority(&app.store, app.actor.clone(), &task_id, &selected) {
                    Ok(outcome) => app.apply_outcome(outcome, req_tx),
                    Err(err) => app.set_error(err.to_string()),
                }
            }
        }
        return false;
    }

    if app.filter_active {
        match key.code {
            KeyCode::Esc => {
                app.filter.clear();
                app.filter_active = false;
            }
            KeyCode::Enter => app.filter_active = false,
            KeyCode::Tab => {
                let current = app
                    .status_filter
                    .clone()
                    .unwrap_or_else(|| "all".to_string());
                let picker = StatusPicker::new(app.status_options(true), Some(&current));
                app.status_picker = Some(StatusPickerState {
                    picker,
                    mode: StatusPickerMode::Filter,
                });
            }
            KeyCode::Backspace => {
                app.filter.pop();
            }
            KeyCode::Char(ch) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    return false;
                }
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
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.move_selection(app.list_jump(), req_tx);
            false
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.move_selection(-app.list_jump(), req_tx);
            false
        }
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
        KeyCode::Char('n') => {
            let default_priority = app.store.default_priority();
            app.editor = Some(EditorState::new_task(default_priority));
            if app.is_narrow() {
                app.show_detail = true;
            }
            false
        }
        KeyCode::Char('e') => {
            let Some(task) = app.selected_task() else {
                app.set_error("no task selected".to_string());
                return false;
            };
            let parent = match app.store.relations(&task.id) {
                Ok(relations) => relations.parent,
                Err(err) => {
                    app.set_error(err.to_string());
                    return false;
                }
            };
            app.editor = Some(EditorState::edit_task(task, parent));
            if app.is_narrow() {
                app.show_detail = true;
            }
            false
        }
        KeyCode::Char('p') => {
            let Some(task) = app.selected_task() else {
                app.set_error("no task selected".to_string());
                return false;
            };
            app.priority_picker = Some(PriorityPicker::new(&task.priority));
            false
        }
        KeyCode::Char('s') => {
            let Some(task) = app.selected_task() else {
                app.set_error("no task selected".to_string());
                return false;
            };
            let options = app.status_options(false);
            let picker = StatusPicker::new(options, Some(task.status.as_str()));
            app.status_picker = Some(StatusPickerState {
                picker,
                mode: StatusPickerMode::Change,
            });
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
                        let (blocked_ids, blocked_error, parent_by_child) =
                            match store.blocked_and_parents() {
                                Ok((blocked_ids, parent_by_child)) => {
                                    (blocked_ids, None, parent_by_child)
                                }
                                Err(err) => (
                                    HashSet::new(),
                                    Some(format!("ready calc error: {err}")),
                                    HashMap::new(),
                                ),
                            };
                        let _ = ui_tx.send(UiMsg::DataLoaded(
                            tasks,
                            blocked_ids,
                            blocked_error,
                            parent_by_child,
                        ));
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

fn parse_task_list(value: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for part in value
        .split(|ch: char| ch == ',' || ch.is_whitespace())
        .map(|item| item.trim())
    {
        if part.is_empty() {
            continue;
        }
        if seen.insert(part.to_string()) {
            out.push(part.to_string());
        }
    }
    out
}

fn spawn_watch(store: TaskStore, req_tx: Sender<LoadRequest>, ui_tx: Sender<UiMsg>) {
    let tasks_dir = store.tasks_dir();
    let shared_dir = store.storage().shared_dir();

    if !tasks_dir.exists() && !shared_dir.exists() {
        return;
    }

    thread::spawn(move || {
        let (event_tx, event_rx) = mpsc::channel();
        let watcher: notify::Result<RecommendedWatcher> = notify::recommended_watcher(move |res| {
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
