//! Shared output helpers for sv commands.
//!
//! Provides consistent human-readable sections and JSON envelope formatting.

use serde::Serialize;

use crate::error::{Error, Result};

pub const SCHEMA_VERSION: &str = "sv.v1";

/// Output mode flags for commands.
#[derive(Debug, Clone, Copy)]
pub struct OutputOptions {
    pub json: bool,
    pub quiet: bool,
}

/// Human-readable output sections.
#[derive(Debug, Clone, Default)]
pub struct HumanOutput {
    pub header: String,
    pub summary: Vec<(String, String)>,
    pub details: Vec<String>,
    pub warnings: Vec<String>,
    pub next_steps: Vec<String>,
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

    pub fn push_detail(&mut self, line: impl Into<String>) {
        self.details.push(line.into());
    }

    pub fn push_warning(&mut self, line: impl Into<String>) {
        self.warnings.push(line.into());
    }

    pub fn push_next_step(&mut self, line: impl Into<String>) {
        self.next_steps.push(line.into());
    }
}

/// JSON success envelope.
#[derive(Debug, Serialize)]
pub struct JsonEnvelope<T: Serialize> {
    pub schema_version: String,
    pub command: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonError>,
}

/// JSON error payload.
#[derive(Debug, Serialize)]
pub struct JsonError {
    pub message: String,
    pub code: i32,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl JsonError {
    pub fn from_error(err: &Error) -> Self {
        let code = err.exit_code();
        Self {
            message: err.to_string(),
            code,
            kind: kind_for_code(code).to_string(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

/// Emit a success response in JSON or human format.
pub fn emit_success<T: Serialize>(
    options: OutputOptions,
    command: &str,
    result: &T,
    human: Option<&HumanOutput>,
) -> Result<()> {
    if options.json {
        let payload = JsonEnvelope {
            schema_version: SCHEMA_VERSION.to_string(),
            command: command.to_string(),
            ok: true,
            result: Some(result),
            error: None,
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

/// Emit an error response in JSON or human format.
pub fn emit_error(
    options: OutputOptions,
    command: &str,
    err: &Error,
    hint: Option<&str>,
    details: Option<serde_json::Value>,
) -> Result<()> {
    if options.json {
        let error = match details {
            Some(details) => JsonError::from_error(err).with_details(details),
            None => JsonError::from_error(err),
        };
        let payload: JsonEnvelope<serde_json::Value> = JsonEnvelope {
            schema_version: SCHEMA_VERSION.to_string(),
            command: command.to_string(),
            ok: false,
            result: None,
            error: Some(error),
        };
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    eprintln!("error: {}", err);
    if let Some(hint) = hint {
        eprintln!("hint: {hint}");
    }

    Ok(())
}

/// Format human output with standard sections.
pub fn format_human(output: &HumanOutput) -> String {
    let mut lines = Vec::new();
    if !output.header.is_empty() {
        lines.push(output.header.clone());
    }

    append_section_key_values(&mut lines, "Summary", &output.summary);
    append_section(&mut lines, "Details", &output.details);
    append_section(&mut lines, "Warnings", &output.warnings);
    append_section(&mut lines, "Next steps", &output.next_steps);

    lines.join("\n")
}

fn append_section_key_values(lines: &mut Vec<String>, title: &str, items: &[(String, String)]) {
    if items.is_empty() {
        return;
    }

    if !lines.is_empty() {
        lines.push(String::new());
    }

    lines.push(format!("{title}:"));
    for (key, value) in items {
        lines.push(format!("- {key}: {value}"));
    }
}

fn append_section(lines: &mut Vec<String>, title: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }

    if !lines.is_empty() {
        lines.push(String::new());
    }

    lines.push(format!("{title}:"));
    for item in items {
        lines.push(format!("- {item}"));
    }
}

fn kind_for_code(code: i32) -> &'static str {
    match code {
        2 => "user_error",
        3 => "policy_blocked",
        4 => "operation_failed",
        _ => "operation_failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_human_sections() {
        let mut output = HumanOutput::new("sv init: initialized repo");
        output.push_summary("repo", "/tmp/repo");
        output.push_detail("created: .sv.toml");
        output.push_warning("missing actor");
        output.push_next_step("sv actor set <name>");

        let formatted = format_human(&output);
        assert!(formatted.contains("sv init: initialized repo"));
        assert!(formatted.contains("Summary:"));
        assert!(formatted.contains("- repo: /tmp/repo"));
        assert!(formatted.contains("Details:"));
        assert!(formatted.contains("Warnings:"));
        assert!(formatted.contains("Next steps:"));
    }

    #[test]
    fn json_envelope_serializes() {
        #[derive(Serialize)]
        struct Dummy {
            ok: bool,
        }

        let payload = JsonEnvelope {
            schema_version: SCHEMA_VERSION.to_string(),
            command: "status".to_string(),
            ok: true,
            result: Some(Dummy { ok: true }),
            error: None,
        };

        let text = serde_json::to_string(&payload).expect("serialize");
        assert!(text.contains("\"schema_version\""));
        assert!(text.contains("\"command\""));
        assert!(text.contains("\"result\""));
    }
}
