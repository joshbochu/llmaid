use std::collections::BTreeMap;

use llmaid::audit;
use llmaid::layout;
use llmaid::parse;
use llmaid::render;
use llmaid::route;
use llmaid::style::Style;
use unicode_width::UnicodeWidthStr;

const DIRECTIONS: [&str; 4] = ["LR", "RL", "TB", "BT"];
const TOPOLOGIES: [(&str, &str); 6] = [
    ("simple", "A -->|one<br/>two| B"),
    ("parallel", "A -->|one<br/>two| B\nA -->|three<br/>four| B"),
    (
        "fork",
        "A -->|upper<br/>detail| B\nA -->|lower<br/>detail| C",
    ),
    (
        "merge",
        "A -->|upper<br/>detail| C\nB -->|lower<br/>detail| C",
    ),
    (
        "back",
        "A -->|forward<br/>path| B\nB -->|return<br/>path| A",
    ),
    ("self", "A -->|again<br/>later| A"),
];

#[test]
fn multiline_edge_labels_are_distinct_and_invariant_clean_across_topologies() {
    for direction in DIRECTIONS {
        for (topology, body) in TOPOLOGIES {
            let source = format!("flowchart {direction}\n{body}\n");
            let graph = parse::parse(&source).unwrap();
            let placed = layout::layout(&graph, 100);
            let scene = route::route(&graph, &placed);
            let rendered = render::render_scene_with_checks(&scene, Style { ascii: false });
            let case = format!("{direction}/{topology}");

            assert!(
                rendered.1.is_empty(),
                "{case}: {}\n{}",
                rendered.1.join("; "),
                rendered.0
            );
            let expected_labels = graph
                .edges
                .iter()
                .filter(|edge| edge.label.is_some())
                .count();
            assert_eq!(
                scene
                    .edges
                    .iter()
                    .filter(|edge| edge.label.is_some())
                    .count(),
                expected_labels,
                "{case}: every source label must remain visible"
            );

            let mut occupied = BTreeMap::new();
            for edge in &scene.edges {
                let Some(label) = &edge.label else {
                    continue;
                };
                for (row, line) in label.text.split('\n').enumerate() {
                    for column in 0..line.width() {
                        let cell = (label.at.x + column as i32, label.at.y + row as i32);
                        assert_eq!(
                            occupied.insert(cell, edge.edge),
                            None,
                            "{case}: labels share cell {cell:?}"
                        );
                    }
                }
            }

            let report = audit::flowchart_json(&graph, &placed, &scene);
            assert!(
                !report.contains("\"name\":\"scene_invariant\""),
                "{case}: {report}\n{}",
                rendered.0
            );
        }
    }
}
