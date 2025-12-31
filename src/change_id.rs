//! Change-Id trailer helpers.

use std::path::Path;

use uuid::Uuid;

use crate::error::Result;

/// Generate a new Change-Id value.
pub fn generate_change_id() -> String {
    Uuid::new_v4().to_string()
}

/// Return the first Change-Id found in a commit message.
pub fn find_change_id(message: &str) -> Option<String> {
    for line in message.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("Change-Id:") {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Ensure a Change-Id trailer is present in a message.
///
/// Returns the updated message and whether it was modified.
pub fn ensure_change_id(message: &str) -> (String, bool) {
    if find_change_id(message).is_some() {
        return (message.to_string(), false);
    }

    let change_id = generate_change_id();
    let updated = append_change_id(message, &change_id);
    (updated, true)
}

/// Ensure a Change-Id trailer exists in the commit message file.
///
/// Returns true if the file was modified.
pub fn ensure_change_id_file(path: &Path) -> Result<bool> {
    let contents = std::fs::read_to_string(path)?;
    let (updated, changed) = ensure_change_id(&contents);
    if changed {
        std::fs::write(path, updated)?;
    }
    Ok(changed)
}

fn append_change_id(message: &str, change_id: &str) -> String {
    let trimmed = message.trim_end_matches(['\n', '\r']);
    if trimmed.is_empty() {
        return format!("Change-Id: {change_id}\n");
    }

    format!("{trimmed}\n\nChange-Id: {change_id}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_existing_change_id() {
        let msg = "Fix: update config\n\nChange-Id: 1234\n";
        assert_eq!(find_change_id(msg), Some("1234".to_string()));
    }

    #[test]
    fn ensure_change_id_adds_trailer() {
        let msg = "Fix: update config";
        let (updated, changed) = ensure_change_id(msg);
        assert!(changed);
        assert!(updated.contains("\n\nChange-Id: "));
    }

    #[test]
    fn ensure_change_id_noop_when_present() {
        let msg = "Fix: update config\n\nChange-Id: abc";
        let (updated, changed) = ensure_change_id(msg);
        assert!(!changed);
        assert_eq!(updated, msg);
    }
}
