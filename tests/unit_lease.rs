use sv::lease::{Lease, LeaseStore, LeaseStrength};

#[test]
fn lease_matches_path_with_exact_and_glob() {
    let exact = Lease::builder("README.md").build().expect("lease");
    assert!(exact.matches_path("README.md"));
    assert!(!exact.matches_path("src/main.rs"));

    let glob = Lease::builder("src/**").build().expect("lease");
    assert!(glob.matches_path("src/main.rs"));
    assert!(glob.matches_path("src/nested/mod.rs"));
    assert!(!glob.matches_path("docs/readme.md"));
}

#[test]
fn pathspec_overlaps_is_symmetric_for_prefixes() {
    let lease = Lease::builder("src/**").build().expect("lease");
    assert!(lease.pathspec_overlaps("src/lib.rs"));
    assert!(!lease.pathspec_overlaps("docs/**"));

    let other = Lease::builder("src/lib.rs").build().expect("lease");
    assert!(other.pathspec_overlaps("src/**"));
}

#[test]
fn lease_store_conflicts_respect_actor_and_overlap_policy() {
    let mut store = LeaseStore::new();
    let existing = Lease::builder("src/**")
        .strength(LeaseStrength::Strong)
        .actor("alice")
        .note("work in progress")
        .build()
        .expect("lease");
    store.add(existing);

    let conflicts =
        store.check_conflicts("src/lib.rs", LeaseStrength::Cooperative, Some("bob"), false);
    assert_eq!(conflicts.len(), 1);

    let allow_overlap =
        store.check_conflicts("src/lib.rs", LeaseStrength::Cooperative, Some("bob"), true);
    assert!(allow_overlap.is_empty());

    let same_actor =
        store.check_conflicts("src/lib.rs", LeaseStrength::Strong, Some("alice"), false);
    assert!(same_actor.is_empty());

    let ownerless = store.check_conflicts("src/lib.rs", LeaseStrength::Strong, None, false);
    assert!(ownerless.is_empty());
}
