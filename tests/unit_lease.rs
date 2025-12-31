use sv::lease::{Lease, LeaseIntent, LeaseStrength};

#[test]
fn lease_strength_compatibility_matrix() {
    use LeaseStrength::*;

    // Observe overlaps with anything.
    for other in [Observe, Cooperative, Strong, Exclusive] {
        assert!(Observe.is_compatible_with(&other, false));
        assert!(other.is_compatible_with(&Observe, false));
    }

    // Cooperative overlaps with cooperative.
    assert!(Cooperative.is_compatible_with(&Cooperative, false));

    // Strong vs cooperative depends on allow_overlap.
    assert!(!Strong.is_compatible_with(&Cooperative, false));
    assert!(!Cooperative.is_compatible_with(&Strong, false));
    assert!(Strong.is_compatible_with(&Cooperative, true));
    assert!(Cooperative.is_compatible_with(&Strong, true));

    // Strong blocks strong.
    assert!(!Strong.is_compatible_with(&Strong, false));
    assert!(!Strong.is_compatible_with(&Strong, true));

    // Exclusive blocks everything except observe (handled above).
    assert!(!Exclusive.is_compatible_with(&Cooperative, false));
    assert!(!Exclusive.is_compatible_with(&Strong, false));
    assert!(!Exclusive.is_compatible_with(&Exclusive, false));
}

#[test]
fn lease_strength_note_and_priority() {
    use LeaseStrength::*;

    assert!(!Observe.requires_note());
    assert!(!Cooperative.requires_note());
    assert!(Strong.requires_note());
    assert!(Exclusive.requires_note());

    assert_eq!(Observe.priority(), 0);
    assert_eq!(Cooperative.priority(), 1);
    assert_eq!(Strong.priority(), 2);
    assert_eq!(Exclusive.priority(), 3);
}

#[test]
fn lease_intent_conflict_risk_is_ordered() {
    assert!(LeaseIntent::Docs.conflict_risk() < LeaseIntent::Feature.conflict_risk());
    assert!(LeaseIntent::Feature.conflict_risk() < LeaseIntent::Refactor.conflict_risk());
    assert_eq!(LeaseIntent::Format.conflict_risk(), 5);
    assert_eq!(LeaseIntent::Rename.conflict_risk(), 5);
}

#[test]
fn lease_pathspec_overlap_checks_globs_and_prefixes() {
    let lease = Lease::builder("src/auth/**")
        .build()
        .expect("lease");

    assert!(lease.pathspec_overlaps("src/auth/login.rs"));
    assert!(lease.pathspec_overlaps("src/auth/**"));
    assert!(lease.pathspec_overlaps("src/**"));
    assert!(!lease.pathspec_overlaps("src/other/**"));

    let specific = Lease::builder("src/auth/login.rs")
        .build()
        .expect("lease");
    assert!(specific.pathspec_overlaps("src/auth/*.rs"));
    assert!(!specific.pathspec_overlaps("tests/**"));
}
