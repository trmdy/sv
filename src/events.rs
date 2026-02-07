//! Event output for external integrations.
//!
//! Events are emitted as JSON lines to stdout or a configured file.

use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::error::{Error, Result};

pub const EVENT_SCHEMA_VERSION: &str = "sv.event.v1";

#[derive(Debug, Clone)]
pub enum EventDestination {
    Stdout,
    File(PathBuf),
}

impl EventDestination {
    pub fn parse(raw: Option<&str>) -> Option<Self> {
        raw.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return None;
            }
            if trimmed == "-" {
                return Some(EventDestination::Stdout);
            }
            Some(EventDestination::File(PathBuf::from(trimmed)))
        })
    }

    pub fn open(&self) -> Result<EventSink> {
        match self {
            EventDestination::Stdout => Ok(EventSink::stdout()),
            EventDestination::File(path) => EventSink::file(path),
        }
    }
}

/// High-level event kinds emitted by sv.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    LeaseCreated,
    LeaseReleased,
    WorkspaceCreated,
    WorkspaceRemoved,
    CommitBlocked,
    CommitCreated,
    TaskCreated,
    TaskStarted,
    TaskStatusChanged,
    TaskPriorityChanged,
    TaskEdited,
    TaskClosed,
    TaskDeleted,
    TaskCommented,
    TaskEpicSet,
    TaskEpicCleared,
    TaskProjectSet,
    TaskProjectCleared,
    TaskParentSet,
    TaskParentCleared,
    TaskBlocked,
    TaskUnblocked,
    TaskRelated,
    TaskUnrelated,
}

/// A structured event with optional payload.
#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub schema_version: &'static str,
    pub event: EventKind,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Event {
    /// Build a new event with an optional payload.
    pub fn new(event: EventKind, actor: Option<String>) -> Self {
        Self {
            schema_version: EVENT_SCHEMA_VERSION,
            event,
            timestamp: Utc::now(),
            actor,
            data: None,
        }
    }

    /// Attach a serializable payload to the event.
    pub fn with_data<T: Serialize>(mut self, data: T) -> Result<Self> {
        self.data = Some(serde_json::to_value(data)?);
        Ok(self)
    }
}

/// Event sink that writes JSONL output to a destination.
pub struct EventSink {
    writer: Box<dyn Write + Send>,
}

impl EventSink {
    /// Emit events to stdout.
    pub fn stdout() -> Self {
        Self {
            writer: Box::new(std::io::stdout()),
        }
    }

    /// Emit events to a file, creating it if necessary.
    pub fn file(path: &Path) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self {
            writer: Box::new(file),
        })
    }

    /// Write a single event as JSONL.
    pub fn emit(&mut self, event: &Event) -> Result<()> {
        let serialized = serde_json::to_vec(event)?;
        self.writer.write_all(&serialized)?;
        self.writer.write_all(b"\n")?;
        self.writer.flush().map_err(Error::Io)?;
        Ok(())
    }
}
