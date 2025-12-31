//! sv protect subcommand implementations
//!
//! Provides protect management commands: status, add, off, rm

use std::path::PathBuf;

use crate::config::{Config, ProtectPath};
use crate::error::{Error, Result};
use crate::git;
use crate::output::{emit_success, HumanOutput, OutputOptions};
use crate::protect::{compute_status, load_override};
use crate::storage::Storage;

/// Options for the protect status command
pub struct StatusOptions {
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

/// Options for the protect add command
pub struct AddOptions {
    pub patterns: Vec<String>,
    pub mode: String,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

/// Options for the protect rm command
pub struct RmOptions {
    pub patterns: Vec<String>,
    pub force: bool,
    pub repo: Option<PathBuf>,
    pub json: bool,
    pub quiet: bool,
}

/// Result of protect add command
#[derive(serde::Serialize)]
struct AddReport {
    added: Vec<String>,
    already_exists: Vec<String>,
    invalid: Vec<InvalidPattern>,
}

#[derive(Clone, serde::Serialize)]
struct InvalidPattern {
    pattern: String,
    error: String,
}

/// Result of protect status command
#[derive(serde::Serialize)]
struct StatusReport {
    rules: Vec<RuleInfo>,
    matches: MatchInfo,
}

#[derive(serde::Serialize)]
struct RuleInfo {
    pattern: String,
    mode: String,
    disabled: bool,
}

#[derive(serde::Serialize)]
struct MatchInfo {
    staged: Vec<String>,
    disabled: Vec<String>,
}

/// Result of protect rm command
#[derive(serde::Serialize)]
struct RmReport {
    removed: Vec<String>,
    not_found: Vec<String>,
}

/// Run the protect status command
pub fn run_status(options: StatusOptions) -> Result<()> {
    // Discover repository
    let start = options.repo.clone().unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    });
    
    let repository = git2::Repository::discover(&start)
        .map_err(|_| Error::RepoNotFound(start.clone()))?;
    
    let workdir = repository
        .workdir()
        .ok_or_else(|| Error::NotARepo(start.clone()))?
        .to_path_buf();
    
    // Load config
    let config = Config::load_from_repo(&workdir);
    
    // Resolve common dir for worktree support
    let common_dir = resolve_common_dir(&repository)?;
    let storage = Storage::new(workdir.clone(), common_dir.clone(), workdir.clone());
    
    // Load overrides
    let override_data = load_override(&storage).ok();

    // Determine staged files
    let staged_paths = git::staged_paths(&repository)?;

    // Compute status with staged matches
    let status = compute_status(&config, override_data.as_ref(), &staged_paths)?;
    let disabled_patterns = status.disabled_patterns.clone();
    let disabled_count = disabled_patterns.len();

    let rule_infos: Vec<RuleInfo> = status
        .rules
        .iter()
        .map(|r| RuleInfo {
            pattern: r.rule.pattern.clone(),
            mode: r.rule.mode.clone(),
            disabled: r.disabled,
        })
        .collect();

    let mut staged_matches = Vec::new();
    for rule in &status.rules {
        if rule.disabled {
            continue;
        }
        for path in &rule.matched_files {
            staged_matches.push(path.display().to_string());
        }
    }
    staged_matches.sort();
    staged_matches.dedup();

    let report = StatusReport {
        rules: rule_infos,
        matches: MatchInfo {
            staged: staged_matches.clone(),
            disabled: disabled_patterns.clone(),
        },
    };

    let header = if status.rules.is_empty() {
        "sv protect status: no rules".to_string()
    } else if disabled_count > 0 {
        format!(
            "sv protect status: {} rules ({} disabled)",
            status.rules.len(),
            disabled_count
        )
    } else {
        format!("sv protect status: {} rules", status.rules.len())
    };

    let mut human = HumanOutput::new(header);
    human.push_summary("rules", status.rules.len().to_string());
    human.push_summary("disabled_for_workspace", disabled_count.to_string());

    for rule in &status.rules {
        let mut line = format!("{} ({})", rule.rule.pattern, rule.rule.mode);
        if rule.disabled {
            line.push_str(" [disabled]");
        }
        human.push_detail(line);
    }

    if !staged_matches.is_empty() {
        human.push_warning(format!(
            "staged files match protected patterns: {}",
            staged_matches.join(", ")
        ));
    }

    if status.rules.is_empty() {
        human.push_next_step("sv protect add <pattern>");
    } else {
        human.push_next_step("sv protect off <pattern>");
        human.push_next_step("sv protect rm <pattern>");
    }

    emit_success(
        OutputOptions {
            json: options.json,
            quiet: options.quiet,
        },
        "protect status",
        &report,
        Some(&human),
    )?;
    
    Ok(())
}

/// Run the protect add command
pub fn run_add(options: AddOptions) -> Result<()> {
    // Discover repository
    let start = options.repo.clone().unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    });
    
    let repository = git2::Repository::discover(&start)
        .map_err(|_| Error::RepoNotFound(start.clone()))?;
    
    let workdir = repository
        .workdir()
        .ok_or_else(|| Error::NotARepo(start.clone()))?
        .to_path_buf();
    
    // Load current config
    let config_path = workdir.join(".sv.toml");
    let mut config = if config_path.exists() {
        Config::load(&config_path)?
    } else {
        Config::default()
    };
    
    // Validate mode
    if !["guard", "readonly", "warn"].contains(&options.mode.as_str()) {
        return Err(Error::InvalidArgument(format!(
            "Invalid mode '{}'. Expected: guard, readonly, warn",
            options.mode
        )));
    }
    
    let mut added = Vec::new();
    let mut already_exists = Vec::new();
    let mut invalid = Vec::new();
    
    // Get existing patterns
    let existing_patterns: Vec<String> = config.protect.paths.iter().map(|p| match p {
        ProtectPath::Simple(s) => s.clone(),
        ProtectPath::WithMode { pattern, .. } => pattern.clone(),
    }).collect();
    
    // Process each pattern
    for pattern in &options.patterns {
        // Validate pattern syntax
        if let Err(e) = glob::Pattern::new(pattern) {
            invalid.push(InvalidPattern {
                pattern: pattern.clone(),
                error: e.to_string(),
            });
            continue;
        }
        
        // Check for duplicates
        if existing_patterns.contains(pattern) {
            already_exists.push(pattern.clone());
            continue;
        }
        
        // Add the pattern
        let entry = if options.mode == config.protect.mode {
            // Use simple form if mode matches default
            ProtectPath::Simple(pattern.clone())
        } else {
            // Use explicit mode form
            ProtectPath::WithMode {
                pattern: pattern.clone(),
                mode: options.mode.clone(),
            }
        };
        
        config.protect.paths.push(entry);
        added.push(pattern.clone());
    }
    
    // Save config if we added anything
    if !added.is_empty() {
        config.save(&config_path)?;
    }
    
    let report = AddReport {
        added: added.clone(),
        already_exists: already_exists.clone(),
        invalid: invalid.clone(),
    };
    
    if options.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if !options.quiet {
        if !added.is_empty() {
            println!("Added {} protected pattern(s):", added.len());
            for pattern in &added {
                println!("  {} [{}]", pattern, options.mode);
            }
        }
        
        if !already_exists.is_empty() {
            eprintln!("\nAlready protected ({}):", already_exists.len());
            for pattern in &already_exists {
                eprintln!("  {}", pattern);
            }
        }
        
        if !invalid.is_empty() {
            eprintln!("\nInvalid patterns ({}):", invalid.len());
            for inv in &invalid {
                eprintln!("  {}: {}", inv.pattern, inv.error);
            }
        }
        
        if added.is_empty() && already_exists.is_empty() && invalid.is_empty() {
            println!("No patterns to add.");
        }
    }
    
    // Return error if all patterns were invalid
    if added.is_empty() && !options.patterns.is_empty() {
        if !invalid.is_empty() {
            return Err(Error::InvalidArgument(format!(
                "Invalid pattern: {}", invalid[0].error
            )));
        }
    }
    
    Ok(())
}

/// Run the protect rm command
pub fn run_rm(options: RmOptions) -> Result<()> {
    // Discover repository
    let start = options.repo.clone().unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    });
    
    let repository = git2::Repository::discover(&start)
        .map_err(|_| Error::RepoNotFound(start.clone()))?;
    
    let workdir = repository
        .workdir()
        .ok_or_else(|| Error::NotARepo(start.clone()))?
        .to_path_buf();
    
    // Load current config
    let config_path = workdir.join(".sv.toml");
    if !config_path.exists() {
        if options.force {
            if options.json {
                let report = RmReport {
                    removed: vec![],
                    not_found: options.patterns.clone(),
                };
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else if !options.quiet {
                println!("No .sv.toml config file. Nothing to remove.");
            }
            return Ok(());
        }
        return Err(Error::OperationFailed("No .sv.toml config file.".to_string()));
    }
    
    let mut config = Config::load(&config_path)?;
    
    let mut removed = Vec::new();
    let mut not_found = Vec::new();
    
    for pattern in &options.patterns {
        let initial_len = config.protect.paths.len();
        
        config.protect.paths.retain(|p| {
            let p_pattern = match p {
                ProtectPath::Simple(s) => s,
                ProtectPath::WithMode { pattern, .. } => pattern,
            };
            p_pattern != pattern
        });
        
        if config.protect.paths.len() < initial_len {
            removed.push(pattern.clone());
        } else {
            not_found.push(pattern.clone());
        }
    }
    
    // Save config if we removed anything
    if !removed.is_empty() {
        config.save(&config_path)?;
    }
    
    let report = RmReport {
        removed: removed.clone(),
        not_found: not_found.clone(),
    };
    
    if options.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if !options.quiet {
        if !removed.is_empty() {
            println!("Removed {} protected pattern(s):", removed.len());
            for pattern in &removed {
                println!("  {}", pattern);
            }
        }
        
        if !not_found.is_empty() {
            if options.force {
                eprintln!("\nNot found (ignored):");
            } else {
                eprintln!("\nNot found:");
            }
            for pattern in &not_found {
                eprintln!("  {}", pattern);
            }
        }
    }
    
    // Return error if patterns not found (unless --force)
    if !not_found.is_empty() && !options.force {
        return Err(Error::OperationFailed(format!(
            "Pattern not found: {}", not_found[0]
        )));
    }
    
    Ok(())
}

// =============================================================================
// Helper functions
// =============================================================================

fn resolve_common_dir(repository: &git2::Repository) -> Result<PathBuf> {
    let git_dir = repository.path();
    let commondir_path = git_dir.join("commondir");
    if !commondir_path.exists() {
        return Ok(git_dir.to_path_buf());
    }

    let content = std::fs::read_to_string(&commondir_path)?;
    let rel = content.trim();
    if rel.is_empty() {
        return Err(Error::OperationFailed(format!(
            "commondir file is empty: {}",
            commondir_path.display()
        )));
    }

    Ok(git_dir.join(rel))
}
