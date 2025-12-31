//! Shared output formatting for sv CLI commands.

use serde::Serialize;

use crate::error::Result;

pub const SCHEMA_VERSION: &str = "sv.v1";

#[derive(Debug, Clone)]
pub struct Output {
    command: String,
    header: String,
    data: serde_json::Value,
    summary: Vec<(String, String)>,
    details: Vec<String>,
    warnings: Vec<String>,
    next_steps: Vec<String>,
}

impl Output {
    pub fn new<T: Serialize>(
        command: impl Into<String>,
        header: impl Into<String>,
        data: T,
    ) -> Result<Self> {
        Ok(Self {
            command: command.into(),
            header: header.into(),
            data: serde_json::to_value(data)?,
            summary: Vec::new(),
            details: Vec::new(),
            warnings: Vec::new(),
            next_steps: Vec::new(),
        })
    }

    pub fn summary(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.summary.push((key.into(), value.into()));
        self
    }

    pub fn detail(mut self, value: impl Into<String>) -> Self {
        self.details.push(value.into());
        self
    }

    pub fn warning(mut self, value: impl Into<String>) -> Self {
        self.warnings.push(value.into());
        self
    }

    pub fn next_step(mut self, value: impl Into<String>) -> Self {
        self.next_steps.push(value.into());
        self
    }

    pub fn emit(&self, json: bool, quiet: bool) -> Result<()> {
        if json {
            self.emit_json()?;
            return Ok(());
        }

        if quiet {
            return Ok(());
        }

        self.emit_human();
        Ok(())
    }

    fn emit_json(&self) -> Result<()> {
        #[derive(Serialize)]
        struct Envelope<'a> {
            schema_version: &'static str,
            command: &'a str,
            status: &'static str,
            data: &'a serde_json::Value,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            warnings: &'a Vec<String>,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            next_steps: &'a Vec<String>,
        }

        let payload = Envelope {
            schema_version: SCHEMA_VERSION,
            command: &self.command,
            status: "success",
            data: &self.data,
            warnings: &self.warnings,
            next_steps: &self.next_steps,
        };

        println!("{}", serde_json::to_string_pretty(&payload)?);
        Ok(())
    }

    fn emit_human(&self) {
        println!("{}", self.header);

        print_summary(&self.summary);
        print_section("Details", &self.details);
        print_section("Warnings", &self.warnings);
        print_section("Next steps", &self.next_steps);
    }
}

fn print_summary(summary: &[(String, String)]) {
    if summary.is_empty() {
        return;
    }

    println!();
    println!("Summary:");
    for (key, value) in summary {
        if value.is_empty() {
            println!("- {key}");
        } else {
            println!("- {key}: {value}");
        }
    }
}

fn print_section(title: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }

    println!();
    println!("{title}:");
    for item in items {
        println!("- {item}");
    }
}
