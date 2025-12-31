use std::collections::HashSet;

use sv::selector::{
    evaluate_selector, parse_selector, EntityKind, Predicate, SelectorContext, SelectorItem,
    SelectorMatch,
};

fn ids(matches: Vec<SelectorMatch>) -> HashSet<String> {
    matches
        .into_iter()
        .map(|m| format!("{:?}:{}", m.kind, m.item.id))
        .collect()
}

fn matcher(_kind: EntityKind, item: &SelectorItem, predicate: &Predicate) -> bool {
    let name = &item.name;
    match predicate {
        Predicate::Active => name.contains("active"),
        Predicate::Stale => name.contains("stale"),
        Predicate::Blocked => name.contains("blocked"),
        Predicate::Ahead(value) => name.contains(value),
        Predicate::Touching(value) => name.contains(value),
        Predicate::Overlaps(value) => name.contains(value),
        Predicate::NameMatches(_) => false,
    }
}

#[test]
fn evaluates_entity_predicates() {
    let workspaces = vec![
        SelectorItem::new("ws1", "alpha-active"),
        SelectorItem::new("ws2", "beta-stale"),
    ];

    let ctx = SelectorContext::new(&workspaces, &[], &[], matcher);
    let expr = parse_selector("ws(active)").unwrap();
    let result = ids(evaluate_selector(&expr, &ctx));

    assert!(result.contains("Workspace:ws1"));
    assert!(!result.contains("Workspace:ws2"));
}

#[test]
fn evaluates_union_and_difference() {
    let workspaces = vec![SelectorItem::new("ws1", "alpha-active")];
    let leases = vec![SelectorItem::new("lease1", "lease-active")];

    let ctx = SelectorContext::new(&workspaces, &leases, &[], matcher);
    let expr = parse_selector("ws(active) | lease(active)").unwrap();
    let result = ids(evaluate_selector(&expr, &ctx));

    assert!(result.contains("Workspace:ws1"));
    assert!(result.contains("Lease:lease1"));

    let expr = parse_selector("ws(active) ~ ws(name~\"alpha\")").unwrap();
    let result = ids(evaluate_selector(&expr, &ctx));
    assert!(result.is_empty());
}

#[test]
fn evaluates_bare_predicate() {
    let workspaces = vec![SelectorItem::new("ws1", "alpha-active")];
    let leases = vec![SelectorItem::new("lease1", "lease-stale")];
    let branches = vec![SelectorItem::new("main", "main-active")];

    let ctx = SelectorContext::new(&workspaces, &leases, &branches, matcher);
    let expr = parse_selector("active").unwrap();
    let result = ids(evaluate_selector(&expr, &ctx));

    assert!(result.contains("Workspace:ws1"));
    assert!(result.contains("Branch:main"));
    assert!(!result.contains("Lease:lease1"));
}
