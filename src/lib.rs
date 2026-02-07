//! sv - Simultaneous Versioning Library
//!
//! This library provides the core functionality for the sv CLI tool,
//! enabling Git coordination for parallel agents.
//!
//! # Core Concepts
//!
//! - **Workspaces**: agent sandboxes (Git worktrees)
//! - **Leases**: Graded reservations over paths with intent and TTL
//! - **Protected Paths**: Global no-edit zones like `.beads/**`
//! - **Risk Prediction**: Overlap detection and conflict simulation
//! - **Operation Log**: JJ-inspired undo capability
//!
//! # Module Organization
//!
//! - `cli`: Command-line interface using clap
//! - `config`: Configuration loading from `.sv.toml`
//! - `error`: Error types and result aliases
//! - `git`: Git operations wrapper using libgit2
//! - `lease`: Lease system for path reservations
//! - `workspace`: Workspace management (Git worktree)
//! - `protect`: Protected path enforcement
//! - `risk`: Overlap detection and risk scoring
//! - `oplog`: Operation log and undo support
//! - `actor`: Actor identity management
//! - `storage`: File storage and directory management
//! - `lock`: File locking and atomic operations for concurrency safety
//! - `task`: Task management and storage

pub mod actor;
pub mod change_id;
pub mod cli;
pub mod config;
pub mod conflict;
pub mod error;
pub mod events;
pub mod git;
pub mod hoist;
pub mod integrations;
pub mod lease;
pub mod lock;
pub mod merge;
pub mod oplog;
pub mod output;
pub mod project;
pub mod protect;
pub mod refs;
pub mod risk;
pub mod selector;
pub mod storage;
pub mod task;
pub mod ui;
pub mod undo;
pub mod workspace;

pub use error::{Error, Result};
