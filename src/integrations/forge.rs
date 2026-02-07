use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy)]
pub enum ForgeTaskHookKind {
    TaskStart,
    TaskBlock,
    TaskClose,
}

#[derive(Debug, Clone)]
struct ForgeHooksConfig {
    enabled: bool,
    loop_ref: String,
    on_task_start_cmd: Option<String>,
    on_task_block_cmd: Option<String>,
    on_task_close_cmd: Option<String>,
}

impl ForgeHooksConfig {
    fn disabled() -> Self {
        Self {
            enabled: false,
            loop_ref: "{actor}".to_string(),
            on_task_start_cmd: None,
            on_task_block_cmd: None,
            on_task_close_cmd: None,
        }
    }

    fn load_from_repo(repo_root: &Path) -> crate::Result<Self> {
        let config_path = repo_root.join(".sv.toml");
        if !config_path.exists() {
            return Ok(Self::disabled());
        }

        let content = std::fs::read_to_string(&config_path)?;
        if content.trim().is_empty() {
            return Ok(Self::disabled());
        }

        let value: toml::Value = toml::from_str(&content)?;
        Ok(Self::from_toml_value(&value))
    }

    fn from_toml_value(value: &toml::Value) -> Self {
        let forge = value
            .get("integrations")
            .and_then(|v| v.get("forge"))
            .and_then(|v| v.as_table());

        let Some(forge) = forge else {
            return Self::disabled();
        };

        let enabled = forge
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let loop_ref = forge
            .get("loop_ref")
            .and_then(|v| v.as_str())
            .unwrap_or("{actor}")
            .to_string();

        let on_task_start_cmd = forge
            .get("on_task_start")
            .and_then(|v| v.as_table())
            .and_then(|t| t.get("cmd"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let on_task_block_cmd = forge
            .get("on_task_block")
            .and_then(|v| v.as_table())
            .and_then(|t| t.get("cmd"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let on_task_close_cmd = forge
            .get("on_task_close")
            .and_then(|v| v.as_table())
            .and_then(|t| t.get("cmd"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Self {
            enabled,
            loop_ref,
            on_task_start_cmd,
            on_task_block_cmd,
            on_task_close_cmd,
        }
    }

    fn cmd_for(&self, kind: ForgeTaskHookKind) -> Option<&str> {
        match kind {
            ForgeTaskHookKind::TaskStart => self.on_task_start_cmd.as_deref(),
            ForgeTaskHookKind::TaskBlock => self.on_task_block_cmd.as_deref(),
            ForgeTaskHookKind::TaskClose => self.on_task_close_cmd.as_deref(),
        }
    }
}

pub fn run_task_hook_best_effort(
    repo_root: &Path,
    kind: ForgeTaskHookKind,
    task_id: &str,
    actor: &str,
) -> Option<String> {
    let cfg = match ForgeHooksConfig::load_from_repo(repo_root) {
        Ok(cfg) => cfg,
        Err(err) => {
            return Some(format!(
                "forge hooks: failed to load .sv.toml; skipping ({})",
                err
            ));
        }
    };

    if !cfg.enabled {
        return None;
    }

    let cmd_template = cfg.cmd_for(kind)?;
    let loop_ref = render_template(&cfg.loop_ref, task_id, actor, None);
    let cmd = render_template(cmd_template, task_id, actor, Some(&loop_ref));

    if cmd.trim().is_empty() {
        return Some("forge hooks: empty cmd; skipping".to_string());
    }

    match run_shell(repo_root, &cmd) {
        Ok(()) => None,
        Err(err) => Some(format!("forge hooks: command failed; {}", err)),
    }
}

fn render_template(template: &str, task_id: &str, actor: &str, loop_ref: Option<&str>) -> String {
    let mut rendered = template
        .replace("{task_id}", task_id)
        .replace("{actor}", actor);
    if let Some(loop_ref) = loop_ref {
        rendered = rendered.replace("{loop_ref}", loop_ref);
    }
    rendered
}

fn run_shell(repo_root: &Path, cmd: &str) -> std::io::Result<()> {
    let mut command = build_shell_command(cmd);
    command.current_dir(repo_root);
    let output = command.output()?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let message = first_non_empty(stderr.trim(), stdout.trim()).unwrap_or("unknown error");

    Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!(
            "exit={} msg={}",
            output.status.code().unwrap_or(-1),
            truncate(message, 400)
        ),
    ))
}

fn build_shell_command(cmd: &str) -> Command {
    if cfg!(windows) {
        let mut command = Command::new("cmd");
        command.args(["/C", cmd]);
        command
    } else {
        let mut command = Command::new("sh");
        command.args(["-lc", cmd]);
        command
    }
}

fn first_non_empty<'a>(a: &'a str, b: &'a str) -> Option<&'a str> {
    if !a.is_empty() {
        return Some(a);
    }
    if !b.is_empty() {
        return Some(b);
    }
    None
}

fn truncate(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }
    value.chars().take(max_len).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_missing_is_disabled() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = ForgeHooksConfig::load_from_repo(dir.path()).expect("load");
        assert!(!cfg.enabled);
    }

    #[test]
    fn template_renders_placeholders() {
        let out = render_template(
            "forge work set {task_id} --loop {loop_ref} --agent {actor}",
            "sv-123",
            "alice",
            Some("alice"),
        );
        assert_eq!(
            out,
            "forge work set sv-123 --loop alice --agent alice".to_string()
        );
    }

    #[cfg(unix)]
    #[test]
    fn run_hook_executes_shell_command() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join(".sv.toml"),
            r#"
[integrations.forge]
enabled = true
loop_ref = "{actor}"

[integrations.forge.on_task_start]
cmd = "printf 'start:{task_id}:{actor}:{loop_ref}\\n' >> hooks.txt"
"#
            .trim(),
        )
        .expect("write config");

        let warning =
            run_task_hook_best_effort(dir.path(), ForgeTaskHookKind::TaskStart, "sv-abc", "alice");
        assert!(warning.is_none(), "unexpected warning: {warning:?}");

        let content = std::fs::read_to_string(dir.path().join("hooks.txt")).expect("read hooks");
        assert_eq!(content, "start:sv-abc:alice:alice\n");
    }

    #[cfg(unix)]
    #[test]
    fn run_hook_failure_is_warning() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join(".sv.toml"),
            r#"
[integrations.forge]
enabled = true
loop_ref = "{actor}"

[integrations.forge.on_task_start]
cmd = "exit 12"
"#
            .trim(),
        )
        .expect("write config");

        let warning =
            run_task_hook_best_effort(dir.path(), ForgeTaskHookKind::TaskStart, "sv-abc", "alice");
        assert!(warning.is_some());
    }
}
