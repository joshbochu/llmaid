use llmaid::diagram::{self, Diagram};
use llmaid::layout;
use llmaid::parse::{self, EdgeKind, Shape};
use llmaid::render;
use llmaid::route;
use llmaid::scene::{
    Arrow, ArrowHead, Point, Rect, RoutedEdge, Scene, SceneBox, SceneGroup, SceneText,
};
use llmaid::style::Style;
use std::fs;
use std::path::PathBuf;
use unicode_width::UnicodeWidthStr;

#[test]
fn scene_bounds_include_geometry_paths_arrows_and_wide_text() {
    let scene = Scene {
        boxes: vec![SceneBox {
            node: 0,
            rect: Rect::new(-4, -2, 5, 3),
            lines: vec!["界".into()],
            shape: Shape::Rect,
            table: None,
        }],
        foreground_boxes: vec![],
        groups: vec![SceneGroup {
            subgraph: 0,
            rect: Rect::new(-6, -4, 9, 7),
            title: SceneText::new(Point::new(-3, -3), "group"),
        }],
        paths: vec![],
        edges: vec![RoutedEdge {
            edge: 0,
            points: vec![Point::new(0, 0), Point::new(8, 0)],
            rounded: vec![],
            kind: EdgeKind::Solid,
            label: Some(SceneText::new(Point::new(2, 1), "世界")),
            arrow: Some(Arrow {
                at: Point::new(8, 0),
                toward: Point::new(9, 0),
                head: ArrowHead::Filled,
            }),
        }],
        endpoint_decorations: vec![],
        texts: vec![],
    };

    assert_eq!(scene.bounds(), Rect::new(-6, -4, 16, 7));
}

#[test]
fn doubled_rect_centers_preserve_odd_and_even_cell_parity() {
    assert_eq!(Rect::new(3, 5, 5, 3).center2(), Point::new(10, 12));
    assert_eq!(Rect::new(3, 5, 4, 2).center2(), Point::new(9, 11));
}

#[test]
fn scene_normalize_translates_every_primitive_once() {
    let mut scene = Scene {
        boxes: vec![SceneBox {
            node: 0,
            rect: Rect::new(-3, -2, 5, 3),
            lines: vec!["node".into()],
            shape: Shape::Rounded,
            table: None,
        }],
        foreground_boxes: vec![],
        groups: vec![],
        paths: vec![],
        edges: vec![RoutedEdge {
            edge: 0,
            points: vec![Point::new(1, -1), Point::new(7, -1)],
            rounded: vec![Point::new(1, -1)],
            kind: EdgeKind::Dotted,
            label: Some(SceneText::new(Point::new(2, 1), "label")),
            arrow: Some(Arrow {
                at: Point::new(7, -1),
                toward: Point::new(8, -1),
                head: ArrowHead::Filled,
            }),
        }],
        endpoint_decorations: vec![],
        texts: vec![],
    };

    let size = scene.normalize();

    assert_eq!(size, (12, 4));
    assert_eq!(scene.boxes[0].rect, Rect::new(0, 0, 5, 3));
    assert_eq!(scene.edges[0].points[0], Point::new(4, 1));
    assert_eq!(scene.edges[0].rounded[0], Point::new(4, 1));
    assert_eq!(scene.edges[0].label.as_ref().unwrap().at, Point::new(5, 3));
    assert_eq!(
        scene.edges[0].arrow.as_ref().unwrap().toward,
        Point::new(11, 1)
    );
}

#[test]
fn foreground_box_owns_bounds_normalization_and_paints_over_paths() {
    let mut scene = Scene {
        boxes: vec![],
        foreground_boxes: vec![SceneBox {
            node: 9,
            rect: Rect::new(-2, -1, 9, 3),
            lines: vec!["note".into()],
            shape: Shape::Rect,
            table: None,
        }],
        groups: vec![],
        paths: vec![llmaid::scene::ScenePath {
            path: 0,
            points: vec![Point::new(2, -2), Point::new(2, 3)],
            rounded: vec![],
            kind: EdgeKind::Dotted,
        }],
        edges: vec![],
        endpoint_decorations: vec![],
        texts: vec![],
    };

    assert_eq!(scene.bounds(), Rect::new(-2, -2, 9, 6));
    assert_eq!(scene.normalize(), (9, 6));
    assert_eq!(scene.foreground_boxes[0].rect, Rect::new(0, 1, 9, 3));
    let (output, failures) = render::render_scene_with_checks(&scene, Style { ascii: false });
    assert!(failures.is_empty(), "{}\n{output}", failures.join("\n"));
    assert!(output.contains("note"), "{output}");
}

#[test]
fn forward_routing_produces_complete_screen_space_edges() {
    let graph = parse::parse("flowchart LR\nA[source] -->|scan| B[tokens]\n").unwrap();
    let placed = layout::layout(&graph, 100);

    let scene = route::route(&graph, &placed);

    assert_eq!(scene.boxes.len(), 2);
    assert_eq!(scene.edges.len(), 1);
    let edge = &scene.edges[0];
    assert_eq!(edge.edge, 0);
    assert!(edge.points.len() >= 2);
    assert_eq!(edge.label.as_ref().map(|l| l.text.as_str()), Some(" scan "));
    assert!(edge.arrow.is_some());
    assert_eq!(scene.bounds().x, 0);
    assert_eq!(scene.bounds().y, 0);
}

#[test]
fn horizontal_bend_labels_sit_on_their_branch_segment() {
    for source in [
        include_str!("cases/diamond.mmd"),
        include_str!("cases/edge-labels.mmd"),
    ] {
        let graph = parse::parse(source).unwrap();
        let placed = layout::layout(&graph, 100);
        let scene = route::route(&graph, &placed);

        for edge in scene.edges.iter().filter(|edge| edge.label.is_some()) {
            let label = edge.label.as_ref().unwrap();
            let label_left = label.at.x;
            let label_right = label_left + label.text.width() as i32 - 1;
            let supported = edge.points.windows(2).any(|points| {
                points[0].y == label.at.y
                    && points[1].y == label.at.y
                    && label_left >= points[0].x.min(points[1].x)
                    && label_right <= points[0].x.max(points[1].x)
            });
            assert!(supported, "edge {} label floats off its path", edge.edge);
        }
    }
}

#[test]
fn scene_painter_preserves_forward_geometry_and_removes_dead_origin_space() {
    for source in [
        "flowchart LR\nA[source] -->|scan| B[tokens]\nA --> C[errors]\n",
        "flowchart TB\nA -->|left| B\nA -->|right| C\nB --> D\nC --> D\n",
    ] {
        let graph = parse::parse(source).unwrap();
        let placed = layout::layout(&graph, 100);
        let style = Style { ascii: false };
        let expected = render::render(&graph, &placed, style);
        let scene = route::route(&graph, &placed);

        assert_eq!(
            render::render_scene(&scene, style),
            strip_common_indent(&expected),
            "{source}"
        );
    }
}

#[test]
fn scene_routing_includes_back_edges_and_self_loops() {
    let source = "\
flowchart TB
  scan[Scanner] --> parse[Parser] --> eval[Interpreter]
  eval -->|next line| scan
  eval -->|again| eval
";
    let graph = parse::parse(source).unwrap();
    let placed = layout::layout(&graph, 100);
    let style = Style { ascii: false };
    let expected = render::render(&graph, &placed, style);

    let scene = route::route(&graph, &placed);

    assert_eq!(scene.edges.len(), graph.edges.len());
    assert!(scene.edges.iter().all(|edge| edge.points.len() >= 2));
    assert!(scene.edges.iter().all(|edge| edge.arrow.is_some()));
    assert!(scene.edges.iter().any(|edge| {
        edge.label
            .as_ref()
            .is_some_and(|label| label.text.contains("next line"))
    }));
    assert!(scene.edges.iter().any(|edge| {
        edge.label
            .as_ref()
            .is_some_and(|label| label.text.contains("again"))
    }));
    assert_eq!(
        render::render_scene(&scene, style),
        strip_common_indent(&expected)
    );
}

#[test]
fn self_loop_has_a_readable_return_leg_below_the_box() {
    let graph = parse::parse("flowchart TB\nA[Interpreter] -->|again| A\n").unwrap();
    let placed = layout::layout(&graph, 100);
    let scene = route::route(&graph, &placed);
    let box_bottom = scene.boxes[0].rect.bottom();
    let loop_bottom = scene.edges[0]
        .points
        .iter()
        .map(|point| point.y)
        .max()
        .unwrap();

    assert!(
        loop_bottom > box_bottom,
        "self-loop should extend at least two rows beyond the bottom border"
    );
}

#[test]
fn public_render_matches_scene_pipeline_for_all_existing_cases() {
    let cases = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
    let mut inputs: Vec<PathBuf> = fs::read_dir(cases)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.unwrap().path();
            (path.extension().and_then(|ext| ext.to_str()) == Some("mmd")).then_some(path)
        })
        .collect();
    inputs.sort();

    let mut changed = Vec::new();
    for path in inputs {
        let source = fs::read_to_string(&path).unwrap();
        let Diagram::Flowchart(graph) = diagram::parse(&source).unwrap() else {
            continue;
        };
        if graph.nodes.is_empty() {
            continue;
        }
        let placed = layout::layout(&graph, 100);
        let style = Style { ascii: false };
        let current = render::render(&graph, &placed, style);
        let scene = route::route(&graph, &placed);
        let routed = render::render_scene(&scene, style);
        if routed != strip_common_indent(&current) {
            changed.push(path.file_stem().unwrap().to_string_lossy().into_owned());
        }
    }
    assert!(
        changed.is_empty(),
        "scene/public render diverged: {changed:?}"
    );
}

#[test]
fn scene_invariants_detect_a_label_overwritten_by_another_edge() {
    let scene = Scene {
        boxes: vec![],
        foreground_boxes: vec![],
        groups: vec![],
        paths: vec![],
        edges: vec![
            RoutedEdge {
                edge: 0,
                points: vec![Point::new(0, 0), Point::new(6, 0)],
                rounded: vec![],
                kind: EdgeKind::Solid,
                label: Some(SceneText::new(Point::new(2, 0), "one")),
                arrow: None,
            },
            RoutedEdge {
                edge: 1,
                points: vec![Point::new(0, 1), Point::new(6, 1)],
                rounded: vec![],
                kind: EdgeKind::Solid,
                label: Some(SceneText::new(Point::new(2, 0), "two")),
                arrow: None,
            },
        ],
        endpoint_decorations: vec![],
        texts: vec![],
    };

    let (_, failures) = render::render_scene_with_checks(&scene, Style { ascii: false });

    assert!(
        failures
            .iter()
            .any(|failure| failure.contains("edge 0 label") && failure.contains("overwritten")),
        "{failures:#?}"
    );
}

#[test]
fn scene_invariants_detect_an_edge_crossing_an_unrelated_box() {
    let scene = Scene {
        boxes: vec![SceneBox {
            node: 7,
            rect: Rect::new(2, 0, 5, 3),
            lines: vec![],
            shape: Shape::Rect,
            table: None,
        }],
        foreground_boxes: vec![],
        groups: vec![],
        paths: vec![],
        edges: vec![RoutedEdge {
            edge: 0,
            points: vec![Point::new(0, 1), Point::new(8, 1)],
            rounded: vec![],
            kind: EdgeKind::Solid,
            label: None,
            arrow: None,
        }],
        endpoint_decorations: vec![],
        texts: vec![],
    };

    let (_, failures) = render::render_scene_with_checks(&scene, Style { ascii: false });

    assert!(
        failures.iter().any(|failure| {
            failure.contains("edge 0")
                && failure.contains("non-endpoint box 7")
                && failure.contains("intersects")
        }),
        "{failures:#?}"
    );
}

#[test]
fn nested_group_scenes_keep_child_frames_inside_parent_padding() {
    let source = "\
flowchart LR
  subgraph Outer
    subgraph Inner
      A --> B
    end
    B --> C
  end
  C --> D
";
    let graph = parse::parse(source).unwrap();
    let placed = layout::layout(&graph, 100);
    let scene = route::route(&graph, &placed);
    let outer = scene
        .groups
        .iter()
        .find(|group| graph.subgraphs[group.subgraph].title == "Outer")
        .unwrap()
        .rect;
    let inner = scene
        .groups
        .iter()
        .find(|group| graph.subgraphs[group.subgraph].title == "Inner")
        .unwrap()
        .rect;

    assert!(inner.x - outer.x >= 2, "outer={outer:?}, inner={inner:?}");
    assert!(inner.y - outer.y >= 2, "outer={outer:?}, inner={inner:?}");
    assert!(
        outer.right() - inner.right() >= 2,
        "outer={outer:?}, inner={inner:?}"
    );
    assert!(
        outer.bottom() - inner.bottom() >= 2,
        "outer={outer:?}, inner={inner:?}"
    );
}

#[test]
fn title_expansion_preserves_a_single_column_groups_structural_center() {
    let source = "\
flowchart TB
  subgraph Pipe [Pipeline]
    A[src] --> B[tok]
  end
  B --> C[out]
";
    let graph = parse::parse(source).unwrap();
    let placed = layout::layout(&graph, 100);
    let scene = route::route(&graph, &placed);
    let group = scene.groups[0].rect;
    let member = scene.boxes[0].rect;
    let center2 = |rect: Rect| 2 * rect.x + rect.w - 1;

    assert_eq!(center2(group), center2(member));
}

#[test]
fn b15_given_an_external_node_then_its_box_does_not_intersect_the_group_frame() {
    let source = "\
flowchart TB
  classDef default fill:#f9f
  subgraph one
    A --> B
  end
  style A fill:#bbf
  A --> C
";
    let graph = parse::parse(source).unwrap();
    let placed = layout::layout(&graph, 100);
    let scene = route::route(&graph, &placed);
    let group = scene.groups[0].rect;
    let c_index = graph.nodes.iter().position(|node| node.id == "C").unwrap();
    let outside = scene.boxes[c_index].rect;
    let intersects = group.x < outside.right()
        && group.right() > outside.x
        && group.y < outside.bottom()
        && group.bottom() > outside.y;

    assert!(!intersects, "group={group:?}, outside={outside:?}");
}

fn strip_common_indent(text: &str) -> String {
    let indent = text
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.bytes().take_while(|&b| b == b' ').count())
        .min()
        .unwrap_or(0);
    text.lines()
        .map(|line| line.get(indent.min(line.len())..).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}
