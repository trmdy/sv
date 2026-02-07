//! File locking and atomic operations for sv
//!
//! This module provides concurrency-safe file operations:
//! - File locking (using fs2/flock) for `.git/sv/` writes
//! - Atomic write pattern (write temp + rename)
//! - Lock timeout with configurable wait
//! - Error handling for lock contention
//!
//! Critical for multi-agent safety per spec Section 13.4

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use fs2::FileExt;

use crate::error::{Error, Result};

/// Default lock timeout in milliseconds
pub const DEFAULT_LOCK_TIMEOUT_MS: u64 = 5000;

/// Default retry interval when waiting for a lock
const LOCK_RETRY_INTERVAL_MS: u64 = 50;

fn is_lock_contended(err: &io::Error) -> bool {
    if err.kind() == io::ErrorKind::WouldBlock {
        return true;
    }

    // On Windows, fs2/libc can surface lock/sharing violations as "Other".
    // Treat them as contention so callers get Err(LockFailed) after timeout.
    #[cfg(windows)]
    {
        matches!(err.raw_os_error(), Some(32) | Some(33))
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// A file lock guard that releases the lock when dropped
pub struct FileLock {
    file: File,
    path: PathBuf,
}

impl FileLock {
    /// Acquire an exclusive lock on a file with timeout
    ///
    /// If the file doesn't exist, it will be created.
    /// Returns an error if the lock cannot be acquired within the timeout.
    pub fn acquire(path: impl AsRef<Path>, timeout_ms: u64) -> Result<Self> {
        let path = path.as_ref();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Open or create the lock file
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;

        let start = Instant::now();
        let timeout = Duration::from_millis(timeout_ms);
        let retry_interval = Duration::from_millis(LOCK_RETRY_INTERVAL_MS);

        loop {
            // Try to acquire exclusive lock
            match file.try_lock_exclusive() {
                Ok(()) => {
                    return Ok(FileLock {
                        file,
                        path: path.to_path_buf(),
                    });
                }
                Err(e) if is_lock_contended(&e) => {
                    // Lock is held by another process
                    if start.elapsed() >= timeout {
                        return Err(Error::LockFailed(path.to_path_buf()));
                    }
                    std::thread::sleep(retry_interval);
                }
                Err(e) => {
                    return Err(Error::Io(e));
                }
            }
        }
    }

    /// Acquire an exclusive lock without timeout (blocking)
    pub fn acquire_blocking(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;

        file.lock_exclusive()?;

        Ok(FileLock {
            file,
            path: path.to_path_buf(),
        })
    }

    /// Try to acquire a lock without waiting
    ///
    /// Returns `Ok(Some(lock))` if acquired, `Ok(None)` if would block,
    /// or `Err` for other errors.
    pub fn try_acquire(path: impl AsRef<Path>) -> Result<Option<Self>> {
        let path = path.as_ref();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;

        match file.try_lock_exclusive() {
            Ok(()) => Ok(Some(FileLock {
                file,
                path: path.to_path_buf(),
            })),
            Err(e) if is_lock_contended(&e) => Ok(None),
            Err(e) => Err(Error::Io(e)),
        }
    }

    /// Get a reference to the underlying file
    pub fn file(&self) -> &File {
        &self.file
    }

    /// Get a mutable reference to the underlying file
    pub fn file_mut(&mut self) -> &mut File {
        &mut self.file
    }

    /// Get the path to the locked file
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        // Unlock the file - ignore errors during drop
        let _ = self.file.unlock();
    }
}

/// Atomically write data to a file
///
/// This writes to a temporary file in the same directory, then renames
/// it to the target path. This ensures the file is either fully written
/// or not modified at all.
///
/// Note: This does NOT acquire a lock. Use `write_atomic_locked` if you
/// need to coordinate with other processes.
pub fn write_atomic(path: impl AsRef<Path>, data: &[u8]) -> Result<()> {
    let path = path.as_ref();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Create temp file in same directory (important for atomic rename)
    let temp_path = path.with_extension(format!(
        "{}.tmp.{}",
        path.extension().and_then(|e| e.to_str()).unwrap_or(""),
        std::process::id()
    ));

    // Write to temp file
    let mut temp_file = File::create(&temp_path)?;
    temp_file.write_all(data)?;
    temp_file.sync_all()?; // Ensure data is flushed to disk
    drop(temp_file);

    // Atomic rename
    fs::rename(&temp_path, path)?;

    Ok(())
}

/// Atomically write string data to a file
pub fn write_atomic_str(path: impl AsRef<Path>, data: &str) -> Result<()> {
    write_atomic(path, data.as_bytes())
}

/// Write data atomically while holding a lock on a separate lock file
///
/// This is the recommended pattern for files that may be read/written
/// concurrently by multiple sv processes:
///
/// 1. Acquire lock on `<path>.lock`
/// 2. Write to temp file
/// 3. Rename temp to target
/// 4. Release lock (automatic on drop)
pub fn write_atomic_locked(path: impl AsRef<Path>, data: &[u8], timeout_ms: u64) -> Result<()> {
    let path = path.as_ref();
    let lock_path = PathBuf::from(format!("{}.lock", path.display()));

    // Acquire lock
    let _lock = FileLock::acquire(&lock_path, timeout_ms)?;

    // Write atomically
    write_atomic(path, data)?;

    // Lock released on drop
    Ok(())
}

/// Read a file while holding a lock
///
/// This ensures the file isn't being written to while we read it.
pub fn read_locked(path: impl AsRef<Path>, timeout_ms: u64) -> Result<Vec<u8>> {
    let path = path.as_ref();
    let lock_path = PathBuf::from(format!("{}.lock", path.display()));

    // Acquire lock
    let _lock = FileLock::acquire(&lock_path, timeout_ms)?;

    // Read file
    let data = fs::read(path)?;

    // Lock released on drop
    Ok(data)
}

/// Read a file as string while holding a lock
pub fn read_locked_str(path: impl AsRef<Path>, timeout_ms: u64) -> Result<String> {
    let data = read_locked(path, timeout_ms)?;
    String::from_utf8(data).map_err(|e| Error::OperationFailed(format!("Invalid UTF-8: {}", e)))
}

/// A wrapper for performing multiple operations while holding a lock
pub struct LockedOperation {
    lock: FileLock,
}

impl LockedOperation {
    /// Start a locked operation on a file
    pub fn begin(lock_path: impl AsRef<Path>, timeout_ms: u64) -> Result<Self> {
        let lock = FileLock::acquire(lock_path, timeout_ms)?;
        Ok(LockedOperation { lock })
    }

    /// Get a reference to the lock
    pub fn lock(&self) -> &FileLock {
        &self.lock
    }

    /// Atomically write to a file (while holding the lock)
    pub fn write_atomic(&self, path: impl AsRef<Path>, data: &[u8]) -> Result<()> {
        write_atomic(path, data)
    }

    /// Read a file (while holding the lock)
    pub fn read(&self, path: impl AsRef<Path>) -> Result<Vec<u8>> {
        Ok(fs::read(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::thread;
    use tempfile::TempDir;

    #[test]
    fn test_file_lock_acquire_release() {
        let temp_dir = TempDir::new().unwrap();
        let lock_path = temp_dir.path().join("test.lock");

        // Acquire lock
        let lock = FileLock::acquire(&lock_path, 1000).unwrap();
        assert!(lock_path.exists());

        // Try to acquire again (should fail with timeout)
        let result = FileLock::try_acquire(&lock_path).unwrap();
        assert!(result.is_none());

        // Drop the lock
        drop(lock);

        // Now should be able to acquire
        let lock2 = FileLock::try_acquire(&lock_path).unwrap();
        assert!(lock2.is_some());
    }

    #[test]
    fn test_atomic_write() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        write_atomic_str(&file_path, "Hello, World!").unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello, World!");

        // Overwrite
        write_atomic_str(&file_path, "Updated!").unwrap();
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Updated!");
    }

    #[test]
    fn test_atomic_write_locked() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("data.json");

        write_atomic_locked(&file_path, b"{\"key\": \"value\"}", 1000).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "{\"key\": \"value\"}");
    }

    #[test]
    fn test_concurrent_locking() {
        let temp_dir = TempDir::new().unwrap();
        let lock_path = temp_dir.path().join("concurrent.lock");
        let lock_path_clone = lock_path.clone();

        // Acquire lock in main thread
        let lock = FileLock::acquire(&lock_path, 1000).unwrap();

        // Try to acquire in another thread (should fail quickly with try_acquire)
        let handle = thread::spawn(move || {
            let result = FileLock::try_acquire(&lock_path_clone).unwrap();
            result.is_none() // Should be None since main thread holds lock
        });

        let other_thread_blocked = handle.join().unwrap();
        assert!(other_thread_blocked);

        // Release and let another thread acquire
        drop(lock);

        let lock_path_clone2 = temp_dir.path().join("concurrent.lock");
        let handle2 = thread::spawn(move || FileLock::acquire(&lock_path_clone2, 1000).is_ok());

        assert!(handle2.join().unwrap());
    }

    #[test]
    fn stress_single_lock_holder() {
        let temp_dir = TempDir::new().unwrap();
        let lock_path = temp_dir.path().join("stress.lock");

        let threads = 12;
        let barrier = Arc::new(Barrier::new(threads));
        let in_lock = Arc::new(AtomicUsize::new(0));
        let max_concurrent = Arc::new(AtomicUsize::new(0));
        let acquired = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::with_capacity(threads);
        for _ in 0..threads {
            let barrier = Arc::clone(&barrier);
            let in_lock = Arc::clone(&in_lock);
            let max_concurrent = Arc::clone(&max_concurrent);
            let acquired = Arc::clone(&acquired);
            let lock_path = lock_path.clone();

            handles.push(thread::spawn(move || {
                barrier.wait();
                let _lock = FileLock::acquire(&lock_path, 2000).unwrap();

                let current = in_lock.fetch_add(1, Ordering::SeqCst) + 1;
                let _ = max_concurrent.fetch_max(current, Ordering::SeqCst);

                thread::sleep(Duration::from_millis(10));

                in_lock.fetch_sub(1, Ordering::SeqCst);
                acquired.fetch_add(1, Ordering::SeqCst);
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(acquired.load(Ordering::SeqCst), threads);
        assert_eq!(max_concurrent.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn timeout_returns_lock_failed() {
        let temp_dir = TempDir::new().unwrap();
        let lock_path = temp_dir.path().join("timeout.lock");

        let _lock = FileLock::acquire(&lock_path, 1000).unwrap();
        let result = FileLock::acquire(&lock_path, 50);
        assert!(matches!(result, Err(Error::LockFailed(_))));
    }

    #[test]
    fn atomic_write_locked_is_consistent() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("data.json");

        let threads = 8;
        let barrier = Arc::new(Barrier::new(threads));
        let mut handles = Vec::with_capacity(threads);
        let mut expected = Vec::with_capacity(threads);

        for idx in 0..threads {
            let barrier = Arc::clone(&barrier);
            let file_path = file_path.clone();
            let payload = format!("{{\"writer\":{},\"data\":\"{}\"}}", idx, "x".repeat(64));
            expected.push(payload.clone());

            handles.push(thread::spawn(move || {
                barrier.wait();
                write_atomic_locked(&file_path, payload.as_bytes(), 2000).unwrap();
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let final_contents = fs::read_to_string(&file_path).unwrap();
        assert!(expected.contains(&final_contents));
    }
}
