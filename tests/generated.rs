//! Deterministic generated and metamorphic coverage for small flowcharts.
//!
//! This deliberately exhausts a small, fixed space instead of using random
//! generation. Every forward-edge subset on two through four declared nodes is
//! exercised in all four Mermaid directions.

use llmaid::audit;
use llmaid::layout;
use llmaid::parse;
use llmaid::render;
use llmaid::route;
use llmaid::style::Style;

const DIRECTIONS: [&str; 4] = ["LR", "RL", "TB", "BT"];

fn forward_edges(nodes: usize) -> Vec<(usize, usize)> {
    let mut edges = Vec::new();
    for from in 0..nodes {
        for to in from + 1..nodes {
            edges.push((from, to));
        }
    }
    edges
}

fn source(nodes: usize, mask: usize, direction: &str) -> String {
    let mut source = format!("flowchart {direction}\n");
    for node in 0..nodes {
        source.push_str(&format!("N{node}[node {node}]\n"));
    }
    for (bit, (from, to)) in forward_edges(nodes).into_iter().enumerate() {
        if mask & (1 << bit) != 0 {
            source.push_str(&format!("N{from} --> N{to}\n"));
        }
    }
    source
}

#[test]
fn widened_fork_does_not_swallow_an_unrelated_long_edge_lane() {
    let source = "\
flowchart LR
N0[node 0]
N1[node 1]
N2[node 2]
N3[node 3]
N0 --> N1
N0 --> N2
N1 --> N2
N1 --> N3
";
    let graph = parse::parse(source).unwrap();
    let placed = layout::layout(&graph, 100);
    let scene = route::route(&graph, &placed);
    let geometry = audit::measure(&graph, &placed, &scene);
    let (rendered, failures) = render::render_scene_with_checks(&scene, Style { ascii: false });

    assert!(
        geometry.hard_violations.is_empty(),
        "{:?}\n{rendered}",
        geometry.hard_violations
    );
    assert!(failures.is_empty(), "{}\n{rendered}", failures.join("; "));
}

#[test]
fn exhaustive_small_dags_satisfy_scene_and_geometry_invariants() {
    let style = Style { ascii: false };
    let mut cases = 0;

    for nodes in 2..=4 {
        let edge_slots = forward_edges(nodes).len();
        for mask in 1..(1 << edge_slots) {
            for direction in DIRECTIONS {
                let source = source(nodes, mask, direction);
                let graph = parse::parse(&source).unwrap_or_else(|error| {
                    panic!("generated case did not parse: {error}\n{source}")
                });
                let placed = layout::layout(&graph, 100);
                let scene = route::route(&graph, &placed);
                let geometry = audit::measure(&graph, &placed, &scene);
                let (rendered, failures) = render::render_scene_with_checks(&scene, style);

                assert!(
                    geometry.hard_violations.is_empty(),
                    "geometry violations: {:?}\n{source}\n{rendered}",
                    geometry.hard_violations
                );
                assert!(
                    failures.is_empty(),
                    "render invariants: {}\n{source}\n{rendered}",
                    failures.join("; ")
                );
                assert_eq!(scene.boxes.len(), nodes, "{source}");
                assert_eq!(scene.edges.len(), mask.count_ones() as usize, "{source}");
                assert!(
                    scene
                        .edges
                        .iter()
                        .all(|edge| edge.points.len() >= 2 && edge.arrow.is_some()),
                    "incomplete routed edge\n{source}\n{rendered}"
                );
                assert!(
                    !rendered.contains('…'),
                    "truncated output\n{source}\n{rendered}"
                );

                // Re-running the whole integer pipeline must be byte-identical.
                let placed_again = layout::layout(&graph, 100);
                assert_eq!(
                    rendered,
                    render::render(&graph, &placed_again, style),
                    "non-deterministic render\n{source}"
                );
                cases += 1;
            }
        }
    }

    assert_eq!(cases, 284);
}

#[test]
fn small_dag_geometry_is_invariant_under_axis_mirroring() {
    let mut topology_count = 0;

    for nodes in 2..=4 {
        let edge_slots = forward_edges(nodes).len();
        for mask in 1..(1 << edge_slots) {
            let signature = |direction| {
                let source = source(nodes, mask, direction);
                let graph = parse::parse(&source).unwrap();
                audit::measure_graph(&graph, 100).comparable_signature()
            };

            assert_eq!(
                signature("LR"),
                signature("RL"),
                "nodes={nodes} mask={mask}"
            );
            assert_eq!(
                signature("TB"),
                signature("BT"),
                "nodes={nodes} mask={mask}"
            );
            topology_count += 1;
        }
    }

    assert_eq!(topology_count, 71);
}
