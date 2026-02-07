//! Project entities for sv.
//!
//! Projects are standalone grouping entities. Tasks can reference projects by
//! project id via task relation events (`task_project_set` / `task_project_clear`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::error::{Error, Result};
use crate::lock::{FileLock, DEFAULT_LOCK_TIMEOUT_MS};
use crate::storage::Storage;

const TASKS_DIR: &str = ".tasks";
const PROJECTS_LOG: &str = "projects.jsonl";
const PROJECTS_SNAPSHOT: &str = "projects.snapshot.json";
const PROJECTS_SCHEMA_VERSION: &str = "sv.projects.v1";
const DEFAULT_PROJECT_PREFIX: &str = "prj";
const PROJECT_ID_SUFFIX_LEN: usize = 8;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectEventType {
    ProjectCreated,
    ProjectEdited,
    ProjectArchived,
    ProjectUnarchived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEvent {
    pub event_id: String,
    pub project_id: String,
    #[serde(rename = "type")]
    pub event_type: ProjectEventType,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl ProjectEvent {
    pub fn new(event_type: ProjectEventType, project_id: impl Into<String>) -> Self {
        Self {
            event_id: Ulid::new().to_string(),
            project_id: project_id.into(),
            event_type,
            timestamp: Utc::now(),
            actor: None,
            name: None,
            description: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSnapshot {
    pub schema_version: String,
    pub generated_at: DateTime<Utc>,
    pub projects: Vec<ProjectRecord>,
}

impl ProjectSnapshot {
    pub fn empty() -> Self {
        Self {
            schema_version: PROJECTS_SCHEMA_VERSION.to_string(),
            generated_at: Utc::now(),
            projects: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectSyncReport {
    pub total_events: usize,
    pub total_projects: usize,
}

#[derive(Debug, Clone)]
pub struct ProjectStore {
    storage: Storage,
}

impl ProjectStore {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    pub fn projects_dir(&self) -> PathBuf {
        self.storage.workspace_root().join(TASKS_DIR)
    }

    pub fn tracked_log_path(&self) -> PathBuf {
        self.projects_dir().join(PROJECTS_LOG)
    }

    pub fn tracked_snapshot_path(&self) -> PathBuf {
        self.projects_dir().join(PROJECTS_SNAPSHOT)
    }

    pub fn shared_log_path(&self) -> PathBuf {
        self.storage.shared_dir().join(PROJECTS_LOG)
    }

    pub fn shared_snapshot_path(&self) -> PathBuf {
        self.storage.shared_dir().join(PROJECTS_SNAPSHOT)
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(self.projects_dir())?;
        std::fs::create_dir_all(self.storage.shared_dir())?;
        Ok(())
    }

    pub fn append_event(&self, event: ProjectEvent) -> Result<()> {
        self.ensure_dirs()?;
        self.append_event_to_log(&self.tracked_log_path(), &event)?;
        self.append_event_to_log(&self.shared_log_path(), &event)?;
        self.apply_event_to_snapshot(&self.tracked_snapshot_path(), &event)?;
        self.apply_event_to_snapshot(&self.shared_snapshot_path(), &event)?;
        Ok(())
    }

    pub fn list(&self, include_archived: bool) -> Result<Vec<ProjectRecord>> {
        let mut projects = self.load_snapshot_prefer_shared()?.projects;
        if !include_archived {
            projects.retain(|project| !project.archived);
        }
        Ok(projects)
    }

    pub fn get(&self, project_id: &str) -> Result<ProjectRecord> {
        let resolved = self.resolve_project_id(project_id)?;
        let projects = self.load_snapshot_prefer_shared()?.projects;
        projects
            .into_iter()
            .find(|project| project.id == resolved)
            .ok_or_else(|| Error::InvalidArgument(format!("project not found: {project_id}")))
    }

    pub fn create(
        &self,
        name: &str,
        description: Option<String>,
        actor: Option<String>,
    ) -> Result<String> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(Error::InvalidArgument(
                "project name cannot be empty".to_string(),
            ));
        }
        let project_id = self.generate_project_id()?;
        self.create_with_id(&project_id, trimmed, description, actor)?;
        Ok(project_id)
    }

    pub fn create_with_id(
        &self,
        project_id: &str,
        name: &str,
        description: Option<String>,
        actor: Option<String>,
    ) -> Result<()> {
        let project_id = project_id.trim();
        if project_id.is_empty() {
            return Err(Error::InvalidArgument(
                "project id cannot be empty".to_string(),
            ));
        }
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::InvalidArgument(
                "project name cannot be empty".to_string(),
            ));
        }
        if self.try_resolve_project_id(project_id)?.is_some() {
            return Err(Error::InvalidArgument(format!(
                "project already exists: {project_id}"
            )));
        }

        let mut event = ProjectEvent::new(ProjectEventType::ProjectCreated, project_id.to_string());
        event.actor = actor;
        event.name = Some(name.to_string());
        event.description = normalize_description(description);
        self.append_event(event)
    }

    pub fn edit(
        &self,
        project_id: &str,
        name: Option<String>,
        description: Option<String>,
        actor: Option<String>,
    ) -> Result<bool> {
        let resolved = self.resolve_project_id(project_id)?;
        let current = self.get(&resolved)?;
        let next_name = name.as_deref().map(str::trim).map(str::to_string);
        let has_description_input = description.is_some();
        let next_description = description
            .map(Some)
            .map(normalize_description)
            .unwrap_or(None);
        let name_changed = next_name
            .as_deref()
            .map(|value| value != current.name)
            .unwrap_or(false);
        let description_changed = has_description_input && next_description != current.description;
        if !name_changed && !description_changed {
            return Ok(false);
        }

        let mut event = ProjectEvent::new(ProjectEventType::ProjectEdited, resolved);
        event.actor = actor;
        if let Some(name) = next_name {
            if name.is_empty() {
                return Err(Error::InvalidArgument(
                    "project name cannot be empty".to_string(),
                ));
            }
            event.name = Some(name);
        }
        if has_description_input {
            event.description = next_description;
        }
        self.append_event(event)?;
        Ok(true)
    }

    pub fn set_archived(
        &self,
        project_id: &str,
        archived: bool,
        actor: Option<String>,
    ) -> Result<bool> {
        let resolved = self.resolve_project_id(project_id)?;
        let current = self.get(&resolved)?;
        if current.archived == archived {
            return Ok(false);
        }
        let mut event = ProjectEvent::new(
            if archived {
                ProjectEventType::ProjectArchived
            } else {
                ProjectEventType::ProjectUnarchived
            },
            resolved,
        );
        event.actor = actor;
        self.append_event(event)?;
        Ok(true)
    }

    pub fn try_resolve_project_id(&self, input: &str) -> Result<Option<String>> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(Error::InvalidArgument(
                "project id cannot be empty".to_string(),
            ));
        }
        let needle = trimmed.to_ascii_lowercase();
        let snapshot = self.snapshot_readonly()?;
        let mut exact = Vec::new();
        let mut prefix = Vec::new();
        for project in snapshot.projects {
            let id = project.id.to_ascii_lowercase();
            if id == needle {
                exact.push(project.id.clone());
                continue;
            }
            if id.starts_with(&needle) {
                prefix.push(project.id.clone());
            }
        }

        if exact.len() == 1 {
            return Ok(Some(exact.remove(0)));
        }
        if exact.len() > 1 {
            return Err(Error::InvalidArgument(format!(
                "ambiguous project id '{}': {}",
                trimmed,
                exact.join(", ")
            )));
        }

        prefix.sort();
        prefix.dedup();
        if prefix.len() > 1 {
            return Err(Error::InvalidArgument(format!(
                "ambiguous project id '{}': {}",
                trimmed,
                prefix.join(", ")
            )));
        }
        Ok(prefix.into_iter().next())
    }

    pub fn resolve_project_id(&self, input: &str) -> Result<String> {
        self.try_resolve_project_id(input)?
            .ok_or_else(|| Error::InvalidArgument(format!("project not found: {}", input.trim())))
    }

    pub fn generate_project_id(&self) -> Result<String> {
        let snapshot = self.snapshot_readonly()?;
        let existing: HashSet<String> = snapshot
            .projects
            .into_iter()
            .map(|project| project.id)
            .collect();
        loop {
            let raw = Ulid::new().to_string().to_ascii_lowercase();
            let candidate = format!(
                "{DEFAULT_PROJECT_PREFIX}-{}",
                &raw[raw.len() - PROJECT_ID_SUFFIX_LEN..]
            );
            if !existing.contains(&candidate) {
                return Ok(candidate);
            }
        }
    }

    pub fn snapshot_readonly(&self) -> Result<ProjectSnapshot> {
        if let Some(snapshot) = self.load_snapshot(&self.shared_snapshot_path())? {
            return Ok(snapshot);
        }
        if self.shared_log_path().exists() {
            let events = self.load_events(&self.shared_log_path())?;
            return self.build_snapshot(&events);
        }
        if let Some(snapshot) = self.load_snapshot(&self.tracked_snapshot_path())? {
            return Ok(snapshot);
        }
        let events = self.load_events(&self.tracked_log_path())?;
        self.build_snapshot(&events)
    }

    pub fn sync(&self) -> Result<ProjectSyncReport> {
        self.ensure_dirs()?;
        let tracked = self.load_events(&self.tracked_log_path())?;
        let shared = self.load_events(&self.shared_log_path())?;
        let mut merged = merge_events(tracked, shared);
        sort_events(&mut merged);
        let snapshot = self.build_snapshot(&merged)?;
        self.write_events(&self.tracked_log_path(), &merged)?;
        self.write_events(&self.shared_log_path(), &merged)?;
        self.write_snapshot(&self.tracked_snapshot_path(), &snapshot)?;
        self.write_snapshot(&self.shared_snapshot_path(), &snapshot)?;
        Ok(ProjectSyncReport {
            total_events: merged.len(),
            total_projects: snapshot.projects.len(),
        })
    }

    fn load_snapshot_prefer_shared(&self) -> Result<ProjectSnapshot> {
        if let Some(snapshot) = self.load_snapshot(&self.shared_snapshot_path())? {
            return Ok(snapshot);
        }
        if self.shared_log_path().exists() {
            let events = self.load_events(&self.shared_log_path())?;
            let snapshot = self.build_snapshot(&events)?;
            let _ = self.write_snapshot(&self.shared_snapshot_path(), &snapshot);
            return Ok(snapshot);
        }
        if let Some(snapshot) = self.load_snapshot(&self.tracked_snapshot_path())? {
            return Ok(snapshot);
        }
        let events = self.load_events(&self.tracked_log_path())?;
        self.build_snapshot(&events)
    }

    fn load_events(&self, path: &Path) -> Result<Vec<ProjectEvent>> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        self.storage.read_jsonl(path)
    }

    fn write_events(&self, path: &Path, events: &[ProjectEvent]) -> Result<()> {
        let lock_path = path.with_extension("lock");
        let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;
        let mut buffer = Vec::new();
        for event in events {
            let json = serde_json::to_string(event)?;
            buffer.extend_from_slice(json.as_bytes());
            buffer.push(b'\n');
        }
        self.storage.write_atomic(path, &buffer)
    }

    fn append_event_to_log(&self, path: &Path, event: &ProjectEvent) -> Result<()> {
        let lock_path = path.with_extension("lock");
        let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;
        self.storage.append_jsonl(path, event)
    }

    fn load_snapshot(&self, path: &Path) -> Result<Option<ProjectSnapshot>> {
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(self.storage.read_json(path)?))
    }

    fn write_snapshot(&self, path: &Path, snapshot: &ProjectSnapshot) -> Result<()> {
        let lock_path = path.with_extension("lock");
        let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;
        self.storage.write_json(path, snapshot)
    }

    fn apply_event_to_snapshot(&self, path: &Path, event: &ProjectEvent) -> Result<()> {
        let lock_path = path.with_extension("lock");
        let _lock = FileLock::acquire(&lock_path, DEFAULT_LOCK_TIMEOUT_MS)?;
        let mut snapshot = self
            .load_snapshot(path)?
            .unwrap_or_else(ProjectSnapshot::empty);
        let mut map: HashMap<String, ProjectRecord> = snapshot
            .projects
            .drain(..)
            .map(|project| (project.id.clone(), project))
            .collect();
        apply_event(&mut map, event)?;
        let mut projects: Vec<ProjectRecord> = map.into_values().collect();
        projects.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        snapshot.projects = projects;
        snapshot.generated_at = Utc::now();
        self.storage.write_json(path, &snapshot)
    }

    fn build_snapshot(&self, events: &[ProjectEvent]) -> Result<ProjectSnapshot> {
        let mut map: HashMap<String, ProjectRecord> = HashMap::new();
        let mut sorted = events.to_vec();
        sort_events(&mut sorted);
        for event in &sorted {
            apply_event(&mut map, event)?;
        }
        let mut projects: Vec<ProjectRecord> = map.into_values().collect();
        projects.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(ProjectSnapshot {
            schema_version: PROJECTS_SCHEMA_VERSION.to_string(),
            generated_at: Utc::now(),
            projects,
        })
    }
}

fn normalize_description(description: Option<String>) -> Option<String> {
    let description = description?;
    if description.trim().is_empty() {
        None
    } else {
        Some(description)
    }
}

fn apply_event(map: &mut HashMap<String, ProjectRecord>, event: &ProjectEvent) -> Result<()> {
    match event.event_type {
        ProjectEventType::ProjectCreated => {
            if map.contains_key(&event.project_id) {
                return Err(Error::InvalidArgument(format!(
                    "project already exists: {}",
                    event.project_id
                )));
            }
            let name = event.name.clone().ok_or_else(|| {
                Error::InvalidArgument(format!("missing project name for {}", event.project_id))
            })?;
            if name.trim().is_empty() {
                return Err(Error::InvalidArgument(format!(
                    "project name cannot be empty: {}",
                    event.project_id
                )));
            }
            map.insert(
                event.project_id.clone(),
                ProjectRecord {
                    id: event.project_id.clone(),
                    name,
                    description: normalize_description(event.description.clone()),
                    archived: false,
                    created_at: event.timestamp,
                    updated_at: event.timestamp,
                    created_by: event.actor.clone(),
                    updated_by: event.actor.clone(),
                },
            );
        }
        ProjectEventType::ProjectEdited => {
            let project = map.get_mut(&event.project_id).ok_or_else(|| {
                Error::InvalidArgument(format!("project not found: {}", event.project_id))
            })?;
            if let Some(name) = event.name.as_ref() {
                if name.trim().is_empty() {
                    return Err(Error::InvalidArgument(format!(
                        "project name cannot be empty: {}",
                        event.project_id
                    )));
                }
                project.name = name.clone();
            }
            if event.description.is_some() {
                project.description = normalize_description(event.description.clone());
            }
            project.updated_at = event.timestamp;
            project.updated_by = event.actor.clone();
        }
        ProjectEventType::ProjectArchived => {
            let project = map.get_mut(&event.project_id).ok_or_else(|| {
                Error::InvalidArgument(format!("project not found: {}", event.project_id))
            })?;
            project.archived = true;
            project.updated_at = event.timestamp;
            project.updated_by = event.actor.clone();
        }
        ProjectEventType::ProjectUnarchived => {
            let project = map.get_mut(&event.project_id).ok_or_else(|| {
                Error::InvalidArgument(format!("project not found: {}", event.project_id))
            })?;
            project.archived = false;
            project.updated_at = event.timestamp;
            project.updated_by = event.actor.clone();
        }
    }
    Ok(())
}

fn merge_events(mut a: Vec<ProjectEvent>, b: Vec<ProjectEvent>) -> Vec<ProjectEvent> {
    let mut seen = HashSet::new();
    a.retain(|event| seen.insert(event.event_id.clone()));
    for event in b {
        if seen.insert(event.event_id.clone()) {
            a.push(event);
        }
    }
    a
}

fn sort_events(events: &mut [ProjectEvent]) {
    events.sort_by(|a, b| {
        a.timestamp
            .cmp(&b.timestamp)
            .then_with(|| a.event_id.cmp(&b.event_id))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use crate::storage::Storage;

    fn setup_store() -> (tempfile::TempDir, ProjectStore) {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join(".git")).expect("git dir");
        let root = dir.path().to_path_buf();
        let storage = Storage::new(root.clone(), root.join(".git"), root.clone());
        (dir, ProjectStore::new(storage))
    }

    #[test]
    fn create_and_resolve_project() {
        let (_dir, store) = setup_store();
        let project_id = store
            .create("Alpha", Some("desc".to_string()), Some("alice".to_string()))
            .expect("create");
        let resolved = store.resolve_project_id(&project_id).expect("resolve");
        assert_eq!(resolved, project_id);
    }

    #[test]
    fn list_excludes_archived_by_default() {
        let (_dir, store) = setup_store();
        let project_id = store.create("Alpha", None, None).expect("create");
        store
            .set_archived(&project_id, true, None)
            .expect("archive project");
        assert!(store.list(false).expect("list").is_empty());
        assert_eq!(store.list(true).expect("list all").len(), 1);
    }

    #[test]
    fn sync_merges_tracked_and_shared_logs() {
        let (_dir, store) = setup_store();
        store.ensure_dirs().expect("dirs");

        let mut tracked = ProjectEvent::new(ProjectEventType::ProjectCreated, "prj-one");
        tracked.name = Some("One".to_string());
        store
            .storage
            .append_jsonl(&store.tracked_log_path(), &tracked)
            .expect("append tracked");

        let mut shared = ProjectEvent::new(ProjectEventType::ProjectCreated, "prj-two");
        shared.name = Some("Two".to_string());
        store
            .storage
            .append_jsonl(&store.shared_log_path(), &shared)
            .expect("append shared");

        let report = store.sync().expect("sync");
        assert_eq!(report.total_projects, 2);
    }
}
