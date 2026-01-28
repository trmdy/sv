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
    Priority,
    Parent,
    RelatesId,
    RelatesDesc,
    Body,
}

#[derive(Debug, Clone)]
pub struct EditorField {
    pub id: EditorFieldId,
    pub label: &'static str,
    pub value: String,
    pub required: bool,
}

#[derive(Debug, Clone)]
pub struct RelateInput {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct EditorSubmit {
    pub title: String,
    pub priority: Option<String>,
    pub parent: Option<String>,
    pub relates: Option<RelateInput>,
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    None,
    Cancel,
    Submit,
}

#[derive(Debug, Clone)]
pub struct EditorState {
    kind: EditorKind,
    fields: Vec<EditorField>,
    active: usize,
    confirming: bool,
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
                    id: EditorFieldId::RelatesId,
                    label: "Relates",
                    value: String::new(),
                    required: false,
                },
                EditorField {
                    id: EditorFieldId::RelatesDesc,
                    label: "Relate note",
                    value: String::new(),
                    required: false,
                },
                EditorField {
                    id: EditorFieldId::Body,
                    label: "Description",
                    value: String::new(),
                    required: false,
                },
            ],
            active: 0,
            confirming: false,
            error: None,
            default_priority: Some(default_priority),
            task_id: None,
        }
    }

    pub fn edit_task(task: &TaskRecord) -> Self {
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
                    label: "Description",
                    value: task.body.clone().unwrap_or_default(),
                    required: false,
                },
            ],
            active: 0,
            confirming: false,
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

    pub fn confirming(&self) -> bool {
        self.confirming
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn default_priority(&self) -> Option<&str> {
        self.default_priority.as_deref()
    }

    pub fn set_error(&mut self, message: String) {
        self.error = Some(message);
        self.confirming = false;
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> EditorAction {
        if self.confirming {
            return self.handle_confirm_key(key);
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('u') {
            if let Some(field) = self.current_field_mut() {
                field.value.clear();
            }
            self.error = None;
            return EditorAction::None;
        }

        match key.code {
            KeyCode::Esc => return EditorAction::Cancel,
            KeyCode::Tab | KeyCode::Down | KeyCode::Char('j') => {
                self.move_active(1);
            }
            KeyCode::BackTab | KeyCode::Up | KeyCode::Char('k') => {
                self.move_active(-1);
            }
            KeyCode::Enter => {
                if self.active + 1 >= self.fields.len() {
                    return self.attempt_confirm();
                }
                self.move_active(1);
            }
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

        self.error = None;
        EditorAction::None
    }

    pub fn build_submit(&self) -> Result<EditorSubmit, String> {
        self.validate()?;
        let title = self.field_value(EditorFieldId::Title).trim().to_string();
        let priority = non_empty(self.field_value(EditorFieldId::Priority));
        let parent = non_empty(self.field_value(EditorFieldId::Parent));
        let relates_id = non_empty(self.field_value(EditorFieldId::RelatesId));
        let relates_desc = non_empty(self.field_value(EditorFieldId::RelatesDesc));
        let relates = match (relates_id, relates_desc) {
            (Some(id), Some(description)) => Some(RelateInput { id, description }),
            _ => None,
        };
        let body = self.field_value(EditorFieldId::Body).to_string();

        Ok(EditorSubmit {
            title,
            priority,
            parent,
            relates,
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
        let relates_id = non_empty(self.field_value(EditorFieldId::RelatesId));
        let relates_desc = non_empty(self.field_value(EditorFieldId::RelatesDesc));
        if relates_id.is_some() && relates_desc.is_none() {
            return Err("relation description required".to_string());
        }
        if relates_id.is_none() && relates_desc.is_some() {
            return Err("relation id required".to_string());
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

    fn field_value(&self, id: EditorFieldId) -> &str {
        self.fields
            .iter()
            .find(|field| field.id == id)
            .map(|field| field.value.as_str())
            .unwrap_or("")
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

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
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
        let mut editor = EditorState::new_task("P2".to_string());
        for _ in 0..editor.fields().len() {
            let action = editor.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
            assert_eq!(action, EditorAction::None);
        }
        assert_eq!(editor.error(), Some("title is required"));
    }

    #[test]
    fn editor_validates_relates_pair() {
        let mut editor = EditorState::new_task("P2".to_string());
        if let Some(field) = editor
            .fields
            .iter_mut()
            .find(|f| f.id == EditorFieldId::Title)
        {
            field.value = "Title".to_string();
        }
        if let Some(field) = editor
            .fields
            .iter_mut()
            .find(|f| f.id == EditorFieldId::RelatesDesc)
        {
            field.value = "needs id".to_string();
        }
        for _ in 0..editor.fields().len() {
            let action = editor.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
            assert_eq!(action, EditorAction::None);
        }
        assert_eq!(editor.error(), Some("relation id required"));
    }

    #[test]
    fn priority_picker_selects_current() {
        let picker = PriorityPicker::new("p3");
        assert_eq!(picker.selected_priority(), "P3");
    }
}
