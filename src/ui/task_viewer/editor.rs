use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::task::TaskRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorKind {
    NewTask,
    EditTask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorFieldId {
    Title,
    Body,
    Priority,
    Parent,
    Children,
}

#[derive(Debug, Clone)]
pub struct EditorField {
    pub id: EditorFieldId,
    pub label: &'static str,
    pub value: String,
    pub required: bool,
}

#[derive(Debug, Clone)]
pub struct EditorSubmit {
    pub title: String,
    pub priority: Option<String>,
    pub parent: Option<String>,
    pub children: Vec<String>,
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    None,
    Cancel,
    Submit,
    OpenPriorityPicker,
    OpenParentPicker,
    OpenChildrenPicker,
    OpenBodyEditor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    Normal,
    Insert,
}

#[derive(Debug, Clone)]
pub struct EditorState {
    kind: EditorKind,
    fields: Vec<EditorField>,
    active: usize,
    confirming: bool,
    mode: EditorMode,
    error: Option<String>,
    default_priority: Option<String>,
    task_id: Option<String>,
}

impl EditorState {
    pub fn new_task(default_priority: String) -> Self {
        Self {
            kind: EditorKind::NewTask,
            fields: vec![
                EditorField {
                    id: EditorFieldId::Title,
                    label: "Title",
                    value: String::new(),
                    required: true,
                },
                EditorField {
                    id: EditorFieldId::Body,
                    label: "Body",
                    value: String::new(),
                    required: false,
                },
                EditorField {
                    id: EditorFieldId::Priority,
                    label: "Priority",
                    value: String::new(),
                    required: false,
                },
                EditorField {
                    id: EditorFieldId::Parent,
                    label: "Parent",
                    value: String::new(),
                    required: false,
                },
                EditorField {
                    id: EditorFieldId::Children,
                    label: "Children",
                    value: String::new(),
                    required: false,
                },
            ],
            active: 0,
            confirming: false,
            mode: EditorMode::Normal,
            error: None,
            default_priority: Some(default_priority),
            task_id: None,
        }
    }

    pub fn edit_task(task: &TaskRecord, parent: Option<String>) -> Self {
        Self {
            kind: EditorKind::EditTask,
            fields: vec![
                EditorField {
                    id: EditorFieldId::Title,
                    label: "Title",
                    value: task.title.clone(),
                    required: true,
                },
                EditorField {
                    id: EditorFieldId::Body,
                    label: "Body",
                    value: task.body.clone().unwrap_or_default(),
                    required: false,
                },
                EditorField {
                    id: EditorFieldId::Priority,
                    label: "Priority",
                    value: task.priority.clone(),
                    required: false,
                },
                EditorField {
                    id: EditorFieldId::Parent,
                    label: "Parent",
                    value: parent.unwrap_or_default(),
                    required: false,
                },
                EditorField {
                    id: EditorFieldId::Children,
                    label: "Children",
                    value: String::new(),
                    required: false,
                },
            ],
            active: 0,
            confirming: false,
            mode: EditorMode::Normal,
            error: None,
            default_priority: None,
            task_id: Some(task.id.clone()),
        }
    }

    pub fn kind(&self) -> EditorKind {
        self.kind
    }

    pub fn task_id(&self) -> Option<&str> {
        self.task_id.as_deref()
    }

    pub fn fields(&self) -> &[EditorField] {
        &self.fields
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active_field_id(&self) -> Option<EditorFieldId> {
        self.fields.get(self.active).map(|field| field.id)
    }

    pub fn confirming(&self) -> bool {
        self.confirming
    }

    pub fn mode(&self) -> EditorMode {
        self.mode
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn default_priority(&self) -> Option<&str> {
        self.default_priority.as_deref()
    }

    pub fn field_value(&self, id: EditorFieldId) -> &str {
        self.field_value_inner(id)
    }

    pub fn set_field_value(&mut self, id: EditorFieldId, value: String) {
        if let Some(field) = self.fields.iter_mut().find(|field| field.id == id) {
            field.value = value;
        }
    }

    pub fn set_error(&mut self, message: String) {
        self.error = Some(message);
        self.confirming = false;
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> EditorAction {
        if self.confirming {
            return self.handle_confirm_key(key);
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Enter {
            return self.attempt_confirm();
        }
        let action = match self.mode {
            EditorMode::Normal => self.handle_normal_key(key),
            EditorMode::Insert => self.handle_insert_key(key),
        };
        if matches!(
            action,
            EditorAction::None
                | EditorAction::OpenPriorityPicker
                | EditorAction::OpenParentPicker
                | EditorAction::OpenChildrenPicker
                | EditorAction::OpenBodyEditor
        ) {
            self.error = None;
        }
        action
    }

    pub fn build_submit(&self) -> Result<EditorSubmit, String> {
        self.validate()?;
        let title = self.field_value(EditorFieldId::Title).trim().to_string();
        let priority = non_empty(self.field_value(EditorFieldId::Priority));
        let parent = non_empty(self.field_value(EditorFieldId::Parent));
        let children = parse_task_list(self.field_value(EditorFieldId::Children));
        let body = self.field_value(EditorFieldId::Body).to_string();

        Ok(EditorSubmit {
            title,
            priority,
            parent,
            children,
            body,
        })
    }

    fn attempt_confirm(&mut self) -> EditorAction {
        match self.validate() {
            Ok(()) => {
                self.confirming = true;
                EditorAction::None
            }
            Err(err) => {
                self.error = Some(err);
                self.confirming = false;
                EditorAction::None
            }
        }
    }

    fn handle_confirm_key(&mut self, key: KeyEvent) -> EditorAction {
        match key.code {
            KeyCode::Esc => EditorAction::Cancel,
            KeyCode::Backspace => {
                self.confirming = false;
                self.error = None;
                EditorAction::None
            }
            KeyCode::Char('e') => {
                self.confirming = false;
                self.error = None;
                EditorAction::None
            }
            KeyCode::Char('y') | KeyCode::Enter => EditorAction::Submit,
            _ => EditorAction::None,
        }
    }

    fn validate(&self) -> Result<(), String> {
        let title = self.field_value(EditorFieldId::Title).trim();
        if title.is_empty() {
            return Err("title is required".to_string());
        }
        if let Some(priority) = non_empty(self.field_value(EditorFieldId::Priority)) {
            if !is_valid_priority(&priority) {
                return Err("priority must be P0-P4".to_string());
            }
        }
        Ok(())
    }

    fn move_active(&mut self, delta: isize) {
        let len = self.fields.len() as isize;
        if len == 0 {
            self.active = 0;
            return;
        }
        let next = (self.active as isize + delta).rem_euclid(len);
        self.active = next as usize;
    }

    fn current_field_mut(&mut self) -> Option<&mut EditorField> {
        self.fields.get_mut(self.active)
    }

    fn current_field_id(&self) -> Option<EditorFieldId> {
        self.fields.get(self.active).map(|field| field.id)
    }

    fn field_value_inner(&self, id: EditorFieldId) -> &str {
        self.fields
            .iter()
            .find(|field| field.id == id)
            .map(|field| field.value.as_str())
            .unwrap_or("")
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> EditorAction {
        match key.code {
            KeyCode::Esc => return EditorAction::Cancel,
            KeyCode::Char('c') => return self.attempt_confirm(),
            KeyCode::Tab | KeyCode::Down | KeyCode::Char('j') => {
                self.move_active(1);
            }
            KeyCode::BackTab | KeyCode::Up | KeyCode::Char('k') => {
                self.move_active(-1);
            }
            KeyCode::Enter => match self.current_field_id() {
                Some(EditorFieldId::Body) => return EditorAction::OpenBodyEditor,
                Some(EditorFieldId::Priority) => return EditorAction::OpenPriorityPicker,
                Some(EditorFieldId::Parent) => return EditorAction::OpenParentPicker,
                Some(EditorFieldId::Children) => return EditorAction::OpenChildrenPicker,
                _ => {
                    self.mode = EditorMode::Insert;
                }
            },
            _ => {}
        }
        EditorAction::None
    }

    fn handle_insert_key(&mut self, key: KeyEvent) -> EditorAction {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('u') {
            if let Some(field) = self.current_field_mut() {
                field.value.clear();
            }
            return EditorAction::None;
        }

        let is_body = matches!(self.current_field_id(), Some(EditorFieldId::Body));
        match key.code {
            KeyCode::Esc => return EditorAction::Cancel,
            KeyCode::Enter => {
                if is_body {
                    if let Some(field) = self.current_field_mut() {
                        field.value.push('\n');
                    }
                    return EditorAction::None;
                }
                return self.finish_field();
            }
            KeyCode::Tab => return self.finish_field(),
            KeyCode::Backspace => {
                if let Some(field) = self.current_field_mut() {
                    field.value.pop();
                }
            }
            KeyCode::Char(ch) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    return EditorAction::None;
                }
                if !ch.is_control() {
                    if let Some(field) = self.current_field_mut() {
                        field.value.push(ch);
                    }
                }
            }
            _ => {}
        }
        EditorAction::None
    }

    fn finish_field(&mut self) -> EditorAction {
        self.mode = EditorMode::Normal;
        if self.active + 1 >= self.fields.len() {
            return self.attempt_confirm();
        }
        self.move_active(1);
        EditorAction::None
    }
}

#[derive(Debug, Clone)]
pub struct PriorityPicker {
    options: Vec<String>,
    selected: usize,
    original: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriorityAction {
    None,
    Cancel,
    Confirm,
}

#[derive(Debug, Clone)]
pub struct StatusPicker {
    options: Vec<String>,
    selected: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusPickerAction {
    None,
    Cancel,
    Confirm,
}

#[derive(Debug, Clone)]
pub struct TaskOption {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct TaskPicker {
    options: Vec<TaskOption>,
    filtered: Vec<usize>,
    selected: usize,
    query: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskPickerAction {
    None,
    Cancel,
    Confirm,
}

#[derive(Debug, Clone)]
pub struct MultiTaskPicker {
    options: Vec<TaskOption>,
    filtered: Vec<usize>,
    selected: usize,
    query: String,
    selected_indices: std::collections::HashSet<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiTaskPickerAction {
    None,
    Cancel,
    Confirm,
}

impl PriorityPicker {
    pub fn new(current: &str) -> Self {
        let options = vec![
            "P0".to_string(),
            "P1".to_string(),
            "P2".to_string(),
            "P3".to_string(),
            "P4".to_string(),
        ];
        let normalized = current.trim().to_ascii_uppercase();
        let selected = options
            .iter()
            .position(|value| value.eq_ignore_ascii_case(&normalized))
            .unwrap_or(2);
        Self {
            options,
            selected,
            original: normalized,
        }
    }

    pub fn options(&self) -> &[String] {
        &self.options
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected_priority(&self) -> &str {
        self.options
            .get(self.selected)
            .map(|value| value.as_str())
            .unwrap_or("P2")
    }

    pub fn original_priority(&self) -> &str {
        &self.original
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> PriorityAction {
        match key.code {
            KeyCode::Esc => return PriorityAction::Cancel,
            KeyCode::Enter => return PriorityAction::Confirm,
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Char(ch) if ch.is_ascii_digit() => {
                if let Some(idx) = ch.to_digit(10).and_then(|value| value.checked_sub(0)) {
                    let idx = idx as usize;
                    if idx < self.options.len() {
                        self.selected = idx;
                    }
                }
            }
            _ => {}
        }
        PriorityAction::None
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.options.len() as isize;
        if len == 0 {
            self.selected = 0;
            return;
        }
        let next = (self.selected as isize + delta).rem_euclid(len);
        self.selected = next as usize;
    }
}

impl StatusPicker {
    pub fn new(options: Vec<String>, current: Option<&str>) -> Self {
        let selected = current
            .and_then(|value| {
                options
                    .iter()
                    .position(|option| option.eq_ignore_ascii_case(value))
            })
            .unwrap_or(0);
        Self { options, selected }
    }

    pub fn options(&self) -> &[String] {
        &self.options
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected_status(&self) -> &str {
        self.options
            .get(self.selected)
            .map(|value| value.as_str())
            .unwrap_or("")
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> StatusPickerAction {
        match key.code {
            KeyCode::Esc => return StatusPickerAction::Cancel,
            KeyCode::Enter => return StatusPickerAction::Confirm,
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            _ => {}
        }
        StatusPickerAction::None
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.options.len() as isize;
        if len == 0 {
            self.selected = 0;
            return;
        }
        let next = (self.selected as isize + delta).rem_euclid(len);
        self.selected = next as usize;
    }
}

impl TaskPicker {
    pub fn new(options: Vec<TaskOption>) -> Self {
        let filtered: Vec<usize> = (0..options.len()).collect();
        Self {
            options,
            filtered,
            selected: 0,
            query: String::new(),
        }
    }

    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.rebuild_filter();
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn filtered_indices(&self) -> &[usize] {
        &self.filtered
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected_option(&self) -> Option<&TaskOption> {
        self.filtered
            .get(self.selected)
            .and_then(|idx| self.options.get(*idx))
    }

    pub fn options(&self) -> &[TaskOption] {
        &self.options
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> TaskPickerAction {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('u') {
            self.query.clear();
            self.rebuild_filter();
            return TaskPickerAction::None;
        }

        match key.code {
            KeyCode::Esc => return TaskPickerAction::Cancel,
            KeyCode::Enter => return TaskPickerAction::Confirm,
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Backspace => {
                self.query.pop();
                self.rebuild_filter();
            }
            KeyCode::Char(ch) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    return TaskPickerAction::None;
                }
                if !ch.is_control() {
                    self.query.push(ch);
                    self.rebuild_filter();
                }
            }
            _ => {}
        }
        TaskPickerAction::None
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.filtered.len() as isize;
        if len == 0 {
            self.selected = 0;
            return;
        }
        let next = (self.selected as isize + delta).rem_euclid(len);
        self.selected = next as usize;
    }

    fn rebuild_filter(&mut self) {
        let query = self.query.trim().to_ascii_lowercase();
        if query.is_empty() {
            self.filtered = (0..self.options.len()).collect();
            self.selected = 0;
            return;
        }
        self.filtered = self
            .options
            .iter()
            .enumerate()
            .filter_map(|(idx, option)| {
                let id = option.id.to_ascii_lowercase();
                let title = option.title.to_ascii_lowercase();
                if fuzzy_match(&id, &query) || fuzzy_match(&title, &query) {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect();
        self.selected = 0;
    }
}

impl MultiTaskPicker {
    pub fn new(options: Vec<TaskOption>, selected_ids: &[String]) -> Self {
        let mut selected_indices = std::collections::HashSet::new();
        if !selected_ids.is_empty() {
            for (idx, option) in options.iter().enumerate() {
                if selected_ids.iter().any(|id| id == &option.id) {
                    selected_indices.insert(idx);
                }
            }
        }
        let filtered = (0..options.len()).collect();
        Self {
            options,
            filtered,
            selected: 0,
            query: String::new(),
            selected_indices,
        }
    }

    pub fn options(&self) -> &[TaskOption] {
        &self.options
    }

    pub fn filtered_indices(&self) -> &[usize] {
        &self.filtered
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn is_selected(&self, option_index: usize) -> bool {
        self.selected_indices.contains(&option_index)
    }

    pub fn selected_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .options
            .iter()
            .enumerate()
            .filter_map(|(idx, option)| {
                if self.selected_indices.contains(&idx) {
                    Some(option.id.clone())
                } else {
                    None
                }
            })
            .collect();
        ids.sort();
        ids
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> MultiTaskPickerAction {
        match key.code {
            KeyCode::Esc => return MultiTaskPickerAction::Cancel,
            KeyCode::Enter => return MultiTaskPickerAction::Confirm,
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Backspace => {
                self.query.pop();
                self.rebuild_filter();
            }
            KeyCode::Char(' ') => {
                self.toggle_selected();
            }
            KeyCode::Char(ch) => {
                if !ch.is_control() {
                    self.query.push(ch);
                    self.rebuild_filter();
                }
            }
            _ => {}
        }
        MultiTaskPickerAction::None
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.filtered.len() as isize;
        if len == 0 {
            self.selected = 0;
            return;
        }
        let next = (self.selected as isize + delta).rem_euclid(len);
        self.selected = next as usize;
    }

    fn toggle_selected(&mut self) {
        if let Some(option_idx) = self.filtered.get(self.selected).copied() {
            if self.selected_indices.contains(&option_idx) {
                self.selected_indices.remove(&option_idx);
            } else {
                self.selected_indices.insert(option_idx);
            }
        }
    }

    fn rebuild_filter(&mut self) {
        let query = self.query.trim().to_ascii_lowercase();
        if query.is_empty() {
            self.filtered = (0..self.options.len()).collect();
            self.selected = 0;
            return;
        }
        self.filtered = self
            .options
            .iter()
            .enumerate()
            .filter_map(|(idx, option)| {
                let id = option.id.to_ascii_lowercase();
                let title = option.title.to_ascii_lowercase();
                if fuzzy_match(&id, &query) || fuzzy_match(&title, &query) {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect();
        self.selected = 0;
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_task_list(value: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
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

fn fuzzy_match(value: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let mut query_chars = query.chars();
    let mut current = query_chars.next();
    for ch in value.chars() {
        if Some(ch) == current {
            current = query_chars.next();
            if current.is_none() {
                return true;
            }
        }
    }
    false
}

fn is_valid_priority(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_uppercase().as_str(),
        "P0" | "P1" | "P2" | "P3" | "P4"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_requires_title() {
        let editor = EditorState::new_task("P2".to_string());
        let err = editor.build_submit().expect_err("should require title");
        assert_eq!(err, "title is required");
    }

    #[test]
    fn priority_picker_selects_current() {
        let picker = PriorityPicker::new("p3");
        assert_eq!(picker.selected_priority(), "P3");
    }

    #[test]
    fn ctrl_enter_confirms_from_editor() {
        let mut editor = EditorState::new_task("P2".to_string());
        editor.set_field_value(EditorFieldId::Title, "Ship it".to_string());
        let action = editor.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
        assert_eq!(action, EditorAction::None);
        assert!(editor.confirming());
    }

    #[test]
    fn enter_on_body_opens_external_editor() {
        let mut editor = EditorState::new_task("P2".to_string());
        editor.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::empty()));
        let action = editor.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()));
        assert_eq!(action, EditorAction::OpenBodyEditor);
        assert_eq!(editor.mode(), EditorMode::Normal);
    }

    #[test]
    fn c_confirms_from_editor() {
        let mut editor = EditorState::new_task("P2".to_string());
        editor.set_field_value(EditorFieldId::Title, "Ship it".to_string());
        let action = editor.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::empty()));
        assert_eq!(action, EditorAction::None);
        assert!(editor.confirming());
    }

    #[test]
    fn multi_task_picker_toggles_selection() {
        let options = vec![
            TaskOption {
                id: "sv-1".to_string(),
                title: "One".to_string(),
            },
            TaskOption {
                id: "sv-2".to_string(),
                title: "Two".to_string(),
            },
        ];
        let mut picker = MultiTaskPicker::new(options, &[]);
        assert!(picker.selected_ids().is_empty());
        picker.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()));
        assert_eq!(picker.selected_ids(), vec!["sv-1".to_string()]);
        picker.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()));
        assert!(picker.selected_ids().is_empty());
    }

    #[test]
    fn multi_task_picker_seeds_selected_ids() {
        let options = vec![
            TaskOption {
                id: "sv-1".to_string(),
                title: "One".to_string(),
            },
            TaskOption {
                id: "sv-2".to_string(),
                title: "Two".to_string(),
            },
        ];
        let picker = MultiTaskPicker::new(options, &["sv-2".to_string()]);
        assert_eq!(picker.selected_ids(), vec!["sv-2".to_string()]);
    }
}
