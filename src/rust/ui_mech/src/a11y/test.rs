use super::*;

fn bar() -> Tree {
    // A miniature Control Bar: the shape a real one must be able to produce.
    Tree::of(
        Node::new(
            TargetId(0),
            Role::Window,
            "Control Bar",
            Rect::new(0.0, 0.0, 400.0, 80.0),
        )
        .with_child(vec![
            Node::new(
                TargetId(1),
                Role::Button,
                "Play",
                Rect::new(8.0, 8.0, 60.0, 24.0),
            ),
            Node::new(
                TargetId(2),
                Role::Toggle,
                "Loop",
                Rect::new(72.0, 8.0, 60.0, 24.0),
            )
            .with_state(true),
            Node::new(
                TargetId(3),
                Role::Field,
                "Counter",
                Rect::new(8.0, 40.0, 200.0, 32.0),
            )
            .with_value("012|03|0000"),
        ]),
    )
}

#[test]
fn an_empty_tree_is_the_default() {
    let t = Tree::default();
    assert!(t.empty());
    assert!(t.node().is_empty());
}

#[test]
fn walk_visits_parents_before_children() {
    let ids: Vec<u64> = bar().node().iter().map(|n| n.id.0).collect();
    assert_eq!(ids, vec![0, 1, 2, 3]);
}

#[test]
fn nodes_are_addressed_by_the_same_id_input_routes_to() {
    // The property that keeps pointer and assistive technology pointed at one
    // thing: no separate accessibility identity to drift out of step.
    let t = bar();
    let root = t.root.as_ref().unwrap();
    assert_eq!(
        root.find(TargetId(2)).map(|n| n.label.as_str()),
        Some("Loop")
    );
    assert_eq!(root.find(TargetId(2)).and_then(|n| n.on), Some(true));
    assert!(root.find(TargetId(99)).is_none());
}

#[test]
fn a_field_reports_its_value_as_text() {
    // Not as pixels — a control whose value exists only on screen cannot answer.
    let t = bar();
    let counter = t.root.as_ref().unwrap().find(TargetId(3)).unwrap();
    assert_eq!(counter.role, Role::Field);
    assert_eq!(counter.value.as_deref(), Some("012|03|0000"));
}
