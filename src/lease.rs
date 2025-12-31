//! Lease system for sv
//!
//! Leases are graded reservations over paths, with intent and description.
//! They provide coordination signals, arbitration mechanisms, and inputs
//! for risk assessment and commit-time conflict checking.
//!
//! # Lease Strengths
//!
//! - `observe`: Watch mode, overlaps with anything
//! - `cooperative`: Normal work, overlaps with observe and cooperative
//! - `strong`: Serious intent, blocks strong/exclusive, cooperative only with --allow-overlap
//! - `exclusive`: Full lock, blocks all except observe (if configured)
//!
//! # Storage
//!
//! Leases are stored in `.git/sv/leases.jsonl` as append-only records.
//! Each record has a status field to track active/released/expired state.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};

// =============================================================================
// Lease Strength
// =============================================================================

/// Strength level of a lease, determining overlap rules
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LeaseStrength {
    /// Watch mode - overlaps with anything, just observing
    Observe,
    /// Normal collaborative work - overlaps with observe and cooperative
    Cooperative,
    /// Serious intent - blocks strong/exclusive, cooperative with flag
    Strong,
    /// Full exclusive lock - blocks all except observe (if configured)
    Exclusive,
}

impl LeaseStrength {
    /// Check if this strength is compatible with another for overlapping paths
    ///
    /// Returns `true` if leases with these strengths can coexist on overlapping paths.
    /// Note: This is the default compatibility; policy can override via config.
    pub fn is_compatible_with(&self, other: &LeaseStrength, allow_overlap: bool) -> bool {
        use LeaseStrength::*;
        
        match (self, other) {
            // Observe overlaps with anything
            (Observe, _) | (_, Observe) => true,
            
            // Cooperative overlaps with observe and cooperative
            (Cooperative, Cooperative) => true,
            
            // Strong overlaps with observe; cooperative only with allow_overlap flag
            (Strong, Cooperative) | (Cooperative, Strong) => allow_overlap,
            
            // Strong blocks strong
            (Strong, Strong) => false,
            
            // Exclusive blocks everything except observe (handled above)
            (Exclusive, _) | (_, Exclusive) => false,
        }
    }
    
    /// Check if this strength requires a note
    pub fn requires_note(&self) -> bool {
        matches!(self, LeaseStrength::Strong | LeaseStrength::Exclusive)
    }
    
    /// Get the priority level (higher = stronger)
    pub fn priority(&self) -> u8 {
        match self {
            LeaseStrength::Observe => 0,
            LeaseStrength::Cooperative => 1,
            LeaseStrength::Strong => 2,
            LeaseStrength::Exclusive => 3,
        }
    }
}

impl fmt::Display for LeaseStrength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LeaseStrength::Observe => write!(f, "observe"),
            LeaseStrength::Cooperative => write!(f, "cooperative"),
            LeaseStrength::Strong => write!(f, "strong"),
            LeaseStrength::Exclusive => write!(f, "exclusive"),
        }
    }
}

impl FromStr for LeaseStrength {
    type Err = Error;
    
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "observe" => Ok(LeaseStrength::Observe),
            "cooperative" => Ok(LeaseStrength::Cooperative),
            "strong" => Ok(LeaseStrength::Strong),
            "exclusive" => Ok(LeaseStrength::Exclusive),
            _ => Err(Error::InvalidArgument(format!(
                "Invalid lease strength '{}'. Expected: observe, cooperative, strong, exclusive",
                s
            ))),
        }
    }
}

impl Default for LeaseStrength {
    fn default() -> Self {
        LeaseStrength::Cooperative
    }
}

// =============================================================================
// Lease Intent
// =============================================================================

/// Intent of the lease - what kind of work is being done
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LeaseIntent {
    /// Bug fix
    Bugfix,
    /// New feature
    Feature,
    /// Documentation
    Docs,
    /// Code refactoring
    Refactor,
    /// Renaming (files, symbols)
    Rename,
    /// Code formatting
    Format,
    /// Mechanical/automated changes
    Mechanical,
    /// Investigation/exploration
    Investigation,
    /// Other/unspecified
    Other,
}

impl LeaseIntent {
    /// Get the conflict risk level for this intent
    ///
    /// Higher values indicate more likely to cause merge conflicts.
    /// Format and rename are high risk because they touch many lines.
    pub fn conflict_risk(&self) -> u8 {
        match self {
            LeaseIntent::Docs => 1,
            LeaseIntent::Investigation => 1,
            LeaseIntent::Bugfix => 2,
            LeaseIntent::Feature => 3,
            LeaseIntent::Refactor => 4,
            LeaseIntent::Mechanical => 4,
            LeaseIntent::Format => 5,
            LeaseIntent::Rename => 5,
            LeaseIntent::Other => 3,
        }
    }
}

impl fmt::Display for LeaseIntent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LeaseIntent::Bugfix => write!(f, "bugfix"),
            LeaseIntent::Feature => write!(f, "feature"),
            LeaseIntent::Docs => write!(f, "docs"),
            LeaseIntent::Refactor => write!(f, "refactor"),
            LeaseIntent::Rename => write!(f, "rename"),
            LeaseIntent::Format => write!(f, "format"),
            LeaseIntent::Mechanical => write!(f, "mechanical"),
            LeaseIntent::Investigation => write!(f, "investigation"),
            LeaseIntent::Other => write!(f, "other"),
        }
    }
}

impl FromStr for LeaseIntent {
    type Err = Error;
    
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "bugfix" | "bug" | "fix" => Ok(LeaseIntent::Bugfix),
            "feature" | "feat" => Ok(LeaseIntent::Feature),
            "docs" | "doc" | "documentation" => Ok(LeaseIntent::Docs),
            "refactor" => Ok(LeaseIntent::Refactor),
            "rename" => Ok(LeaseIntent::Rename),
            "format" | "fmt" => Ok(LeaseIntent::Format),
            "mechanical" | "mech" | "auto" => Ok(LeaseIntent::Mechanical),
            "investigation" | "investigate" | "explore" => Ok(LeaseIntent::Investigation),
            "other" => Ok(LeaseIntent::Other),
            _ => Err(Error::InvalidArgument(format!(
                "Invalid lease intent '{}'. Expected: bugfix, feature, docs, refactor, rename, format, mechanical, investigation, other",
                s
            ))),
        }
    }
}

impl Default for LeaseIntent {
    fn default() -> Self {
        LeaseIntent::Other
    }
}

// =============================================================================
// Lease Scope
// =============================================================================

/// Scope of the lease - where it applies
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum LeaseScope {
    /// Repository-wide (default)
    #[serde(rename = "repo")]
    Repo,
    /// Specific branch only
    #[serde(rename = "branch")]
    Branch(String),
    /// Specific workspace only
    #[serde(rename = "workspace")]
    Workspace(String),
}

impl fmt::Display for LeaseScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LeaseScope::Repo => write!(f, "repo"),
            LeaseScope::Branch(name) => write!(f, "branch:{}", name),
            LeaseScope::Workspace(name) => write!(f, "ws:{}", name),
        }
    }
}

impl FromStr for LeaseScope {
    type Err = Error;
    
    fn from_str(s: &str) -> Result<Self> {
        if s == "repo" {
            Ok(LeaseScope::Repo)
        } else if let Some(name) = s.strip_prefix("branch:") {
            Ok(LeaseScope::Branch(name.to_string()))
        } else if let Some(name) = s.strip_prefix("ws:") {
            Ok(LeaseScope::Workspace(name.to_string()))
        } else if let Some(name) = s.strip_prefix("workspace:") {
            Ok(LeaseScope::Workspace(name.to_string()))
        } else {
            Err(Error::InvalidArgument(format!(
                "Invalid lease scope '{}'. Expected: repo, branch:<name>, ws:<name>",
                s
            )))
        }
    }
}

impl Default for LeaseScope {
    fn default() -> Self {
        LeaseScope::Repo
    }
}

// =============================================================================
// Lease Status
// =============================================================================

/// Current status of a lease
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LeaseStatus {
    /// Lease is active
    Active,
    /// Lease was explicitly released
    Released,
    /// Lease expired due to TTL
    Expired,
    /// Lease was broken by another actor
    Broken,
}

impl Default for LeaseStatus {
    fn default() -> Self {
        LeaseStatus::Active
    }
}

// =============================================================================
// Lease Hints (optional)
// =============================================================================

/// Optional hints for more granular lease information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LeaseHints {
    /// Specific symbols (functions, classes) being modified
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symbols: Vec<String>,
    
    /// Line ranges being modified (start, end)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<(u32, u32)>,
    
    /// Free-form hints
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freeform: Option<String>,
}

impl LeaseHints {
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty() && self.lines.is_empty() && self.freeform.is_none()
    }
}

// =============================================================================
// Main Lease Structure
// =============================================================================

/// A lease reservation over a path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lease {
    /// Unique identifier
    pub id: Uuid,
    
    /// Path pattern (file, directory, or glob)
    pub pathspec: String,
    
    /// Strength of the lease
    pub strength: LeaseStrength,
    
    /// Intent of the work
    pub intent: LeaseIntent,
    
    /// Actor holding the lease (None for ownerless/shared leases)
    pub actor: Option<String>,
    
    /// Scope of the lease
    pub scope: LeaseScope,
    
    /// Explanatory note (required for strong/exclusive)
    pub note: Option<String>,
    
    /// Time-to-live duration string (e.g., "2h", "30m")
    pub ttl: String,
    
    /// Expiration timestamp
    pub expires_at: DateTime<Utc>,
    
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    
    /// Current status
    pub status: LeaseStatus,
    
    /// Optional detailed hints
    #[serde(default, skip_serializing_if = "LeaseHints::is_empty")]
    pub hints: LeaseHints,
    
    /// Timestamp when status changed (for released/expired/broken)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_changed_at: Option<DateTime<Utc>>,
    
    /// Reason for status change (for broken leases)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
}

impl Lease {
    /// Create a new lease builder
    pub fn builder(pathspec: impl Into<String>) -> LeaseBuilder {
        LeaseBuilder::new(pathspec)
    }
    
    /// Check if this lease is currently active (not expired, not released)
    pub fn is_active(&self) -> bool {
        self.status == LeaseStatus::Active && Utc::now() < self.expires_at
    }
    
    /// Check if this lease has expired based on TTL
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }
    
    /// Check if this lease overlaps with a given path
    ///
    /// Uses glob matching: the lease pathspec is treated as a glob pattern.
    pub fn matches_path(&self, path: &str) -> bool {
        // Try exact match first
        if self.pathspec == path {
            return true;
        }
        
        // Try glob match
        if let Ok(pattern) = glob::Pattern::new(&self.pathspec) {
            return pattern.matches(path);
        }
        
        // Try prefix match for directories
        if self.pathspec.ends_with('/') || self.pathspec.ends_with("/**") {
            let prefix = self.pathspec.trim_end_matches("/**").trim_end_matches('/');
            return path.starts_with(prefix);
        }
        
        false
    }
    
    /// Check if this lease's pathspec overlaps with another pathspec
    ///
    /// This is symmetric: checks if either pattern could match paths matched by the other.
    pub fn pathspec_overlaps(&self, other_pathspec: &str) -> bool {
        // Direct match
        if self.pathspec == other_pathspec {
            return true;
        }
        
        // Check if self matches other as a path
        if self.matches_path(other_pathspec) {
            return true;
        }
        
        // Check if other matches self as a path (create temp lease for convenience)
        let other_as_pattern = glob::Pattern::new(other_pathspec).ok();
        if let Some(pattern) = other_as_pattern {
            if pattern.matches(&self.pathspec) {
                return true;
            }
        }
        
        // Check prefix overlaps for directory patterns
        let self_prefix = self.pathspec.trim_end_matches("/**").trim_end_matches('/');
        let other_prefix = other_pathspec.trim_end_matches("/**").trim_end_matches('/');
        
        self_prefix.starts_with(other_prefix) || other_prefix.starts_with(self_prefix)
    }
    
    /// Mark this lease as released
    pub fn release(&mut self) {
        self.status = LeaseStatus::Released;
        self.status_changed_at = Some(Utc::now());
    }
    
    /// Mark this lease as broken with a reason
    pub fn break_lease(&mut self, reason: impl Into<String>) {
        self.status = LeaseStatus::Broken;
        self.status_changed_at = Some(Utc::now());
        self.status_reason = Some(reason.into());
    }
    
    /// Renew the lease with a new TTL
    pub fn renew(&mut self, ttl: impl Into<String>) -> Result<()> {
        let ttl_str = ttl.into();
        let duration = parse_duration(&ttl_str)?;
        self.ttl = ttl_str;
        self.expires_at = Utc::now() + duration;
        Ok(())
    }
    
    /// Validate the lease
    pub fn validate(&self) -> Result<()> {
        self.validate_with_note_requirement(true)
    }

    /// Validate the lease with configurable note requirements
    pub fn validate_with_note_requirement(&self, require_note: bool) -> Result<()> {
        if require_note && self.strength.requires_note() && self.note.is_none() {
            return Err(Error::NoteRequired(self.strength.to_string()));
        }

        // Validate pathspec is not empty
        if self.pathspec.trim().is_empty() {
            return Err(Error::InvalidArgument("Pathspec cannot be empty".to_string()));
        }

        Ok(())
    }
}

// =============================================================================
// Lease Builder
// =============================================================================

/// Builder for creating leases with fluent API
pub struct LeaseBuilder {
    pathspec: String,
    strength: LeaseStrength,
    intent: LeaseIntent,
    actor: Option<String>,
    scope: LeaseScope,
    note: Option<String>,
    require_note: bool,
    ttl: String,
    hints: LeaseHints,
}

impl LeaseBuilder {
    /// Create a new lease builder for the given pathspec
    pub fn new(pathspec: impl Into<String>) -> Self {
        Self {
            pathspec: pathspec.into(),
            strength: LeaseStrength::default(),
            intent: LeaseIntent::default(),
            actor: None,
            scope: LeaseScope::default(),
            note: None,
            require_note: true,
            ttl: "2h".to_string(),
            hints: LeaseHints::default(),
        }
    }
    
    /// Set the lease strength
    pub fn strength(mut self, strength: LeaseStrength) -> Self {
        self.strength = strength;
        self
    }
    
    /// Set the lease intent
    pub fn intent(mut self, intent: LeaseIntent) -> Self {
        self.intent = intent;
        self
    }
    
    /// Set the actor (owner) of the lease
    pub fn actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = Some(actor.into());
        self
    }
    
    /// Set the lease scope
    pub fn scope(mut self, scope: LeaseScope) -> Self {
        self.scope = scope;
        self
    }
    
    /// Set the explanatory note
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// Configure whether a note is required for strong/exclusive leases
    pub fn require_note(mut self, require_note: bool) -> Self {
        self.require_note = require_note;
        self
    }
    
    /// Set the TTL
    pub fn ttl(mut self, ttl: impl Into<String>) -> Self {
        self.ttl = ttl.into();
        self
    }
    
    /// Add symbol hints
    pub fn symbols(mut self, symbols: Vec<String>) -> Self {
        self.hints.symbols = symbols;
        self
    }
    
    /// Add line range hints
    pub fn lines(mut self, lines: Vec<(u32, u32)>) -> Self {
        self.hints.lines = lines;
        self
    }
    
    /// Build the lease
    pub fn build(self) -> Result<Lease> {
        let duration = parse_duration(&self.ttl)?;
        let now = Utc::now();
        
        let lease = Lease {
            id: Uuid::new_v4(),
            pathspec: self.pathspec,
            strength: self.strength,
            intent: self.intent,
            actor: self.actor,
            scope: self.scope,
            note: self.note,
            ttl: self.ttl,
            expires_at: now + duration,
            created_at: now,
            status: LeaseStatus::Active,
            hints: self.hints,
            status_changed_at: None,
            status_reason: None,
        };
        
        lease.validate_with_note_requirement(self.require_note)?;
        Ok(lease)
    }
}

// =============================================================================
// Duration Parsing
// =============================================================================

/// Parse a duration string like "2h", "30m", "1d"
pub fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim();
    
    if s.is_empty() {
        return Err(Error::InvalidArgument("Duration cannot be empty".to_string()));
    }
    
    // Find where the number ends and unit begins
    let (num_str, unit) = if let Some(pos) = s.find(|c: char| !c.is_ascii_digit()) {
        (&s[..pos], &s[pos..])
    } else {
        // Assume minutes if no unit
        (s, "m")
    };
    
    let num: i64 = num_str.parse().map_err(|_| {
        Error::InvalidArgument(format!("Invalid duration number: {}", num_str))
    })?;
    
    let duration = match unit.to_lowercase().as_str() {
        "s" | "sec" | "second" | "seconds" => Duration::seconds(num),
        "m" | "min" | "minute" | "minutes" => Duration::minutes(num),
        "h" | "hr" | "hour" | "hours" => Duration::hours(num),
        "d" | "day" | "days" => Duration::days(num),
        "w" | "week" | "weeks" => Duration::weeks(num),
        _ => {
            return Err(Error::InvalidArgument(format!(
                "Invalid duration unit '{}'. Expected: s, m, h, d, w",
                unit
            )));
        }
    };
    
    Ok(duration)
}

// =============================================================================
// Lease Store (for managing multiple leases)
// =============================================================================

/// A collection of leases with query capabilities
#[derive(Debug, Clone, Default)]
pub struct LeaseStore {
    leases: Vec<Lease>,
}

impl LeaseStore {
    /// Create a new empty lease store
    pub fn new() -> Self {
        Self { leases: Vec::new() }
    }
    
    /// Create a lease store from a vector of leases
    pub fn from_vec(leases: Vec<Lease>) -> Self {
        Self { leases }
    }
    
    /// Get all leases
    pub fn all(&self) -> &[Lease] {
        &self.leases
    }
    
    /// Get all active leases (not expired, not released)
    pub fn active(&self) -> impl Iterator<Item = &Lease> {
        self.leases.iter().filter(|l| l.is_active())
    }
    
    /// Find a lease by ID
    pub fn find(&self, id: &Uuid) -> Option<&Lease> {
        self.leases.iter().find(|l| l.id == *id)
    }
    
    /// Find a lease by ID (mutable)
    pub fn find_mut(&mut self, id: &Uuid) -> Option<&mut Lease> {
        self.leases.iter_mut().find(|l| l.id == *id)
    }
    
    /// Find all leases that overlap with a path
    pub fn overlapping_path<'a>(&'a self, path: &'a str) -> impl Iterator<Item = &'a Lease> {
        self.leases.iter().filter(move |l| l.is_active() && l.matches_path(path))
    }
    
    /// Find all leases held by an actor
    pub fn by_actor(&self, actor: &str) -> impl Iterator<Item = &Lease> {
        let actor = actor.to_string();
        self.leases.iter().filter(move |l| l.actor.as_ref() == Some(&actor))
    }
    
    /// Add a lease to the store
    pub fn add(&mut self, lease: Lease) {
        self.leases.push(lease);
    }
    
    /// Check if a new lease would conflict with existing leases
    pub fn check_conflicts(
        &self,
        pathspec: &str,
        strength: LeaseStrength,
        actor: Option<&str>,
        allow_overlap: bool,
    ) -> Vec<&Lease> {
        self.active()
            .filter(|existing| {
                // Skip own leases
                if let (Some(new_actor), Some(existing_actor)) = (actor, &existing.actor) {
                    if new_actor == existing_actor {
                        return false;
                    }
                }
                
                // Check pathspec overlap
                if !existing.pathspec_overlaps(pathspec) {
                    return false;
                }
                
                // Check strength compatibility
                !strength.is_compatible_with(&existing.strength, allow_overlap)
            })
            .collect()
    }
    
    /// Mark expired leases
    pub fn expire_stale(&mut self) {
        let _ = self.expire_stale_collect_at(Utc::now());
    }

    /// Mark expired leases using a provided timestamp (useful for tests)
    pub fn expire_stale_at(&mut self, now: DateTime<Utc>) {
        let _ = self.expire_stale_collect_at(now);
    }

    /// Mark expired leases and return those newly expired
    pub fn expire_stale_collect(&mut self) -> Vec<Lease> {
        self.expire_stale_collect_at(Utc::now())
    }

    /// Mark expired leases at a given timestamp and return newly expired
    pub fn expire_stale_collect_at(&mut self, now: DateTime<Utc>) -> Vec<Lease> {
        let mut expired = Vec::new();

        for lease in &mut self.leases {
            if lease.status == LeaseStatus::Active && lease.expires_at <= now {
                lease.status = LeaseStatus::Expired;
                lease.status_changed_at = Some(now);
                expired.push(lease.clone());
            }
        }

        expired
    }

    /// Remove expired leases after a grace period and return them for archival
    pub fn cleanup_expired(&mut self, grace: Duration) -> Vec<Lease> {
        self.cleanup_expired_at(grace, Utc::now())
    }

    /// Remove expired leases after a grace period using a provided timestamp
    pub fn cleanup_expired_at(&mut self, grace: Duration, now: DateTime<Utc>) -> Vec<Lease> {
        let mut expired = Vec::new();

        self.leases.retain(|lease| {
            let ready = lease.status == LeaseStatus::Expired
                && lease.expires_at + grace <= now;
            if ready {
                expired.push(lease.clone());
                false
            } else {
                true
            }
        });

        expired
    }
    
    /// Convert to vector (consumes the store)
    pub fn into_vec(self) -> Vec<Lease> {
        self.leases
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_strength_compatibility() {
        use LeaseStrength::*;
        
        // Observe is compatible with everything
        assert!(Observe.is_compatible_with(&Observe, false));
        assert!(Observe.is_compatible_with(&Cooperative, false));
        assert!(Observe.is_compatible_with(&Strong, false));
        assert!(Observe.is_compatible_with(&Exclusive, false));
        
        // Cooperative is compatible with observe and cooperative
        assert!(Cooperative.is_compatible_with(&Observe, false));
        assert!(Cooperative.is_compatible_with(&Cooperative, false));
        assert!(!Cooperative.is_compatible_with(&Strong, false));
        assert!(Cooperative.is_compatible_with(&Strong, true)); // with allow_overlap
        assert!(!Cooperative.is_compatible_with(&Exclusive, false));
        
        // Strong blocks strong and exclusive
        assert!(Strong.is_compatible_with(&Observe, false));
        assert!(!Strong.is_compatible_with(&Cooperative, false));
        assert!(Strong.is_compatible_with(&Cooperative, true));
        assert!(!Strong.is_compatible_with(&Strong, false));
        assert!(!Strong.is_compatible_with(&Exclusive, false));
        
        // Exclusive blocks everything except observe
        assert!(Exclusive.is_compatible_with(&Observe, false));
        assert!(!Exclusive.is_compatible_with(&Cooperative, false));
        assert!(!Exclusive.is_compatible_with(&Strong, false));
        assert!(!Exclusive.is_compatible_with(&Exclusive, false));
    }
    
    #[test]
    fn test_strength_parse() {
        assert_eq!(LeaseStrength::from_str("observe").unwrap(), LeaseStrength::Observe);
        assert_eq!(LeaseStrength::from_str("COOPERATIVE").unwrap(), LeaseStrength::Cooperative);
        assert_eq!(LeaseStrength::from_str("Strong").unwrap(), LeaseStrength::Strong);
        assert_eq!(LeaseStrength::from_str("exclusive").unwrap(), LeaseStrength::Exclusive);
        assert!(LeaseStrength::from_str("invalid").is_err());
    }
    
    #[test]
    fn test_intent_parse() {
        assert_eq!(LeaseIntent::from_str("bugfix").unwrap(), LeaseIntent::Bugfix);
        assert_eq!(LeaseIntent::from_str("bug").unwrap(), LeaseIntent::Bugfix);
        assert_eq!(LeaseIntent::from_str("feature").unwrap(), LeaseIntent::Feature);
        assert_eq!(LeaseIntent::from_str("docs").unwrap(), LeaseIntent::Docs);
        assert!(LeaseIntent::from_str("invalid").is_err());
    }
    
    #[test]
    fn test_scope_parse() {
        assert_eq!(LeaseScope::from_str("repo").unwrap(), LeaseScope::Repo);
        assert_eq!(
            LeaseScope::from_str("branch:main").unwrap(),
            LeaseScope::Branch("main".to_string())
        );
        assert_eq!(
            LeaseScope::from_str("ws:agent1").unwrap(),
            LeaseScope::Workspace("agent1".to_string())
        );
        assert!(LeaseScope::from_str("invalid").is_err());
    }
    
    #[test]
    fn test_duration_parse() {
        assert_eq!(parse_duration("2h").unwrap(), Duration::hours(2));
        assert_eq!(parse_duration("30m").unwrap(), Duration::minutes(30));
        assert_eq!(parse_duration("1d").unwrap(), Duration::days(1));
        assert_eq!(parse_duration("60s").unwrap(), Duration::seconds(60));
        assert_eq!(parse_duration("2w").unwrap(), Duration::weeks(2));
        assert!(parse_duration("invalid").is_err());
    }
    
    #[test]
    fn test_lease_builder() {
        let lease = Lease::builder("src/auth/**")
            .strength(LeaseStrength::Strong)
            .intent(LeaseIntent::Bugfix)
            .actor("agent1")
            .note("Fixing auth bug")
            .ttl("4h")
            .build()
            .unwrap();
        
        assert_eq!(lease.pathspec, "src/auth/**");
        assert_eq!(lease.strength, LeaseStrength::Strong);
        assert_eq!(lease.intent, LeaseIntent::Bugfix);
        assert_eq!(lease.actor, Some("agent1".to_string()));
        assert!(lease.is_active());
    }
    
    #[test]
    fn test_lease_note_required() {
        // Strong without note should fail
        let result = Lease::builder("src/**")
            .strength(LeaseStrength::Strong)
            .build();
        assert!(result.is_err());
        
        // Strong with note should succeed
        let result = Lease::builder("src/**")
            .strength(LeaseStrength::Strong)
            .note("Important work")
            .build();
        assert!(result.is_ok());
        
        // Cooperative without note should succeed
        let result = Lease::builder("src/**")
            .strength(LeaseStrength::Cooperative)
            .build();
        assert!(result.is_ok());
    }

    #[test]
    fn test_lease_note_optional_when_disabled() {
        let result = Lease::builder("src/**")
            .strength(LeaseStrength::Strong)
            .require_note(false)
            .build();
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_lease_path_matching() {
        let lease = Lease::builder("src/auth/**")
            .build()
            .unwrap();
        
        assert!(lease.matches_path("src/auth/login.rs"));
        assert!(lease.matches_path("src/auth/nested/deep.rs"));
        assert!(!lease.matches_path("src/other/file.rs"));
        assert!(!lease.matches_path("tests/auth.rs"));
    }
    
    #[test]
    fn test_lease_serialization() {
        let lease = Lease::builder("src/**")
            .strength(LeaseStrength::Cooperative)
            .intent(LeaseIntent::Feature)
            .actor("agent1")
            .build()
            .unwrap();
        
        let json = serde_json::to_string(&lease).unwrap();
        let parsed: Lease = serde_json::from_str(&json).unwrap();
        
        assert_eq!(lease.id, parsed.id);
        assert_eq!(lease.pathspec, parsed.pathspec);
        assert_eq!(lease.strength, parsed.strength);
        assert_eq!(lease.intent, parsed.intent);
    }
    
    #[test]
    fn test_lease_store_conflicts() {
        let mut store = LeaseStore::new();
        
        // Add an exclusive lease
        store.add(
            Lease::builder("src/auth/**")
                .strength(LeaseStrength::Exclusive)
                .actor("agent1")
                .note("Critical fix")
                .build()
                .unwrap()
        );
        
        // Check conflicts for different actors
        let conflicts = store.check_conflicts(
            "src/auth/login.rs",
            LeaseStrength::Cooperative,
            Some("agent2"),
            false,
        );
        assert_eq!(conflicts.len(), 1);
        
        // Same actor should not conflict
        let conflicts = store.check_conflicts(
            "src/auth/login.rs",
            LeaseStrength::Cooperative,
            Some("agent1"),
            false,
        );
        assert_eq!(conflicts.len(), 0);
        
        // Non-overlapping path should not conflict
        let conflicts = store.check_conflicts(
            "src/other/file.rs",
            LeaseStrength::Exclusive,
            Some("agent2"),
            false,
        );
        assert_eq!(conflicts.len(), 0);
    }

    #[test]
    fn test_expire_and_cleanup() {
        let mut lease = Lease::builder("src/**").build().unwrap();
        lease.expires_at = Utc::now() - Duration::seconds(10);

        let mut store = LeaseStore::from_vec(vec![lease]);
        let expired = store.expire_stale_collect();

        assert_eq!(expired.len(), 1);
        assert_eq!(store.all()[0].status, LeaseStatus::Expired);

        let expired = store.cleanup_expired(Duration::seconds(0));
        assert_eq!(expired.len(), 1);
        assert!(store.all().is_empty());
    }

    #[test]
    fn test_cleanup_respects_grace_period() {
        let mut lease = Lease::builder("src/**").build().unwrap();
        lease.expires_at = Utc::now() - Duration::seconds(30);

        let now = Utc::now();
        let mut store = LeaseStore::from_vec(vec![lease]);
        store.expire_stale_at(now);

        let expired = store.cleanup_expired_at(Duration::seconds(60), now);
        assert!(expired.is_empty());
        assert_eq!(store.all().len(), 1);

        let later = now + Duration::seconds(61);
        let expired = store.cleanup_expired_at(Duration::seconds(60), later);
        assert_eq!(expired.len(), 1);
        assert!(store.all().is_empty());
    }
}
