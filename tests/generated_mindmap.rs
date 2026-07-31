use llmaid::diagram;
use llmaid::mindmap;
use llmaid::quality;
use llmaid::render;
use llmaid::style::Style;

fn source(parents: &[usize], labels: &[&str]) -> String {
    let mut output = String::from("mindmap\n  Root\n");
    let mut depths = vec![0usize];
    for (index, &parent) in parents.iter().enumerate() {
        assert!(parent <= index);
        let depth = depths[parent] + 1;
        depths.push(depth);
        output.push_str(&"  ".repeat(depth + 1));
        output.push_str(labels.get(index).copied().unwrap_or("node"));
        output.push('\n');
    }
    output
}

#[test]
fn generated_depth_breadth_and_unicode_trees_are_deterministic_and_valid() {
    let cases = [
        source(&[0, 1, 2, 3, 4, 5], &["深さ", "二", "三", "四", "五", "六"]),
        source(
            &[0, 0, 0, 0, 0, 0, 0, 0],
            &["α", "β", "γ", "δ", "ε", "ζ", "η", "θ"],
        ),
        source(
            &[0, 0, 1, 1, 2, 2, 4, 4],
            &[
                "API", "UI", "parse", "audit", "view", "edit", "JSON", "端末",
            ],
        ),
    ];

    for source in cases {
        let diagram = mindmap::parse(&source).unwrap_or_else(|error| panic!("{error}\n{source}"));
        for width in [18, 40, 100] {
            let scene = mindmap::scene(&diagram, width);
            let semantic = diagram::parse(&source).unwrap();
            let report = quality::evaluate(&semantic, &scene, width);
            assert_eq!(
                report.invariant_failed_checks(),
                0,
                "semantic invariant failures: {:?}\n{source}",
                report.invariant_failures().collect::<Vec<_>>()
            );
            assert_eq!(scene, mindmap::scene(&diagram, width), "{source}");
            assert_eq!(scene.boxes.len(), diagram.nodes.len(), "{source}");
            assert_eq!(scene.edges.len() + 1, diagram.nodes.len(), "{source}");
            for ascii in [false, true] {
                let (rendered, failures) =
                    render::render_scene_with_checks(&scene, Style { ascii });
                assert!(
                    failures.is_empty(),
                    "{}\n{source}\n{rendered}",
                    failures.join("; ")
                );
                if ascii {
                    for drawing in ['╭', '╮', '╰', '╯', '─', '│', '├', '┤', '┬', '┴', '┼']
                    {
                        assert!(!rendered.contains(drawing), "{source}\n{rendered}");
                    }
                }
            }
        }
    }
}

fn ordered_depth_sequences(nodes: usize) -> Vec<Vec<usize>> {
    fn visit(target: usize, depths: &mut Vec<usize>, output: &mut Vec<Vec<usize>>) {
        if depths.len() == target {
            output.push(depths.clone());
            return;
        }
        let maximum = depths.last().copied().unwrap_or(0) + 1;
        for depth in 1..=maximum {
            depths.push(depth);
            visit(target, depths, output);
            depths.pop();
        }
    }

    let mut output = Vec::new();
    visit(nodes, &mut vec![0], &mut output);
    output
}

fn source_from_depths(depths: &[usize]) -> String {
    let mut output = String::from("mindmap\n");
    for (index, depth) in depths.iter().enumerate() {
        output.push_str(&"  ".repeat(depth + 1));
        output.push_str(&format!("N{index}"));
        output.push('\n');
    }
    output
}

#[test]
fn all_small_ordered_tree_shapes_preserve_source_order_and_ascii_purity() {
    let mut cases = 0;
    for nodes in 1..=7 {
        for depths in ordered_depth_sequences(nodes) {
            let source = source_from_depths(&depths);
            let diagram = mindmap::parse(&source).unwrap();
            let scene = mindmap::scene(&diagram, 100);
            let semantic = diagram::parse(&source).unwrap();
            let report = quality::evaluate(&semantic, &scene, 100);
            assert_eq!(
                report.invariant_failed_checks(),
                0,
                "semantic invariant failures: {:?}\n{source}",
                report.invariant_failures().collect::<Vec<_>>()
            );
            let (rendered, failures) =
                render::render_scene_with_checks(&scene, Style { ascii: true });
            assert!(
                failures.is_empty(),
                "{}\n{source}\n{rendered}",
                failures.join("; ")
            );
            assert!(rendered.is_ascii(), "{source}\n{rendered}");

            for parent in 0..diagram.nodes.len() {
                let children: Vec<usize> = diagram
                    .nodes
                    .iter()
                    .enumerate()
                    .filter_map(|(index, node)| (node.parent == Some(parent)).then_some(index))
                    .collect();
                let centers: Vec<i32> = children
                    .iter()
                    .map(|&child| scene.boxes[child].rect.center2().y)
                    .collect();
                assert!(
                    centers.windows(2).all(|pair| pair[0] < pair[1]),
                    "{source}\n{rendered}"
                );
            }
            cases += 1;
        }
    }
    assert_eq!(cases, 197);
}
