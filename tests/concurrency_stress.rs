mod support;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use assert_cmd::cargo::cargo_bin;
use sv::error::Error;
use sv::lock::FileLock;
use sv::oplog::OpLog;
use sv::storage::Storage;
use tempfile::TempDir;

use support::TestRepo;

const READY_POLL_INTERVAL: Duration = Duration::from_millis(25);
const READY_TIMEOUT: Duration = Duration::from_secs(2);

fn sv_bin() -> PathBuf {
    cargo_bin("sv")
}

fn spawn_sv(repo: &Path, args: &[String], actor: Option<&str>) -> std::io::Result<Child> {
    let mut cmd = Command::new(sv_bin());
    cmd.current_dir(repo);
    if let Some(actor) = actor {
        cmd.env("SV_ACTOR", actor);
    }
    cmd.args(args);
    cmd.spawn()
}

fn run_sv(repo: &Path, args: &[String], actor: Option<&str>) -> std::io::Result<()> {
    let status = Command::new(sv_bin())
        .current_dir(repo)
        .envs(actor.map(|value| ("SV_ACTOR", value)))
        .args(args)
        .status()?;
    if !status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("sv {:?} failed", args),
        ));
    }
    Ok(())
}

#[test]
fn lock_helper_process() {
    if std::env::var("SV_LOCK_HELPER").ok().as_deref() != Some("1") {
        return;
    }

    let path = std::env::var("SV_LOCK_PATH").expect("SV_LOCK_PATH");
    let ready = std::env::var("SV_LOCK_READY").expect("SV_LOCK_READY");

    let _lock = FileLock::acquire_blocking(&path).expect("lock helper acquire");
    std::fs::write(&ready, "ready").expect("ready write");
    thread::sleep(Duration::from_secs(2));
}

#[test]
fn file_lock_timeout_when_held_by_other_process() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    let lock_path = dir.path().join("lockfile.lock");
    let ready_path = dir.path().join("ready");

    let mut child = Command::new(std::env::current_exe()?)
        .args(["--exact", "lock_helper_process", "--nocapture"])
        .env("SV_LOCK_HELPER", "1")
        .env("SV_LOCK_PATH", lock_path.display().to_string())
        .env("SV_LOCK_READY", ready_path.display().to_string())
        .spawn()?;

    let start = Instant::now();
    while !ready_path.exists() {
        if start.elapsed() > READY_TIMEOUT {
            let _ = child.kill();
            return Err("lock helper not ready".into());
        }
        thread::sleep(READY_POLL_INTERVAL);
    }

    match FileLock::acquire(&lock_path, 100) {
        Ok(_) => return Err("expected lock timeout".into()),
        Err(err) => assert!(matches!(err, Error::LockFailed(_))),
    }

    child.wait()?;
    Ok(())
}

#[test]
fn workspace_registry_handles_parallel_updates() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.write_file("README.md", "init")?;
    repo.commit_all("init")?;

    let repo_path = repo.path().to_path_buf();
    let bin = Arc::new(sv_bin());
    let count = 4;

    let mut handles = Vec::new();
    for idx in 0..count {
        let name = format!("ws-{idx}");
        let repo_path = repo_path.clone();
        let bin = Arc::clone(&bin);
        handles.push(thread::spawn(move || {
            let args = vec!["ws".to_string(), "here".to_string(), "--name".to_string(), name];
            let status = Command::new(bin.as_ref())
                .current_dir(&repo_path)
                .args(args)
                .status();
            status
        }));
    }

    for handle in handles {
        let status = handle.join().expect("join thread")?;
        assert!(status.success());
    }

    let storage = Storage::for_repo(repo_path);
    let registry = storage.read_workspaces()?;
    assert_eq!(registry.workspaces.len(), count);

    let names: HashSet<_> = registry
        .workspaces
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    for idx in 0..count {
        assert!(names.contains(format!("ws-{idx}").as_str()));
    }

    Ok(())
}

#[test]
fn oplog_append_under_contention() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.write_file("README.md", "init")?;
    repo.commit_all("init")?;

    let repo_path = repo.path().to_path_buf();
    let bin = Arc::new(sv_bin());
    let count = 4;

    let mut handles = Vec::new();
    for idx in 0..count {
        let name = format!("log-{idx}");
        let repo_path = repo_path.clone();
        let bin = Arc::clone(&bin);
        handles.push(thread::spawn(move || {
            let args = vec!["ws".to_string(), "here".to_string(), "--name".to_string(), name];
            Command::new(bin.as_ref())
                .current_dir(&repo_path)
                .args(args)
                .status()
        }));
    }

    for handle in handles {
        let status = handle.join().expect("join thread")?;
        assert!(status.success());
    }

    let storage = Storage::for_repo(repo_path);
    let log = OpLog::for_storage(&storage);
    let records = log.read_all()?;
    let op_count = records
        .iter()
        .filter(|record| record.command.starts_with("sv ws here"))
        .count();
    assert!(op_count >= count);

    Ok(())
}

#[test]
fn lease_release_is_safe_under_parallel_calls() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;

    let repo_path = repo.path().to_path_buf();
    let count = 4;

    for idx in 0..count {
        let pathspec = format!("src/lease-{idx}.rs");
        let args = vec![
            "take".to_string(),
            pathspec,
            "--strength".to_string(),
            "cooperative".to_string(),
            "--intent".to_string(),
            "refactor".to_string(),
        ];
        run_sv(&repo_path, &args, Some("alice"))?;
    }

    let mut handles = Vec::new();
    for idx in 0..count {
        let pathspec = format!("src/lease-{idx}.rs");
        let repo_path = repo_path.clone();
        let args = vec!["release".to_string(), pathspec];
        handles.push(thread::spawn(move || spawn_sv(&repo_path, &args, Some("alice"))));
    }

    for handle in handles {
        let mut child = handle.join().expect("join thread")?;
        let status = child.wait()?;
        assert!(status.success());
    }

    let output = Command::new(sv_bin())
        .current_dir(&repo_path)
        .args(["lease", "ls"])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No active leases."));

    Ok(())
}

#[test]
fn lease_creation_is_safe_under_parallel_calls() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;

    let repo_path = repo.path().to_path_buf();
    let count = 4;

    let mut handles = Vec::new();
    for idx in 0..count {
        let repo_path = repo_path.clone();
        let pathspec = format!("src/lease-create-{idx}.rs");
        let args = vec![
            "take".to_string(),
            pathspec,
            "--strength".to_string(),
            "cooperative".to_string(),
            "--intent".to_string(),
            "refactor".to_string(),
        ];
        handles.push(thread::spawn(move || spawn_sv(&repo_path, &args, Some("alice"))));
    }

    for handle in handles {
        let mut child = handle.join().expect("join thread")?;
        let status = child.wait()?;
        assert!(status.success());
    }

    let storage = Storage::for_repo(repo_path);
    let store = storage.load_leases()?;
    assert_eq!(store.active().count(), count);

    Ok(())
}
