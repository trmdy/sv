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
        let next_steps = human
            .map(|h| h.next_steps.clone())
            .unwrap_or_default();

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

pub fn format_human(output: &HumanOutput) -> String {
    let mut lines = Vec::new();
    lines.push(output.header.clone());

    push_summary(&mut lines, &output.summary);
    push_section(&mut lines, "Details", &output.details);
    push_section(&mut lines, "Warnings", &output.warnings);
    push_section(&mut lines, "Next steps", &output.next_steps);

    lines.join("\n")
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
