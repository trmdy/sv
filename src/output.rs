//! Shared output formatting for sv CLI commands.

use serde::Serialize;

use crate::error::Result;

pub const SCHEMA_VERSION: &str = "sv.v1";

#[derive(Debug, Clone, Copy)]
pub struct OutputOptions {
    pub json: bool,
    pub quiet: bool,
}

#[derive(Debug, Clone)]
pub struct HumanOutput {
    header: String,
    summary: Vec<(String, String)>,
    details: Vec<String>,
    warnings: Vec<String>,
    next_steps: Vec<String>,
}

impl HumanOutput {
    pub fn new(header: impl Into<String>) -> Self {
        Self {
            header: header.into(),
            summary: Vec::new(),
            details: Vec::new(),
            warnings: Vec::new(),
            next_steps: Vec::new(),
        }
    }

    pub fn push_summary(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.summary.push((key.into(), value.into()));
    }

    pub fn push_detail(&mut self, value: impl Into<String>) {
        self.details.push(value.into());
    }

    pub fn push_warning(&mut self, value: impl Into<String>) {
        self.warnings.push(value.into());
    }

    pub fn push_next_step(&mut self, value: impl Into<String>) {
        self.next_steps.push(value.into());
    }
}

pub fn emit_success<T: Serialize>(
    options: OutputOptions,
    command: &str,
    data: &T,
    human: Option<&HumanOutput>,
) -> Result<()> {
    if options.json {
        let warnings = human.map(|h| h.warnings.clone()).unwrap_or_default();
        let next_steps = human.map(|h| h.next_steps.clone()).unwrap_or_default();

        #[derive(Serialize)]
        struct Envelope<'a, T: Serialize> {
            schema_version: &'static str,
            command: &'a str,
            status: &'static str,
            data: &'a T,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            warnings: Vec<String>,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            next_steps: Vec<String>,
        }

        let payload = Envelope {
            schema_version: SCHEMA_VERSION,
            command,
            status: "success",
            data,
            warnings,
            next_steps,
        };

        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    if options.quiet {
        return Ok(());
    }

    if let Some(human) = human {
        println!("{}", format_human(human));
    }

    Ok(())
}

pub fn emit_error(command: &str, err: &crate::error::Error, json: bool) -> Result<()> {
    let next_steps = error_next_steps(err);
    let hint = next_steps.first().map(|step| step.as_str());
    if json {
        #[derive(Serialize)]
        struct ErrorBody<'a> {
            message: &'a str,
            code: i32,
            kind: &'static str,
            #[serde(skip_serializing_if = "Option::is_none")]
            details: Option<serde_json::Value>,
        }

        #[derive(Serialize)]
        struct Envelope<'a> {
            schema_version: &'static str,
            command: &'a str,
            status: &'static str,
            error: ErrorBody<'a>,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            warnings: Vec<String>,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            next_steps: Vec<String>,
        }

        let payload = Envelope {
            schema_version: SCHEMA_VERSION,
            command,
            status: "error",
            error: ErrorBody {
                message: &err.to_string(),
                code: err.exit_code(),
                kind: error_kind(err),
                details: err.details(),
            },
            warnings: Vec::new(),
            next_steps,
        };

        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    eprintln!("error: {err}");
    if let Some(hint) = hint {
        eprintln!("hint: {hint}");
    }
    Ok(())
}

pub fn format_human(output: &HumanOutput) -> String {
    let mut lines = Vec::new();
    lines.push(output.header.clone());

    push_summary(&mut lines, &output.summary);
    push_section(&mut lines, "Details", &output.details);
    push_section(&mut lines, "Warnings", &output.warnings);
    push_section(&mut lines, "Next steps", &output.next_steps);

    lines.join("\n")
}

pub fn infer_command_name_from_args() -> String {
    let mut args = std::env::args().skip(1);
    let mut command = None;
    let mut subcommand = None;

    while let Some(arg) = args.next() {
        if arg.starts_with('-') {
            continue;
        }
        command = Some(arg);
        break;
    }

    let command = match command {
        Some(cmd) => cmd,
        None => return "sv".to_string(),
    };

    if matches!(
        command.as_str(),
        "ws" | "lease" | "protect" | "op" | "actor" | "task"
    ) {
        while let Some(arg) = args.next() {
            if arg.starts_with('-') {
                continue;
            }
            subcommand = Some(arg);
            break;
        }
    }

    if let Some(sub) = subcommand {
        format!("{command} {sub}")
    } else {
        command
    }
}

fn error_kind(err: &crate::error::Error) -> &'static str {
    match err.exit_code() {
        2 => "user_error",
        3 => "policy_blocked",
        _ => "operation_failed",
    }
}

fn error_next_steps(err: &crate::error::Error) -> Vec<String> {
    use crate::error::Error;

    match err {
        Error::ProtectedPath(_) => vec!["sv protect status".to_string()],
        Error::LeaseConflict { path, .. } => {
            vec![format!("sv lease who {}", path.to_string_lossy())]
        }
        Error::NoteRequired(_) => vec!["sv take <path> --note \"...\"".to_string()],
        Error::RepoNotFound(_) | Error::NotARepo(_) => vec!["sv init".to_string()],
        Error::InvalidConfig(_) => vec!["fix .sv.toml then retry".to_string()],
        _ => Vec::new(),
    }
}

fn push_summary(lines: &mut Vec<String>, summary: &[(String, String)]) {
    if summary.is_empty() {
        return;
    }

    lines.push(String::new());
    lines.push("Summary:".to_string());
    for (key, value) in summary {
        if value.is_empty() {
            lines.push(format!("- {key}"));
        } else {
            lines.push(format!("- {key}: {value}"));
        }
    }
}

fn push_section(lines: &mut Vec<String>, title: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }

    lines.push(String::new());
    lines.push(format!("{title}:"));
    for item in items {
        lines.push(format!("- {item}"));
    }
}
