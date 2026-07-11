use llmaid::boxed::{BoxDiagram, BoxNode, NodeId};
use llmaid::parse::{Dir, EdgeKind, Shape};
use llmaid::render;
use llmaid::style::Style;
use unicode_width::UnicodeWidthStr;

fn chain(dir: Dir) -> BoxDiagram {
    let mut diagram = BoxDiagram::new(dir);
    let first = diagram.add_node(BoxNode::new("first", "alpha\nβeta", Shape::Rounded));
    let second = diagram.add_node(BoxNode::new("second", "界面", Shape::Rect));
    diagram.add_edge(first, second).label("go");
    diagram
}

#[test]
fn declaration_order_and_edge_properties_survive_lowering() {
    let mut diagram = BoxDiagram::new(Dir::LR);
    let z = diagram.add_node(BoxNode::new("z", "Last alphabetically", Shape::Diamond));
    let a = diagram.add_node(BoxNode::new("a", "First alphabetically", Shape::Cylinder));
    diagram
        .add_edge(z, a)
        .kind(EdgeKind::Dotted)
        .without_arrow()
        .label("declared first");
    diagram.add_edge(a, z).kind(EdgeKind::Thick).with_arrow();

    let scene = diagram.scene(120);
    assert_eq!(scene.boxes.len(), 2);
    assert_eq!(scene.boxes[0].shape, Shape::Diamond);
    assert_eq!(scene.boxes[0].lines, ["Last alphabetically"]);
    assert_eq!(scene.boxes[1].shape, Shape::Cylinder);
    assert_eq!(scene.edges.len(), 2);
    assert_eq!(scene.edges[0].kind, EdgeKind::Dotted);
    assert_eq!(
        scene.edges[0].label.as_ref().unwrap().text,
        " declared first "
    );
    assert!(scene.edges[0].arrow.is_none());
    assert_eq!(scene.edges[1].kind, EdgeKind::Thick);
    assert!(scene.edges[1].arrow.is_some());
}

#[test]
fn multiline_and_unicode_labels_are_measured_without_truncation() {
    let diagram = chain(Dir::LR);
    let scene = diagram.scene(120);
    let output = render::render_scene(&scene, Style { ascii: false });

    assert_eq!(scene.boxes[0].lines, ["alpha", "βeta"]);
    assert_eq!(scene.boxes[1].lines, ["界面"]);
    assert!(output.contains("alpha"), "{output}");
    assert!(output.contains("βeta"), "{output}");
    assert!(output.contains("界面"), "{output}");
    assert_eq!("界面".width(), 4);
    assert!(scene.boxes[1].rect.w >= "界面".width() as i32 + 2);
}

#[test]
fn all_directions_preserve_exact_flow_order_and_box_geometry() {
    let lr = chain(Dir::LR).scene(120);
    let rl = chain(Dir::RL).scene(120);
    let tb = chain(Dir::TB).scene(120);
    let bt = chain(Dir::BT).scene(120);

    assert_eq!(lr.bounds(), rl.bounds());
    // Vertical edge labels are deliberately biased to a readable side and may
    // contribute one parity column to normalized bounds. Flow-axis geometry
    // itself is an exact mirror.
    assert_eq!(tb.bounds().h, bt.bounds().h);
    assert!(lr.boxes[0].rect.x < lr.boxes[1].rect.x);
    assert!(rl.boxes[0].rect.x > rl.boxes[1].rect.x);
    assert!(tb.boxes[0].rect.y < tb.boxes[1].rect.y);
    assert!(bt.boxes[0].rect.y > bt.boxes[1].rect.y);

    // Odd/even extents cannot share an integer cell center; the doubled-center
    // residual is exactly one half-cell in that case.
    assert_eq!(
        (lr.boxes[0].rect.center2().y - lr.boxes[1].rect.center2().y).abs(),
        1
    );
    assert_eq!(
        (tb.boxes[0].rect.center2().x - tb.boxes[1].rect.center2().x).abs(),
        0
    );

    for node in 0..2 {
        assert_eq!(lr.boxes[node].rect.w, rl.boxes[node].rect.w);
        assert_eq!(lr.boxes[node].rect.h, rl.boxes[node].rect.h);
        // BT may add exactly one cross-axis parity column so the terminal
        // arrow and label land on an integer cell (the existing B16 contract).
        assert!((tb.boxes[node].rect.w - bt.boxes[node].rect.w).abs() <= 1);
        assert_eq!(tb.boxes[node].rect.h, bt.boxes[node].rect.h);
        assert_eq!(
            lr.boxes[node].rect.x + rl.boxes[node].rect.x + lr.boxes[node].rect.w,
            lr.bounds().w
        );
        assert_eq!(
            tb.boxes[node].rect.y + bt.boxes[node].rect.y + tb.boxes[node].rect.h,
            tb.bounds().h
        );
    }
}

#[test]
fn repeated_lowering_is_byte_identical_and_invariant_clean() {
    let diagram = chain(Dir::TB);
    let first = diagram.scene(80);
    let second = diagram.scene(80);
    assert_eq!(first, second);

    let (output, failures) = render::render_scene_with_checks(&first, Style { ascii: false });
    assert!(failures.is_empty(), "{}\n{output}", failures.join("\n"));
}

#[test]
fn node_handles_are_diagram_local_indices() {
    let mut diagram = BoxDiagram::new(Dir::LR);
    assert_eq!(diagram.add_node(BoxNode::rect("a", "A")), NodeId::new(0));
    assert_eq!(diagram.add_node(BoxNode::rect("b", "B")), NodeId::new(1));
}
