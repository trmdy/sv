use sv::selector::{
    parse_selector, EntityKind, EntitySelector, Predicate, SelectorAtom, SelectorExpr,
};

#[test]
fn parses_simple_entity() {
    let expr = parse_selector("ws(active)").expect("parse");
    assert_eq!(
        expr,
        SelectorExpr::Atom(SelectorAtom::Entity(EntitySelector {
            kind: EntityKind::Workspace,
            predicate: Some(Predicate::Active),
        }))
    );
}

#[test]
fn parses_intersection_with_predicate() {
    let expr = parse_selector("ws(active) & ahead(\"main\")").expect("parse");
    assert_eq!(
        expr,
        SelectorExpr::Intersection(
            Box::new(SelectorExpr::Atom(SelectorAtom::Entity(EntitySelector {
                kind: EntityKind::Workspace,
                predicate: Some(Predicate::Active),
            }))),
            Box::new(SelectorExpr::Atom(SelectorAtom::Predicate(
                Predicate::Ahead("main".to_string())
            )))
        )
    );
}

#[test]
fn parses_difference() {
    let expr = parse_selector("ws(active) ~ ws(blocked)").expect("parse");
    assert_eq!(
        expr,
        SelectorExpr::Difference(
            Box::new(SelectorExpr::Atom(SelectorAtom::Entity(EntitySelector {
                kind: EntityKind::Workspace,
                predicate: Some(Predicate::Active),
            }))),
            Box::new(SelectorExpr::Atom(SelectorAtom::Entity(EntitySelector {
                kind: EntityKind::Workspace,
                predicate: Some(Predicate::Blocked),
            })))
        )
    );
}

#[test]
fn parses_name_match() {
    let expr = parse_selector("ws(name~\"agent\")").expect("parse");
    assert_eq!(
        expr,
        SelectorExpr::Atom(SelectorAtom::Entity(EntitySelector {
            kind: EntityKind::Workspace,
            predicate: Some(Predicate::NameMatches("agent".to_string())),
        }))
    );
}

#[test]
fn honors_precedence() {
    let expr = parse_selector("ws(active) | lease(active) & branch(blocked)").expect("parse");
    assert_eq!(
        expr,
        SelectorExpr::Union(
            Box::new(SelectorExpr::Atom(SelectorAtom::Entity(EntitySelector {
                kind: EntityKind::Workspace,
                predicate: Some(Predicate::Active),
            }))),
            Box::new(SelectorExpr::Intersection(
                Box::new(SelectorExpr::Atom(SelectorAtom::Entity(EntitySelector {
                    kind: EntityKind::Lease,
                    predicate: Some(Predicate::Active),
                }))),
                Box::new(SelectorExpr::Atom(SelectorAtom::Entity(EntitySelector {
                    kind: EntityKind::Branch,
                    predicate: Some(Predicate::Blocked),
                })))
            ))
        )
    );
}
