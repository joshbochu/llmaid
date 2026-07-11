use llmaid::audit;
use llmaid::scene::{CardinalityMaximum, CardinalityMinimum, EndpointDecorationKind};
use llmaid::style::Style;
use llmaid::{class, er, layout, parse, render, route};
use unicode_width::UnicodeWidthStr;

fn audit_source(source: &str) -> audit::GeometryAudit {
    let graph = parse::parse(source).unwrap();
    let placed = layout::layout(&graph, 100);
    let scene = route::route(&graph, &placed);
    audit::measure(&graph, &placed, &scene)
}

fn placed_source(source: &str) -> (parse::Graph, layout::Placed) {
    let graph = parse::parse(source).unwrap();
    let placed = layout::layout(&graph, 100);
    (graph, placed)
}

#[test]
fn pipeline_quality_contract_is_an_exact_centerline_without_bends() {
    let audit = audit_source("flowchart LR\nA[source] --> B[tokens] --> C[ast]\n");

    assert_eq!(audit.hard_violations.len(), 0, "{audit:#?}");
    assert_eq!(audit.mono_centerline_residual2, 0, "{audit:#?}");
    assert_eq!(audit.bends, 0, "{audit:#?}");
}

#[test]
fn fanout_quality_contract_centers_both_junctions_and_mirrors_the_diamond() {
    let audit = audit_source("flowchart LR\nA --> B & C\nB & C --> D\n");

    assert_eq!(audit.hard_violations.len(), 0, "{audit:#?}");
    assert_eq!(audit.fork_barycenter_residual2, 0, "{audit:#?}");
    assert_eq!(audit.merge_barycenter_residual2, 0, "{audit:#?}");
    assert_eq!(audit.diamond_mirror_residual2, 0, "{audit:#?}");
    assert_eq!(audit.diamond_motifs, 1, "{audit:#?}");
}

#[test]
fn fanout_uses_one_visible_fork_track_and_one_visible_merge_track() {
    let (graph, placed) = placed_source("flowchart LR\nA --> B & C\nB & C --> D\n");
    let outgoing: Vec<usize> = graph
        .edges
        .iter()
        .enumerate()
        .filter_map(|(edge, candidate)| (candidate.from == 0).then_some(edge))
        .collect();
    let incoming: Vec<usize> = graph
        .edges
        .iter()
        .enumerate()
        .filter_map(|(edge, candidate)| (candidate.to == 3).then_some(edge))
        .collect();

    let source_ports: Vec<usize> = outgoing
        .iter()
        .map(|&edge| placed.segs[edge].first().unwrap().from.1)
        .collect();
    let fork_tracks: Vec<Option<usize>> = outgoing
        .iter()
        .map(|&edge| placed.segs[edge].first().unwrap().track)
        .collect();
    let target_ports: Vec<usize> = incoming
        .iter()
        .map(|&edge| placed.segs[edge].last().unwrap().to.1)
        .collect();
    let merge_tracks: Vec<Option<usize>> = incoming
        .iter()
        .map(|&edge| placed.segs[edge].last().unwrap().track)
        .collect();

    assert!(source_ports.windows(2).all(|ports| ports[0] == ports[1]));
    assert!(fork_tracks.windows(2).all(|tracks| tracks[0] == tracks[1]));
    assert!(target_ports.windows(2).all(|ports| ports[0] == ports[1]));
    assert!(merge_tracks.windows(2).all(|tracks| tracks[0] == tracks[1]));
}

#[test]
fn labeled_diamond_has_a_centered_trunk_and_mirrored_branches() {
    let source = include_str!("cases/diamond.mmd");
    let (graph, placed) = placed_source(source);
    let scene = route::route(&graph, &placed);
    let router = graph.nodes.iter().position(|node| node.id == "B").unwrap();
    let agent = graph.nodes.iter().position(|node| node.id == "A").unwrap();
    let code = graph.nodes.iter().position(|node| node.id == "C").unwrap();
    let query = graph.nodes.iter().position(|node| node.id == "D").unwrap();
    let outgoing: Vec<usize> = graph
        .edges
        .iter()
        .enumerate()
        .filter_map(|(edge, candidate)| (candidate.from == router).then_some(edge))
        .collect();

    assert_eq!(
        scene.boxes[agent].rect.center2().y,
        scene.boxes[router].rect.center2().y
    );
    assert_eq!(
        scene.boxes[code].rect.center2().y + scene.boxes[query].rect.center2().y,
        2 * scene.boxes[router].rect.center2().y
    );
    assert_eq!(scene.boxes[code].rect.x, scene.boxes[query].rect.x);
    assert!(outgoing.windows(2).all(|edges| {
        let left = placed.segs[edges[0]].first().unwrap();
        let right = placed.segs[edges[1]].first().unwrap();
        left.from.1 == right.from.1 && left.track == right.track
    }));
}

#[test]
fn simple_vertical_chain_uses_one_box_width_and_one_centerline() {
    let source = include_str!("cases/dir-tb-labels.mmd");
    let (graph, placed) = placed_source(source);
    let scene = route::route(&graph, &placed);

    assert!(
        placed
            .boxes
            .windows(2)
            .all(|boxes| boxes[0].clen == boxes[1].clen)
    );
    let centers2: Vec<i32> = scene
        .boxes
        .iter()
        .map(|box_| box_.rect.center2().x)
        .collect();
    assert!(centers2.windows(2).all(|centers| centers[0] == centers[1]));
    for edge in &placed.segs {
        for segment in edge {
            assert_eq!(segment.from.1, segment.to.1);
        }
    }
}

#[test]
fn vertical_edge_labels_are_midpointed_in_equal_rank_gaps() {
    let source = include_str!("cases/dir-tb-labels.mmd");
    let (graph, placed) = placed_source(source);
    let scene = route::route(&graph, &placed);
    let mut boxes: Vec<_> = scene.boxes.iter().map(|box_| box_.rect).collect();
    boxes.sort_by_key(|rect| rect.y);
    let gaps: Vec<i32> = boxes
        .windows(2)
        .map(|pair| pair[1].y - pair[0].bottom())
        .collect();

    assert!(gaps.windows(2).all(|pair| pair[0] == pair[1]));
    for edge in &scene.edges {
        let label = edge.label.as_ref().unwrap();
        let source = scene.boxes[graph.edges[edge.edge].from].rect;
        let target = scene.boxes[graph.edges[edge.edge].to].rect;
        assert_eq!(label.at.y, (source.bottom() + target.y - 1) / 2);
    }
}

#[test]
fn bottom_to_top_chain_uses_one_arrow_column_with_only_parity_box_residual() {
    let source = include_str!("cases/dir-bt.mmd");
    let (graph, placed) = placed_source(source);
    let scene = route::route(&graph, &placed);
    let mut boxes: Vec<_> = scene.boxes.iter().map(|box_| box_.rect).collect();
    boxes.sort_by_key(|rect| rect.y);
    let label = scene.edges[0].label.as_ref().unwrap();

    assert!((boxes[0].center2().x - boxes[1].center2().x).abs() <= 1);
    let arrow_x = scene.edges[0].points[0].x;
    assert!(scene.edges[0].points.iter().all(|point| point.x == arrow_x));
    assert_eq!(label.at.y, (boxes[0].bottom() + boxes[1].y - 1) / 2);
}

#[test]
fn bottom_to_top_uses_equal_boxes_and_biases_labels_toward_the_arrow_column() {
    let source = include_str!("cases/dir-bt.mmd");
    let (graph, placed) = placed_source(source);
    let scene = route::route(&graph, &placed);
    let rendered = render::render(&graph, &placed, Style { ascii: false });
    let arrow_x2 = 2 * scene.edges[0].points[0].x;

    assert!(
        placed
            .boxes
            .windows(2)
            .all(|boxes| boxes[0].clen == boxes[1].clen)
    );
    for node in &graph.nodes {
        let text_width = node.label.len() as i32;
        let line = rendered
            .lines()
            .find(|line| line.contains(&node.label))
            .unwrap();
        let byte_x = line.find(&node.label).unwrap();
        let text_x = line[..byte_x].width() as i32;
        let text_center2 = 2 * text_x + text_width - 1;
        assert!(
            (text_center2 - arrow_x2).abs() <= 1,
            "{} text={text_center2} arrow={arrow_x2}",
            node.id
        );
    }

    let top_line = rendered.lines().find(|line| line.contains("top")).unwrap();
    let inner = top_line.trim_matches('│');
    let (left, right) = inner.split_once("top").unwrap();
    assert_eq!(left.width(), right.width());
}

#[test]
fn top_to_bottom_normalized_labels_bias_toward_the_shared_arrow_column() {
    let source = include_str!("cases/dir-tb-labels.mmd");
    let (graph, placed) = placed_source(source);
    let scene = route::route(&graph, &placed);
    let rendered = render::render(&graph, &placed, Style { ascii: false });
    let arrow_x2 = 2 * scene.edges[0].points[0].x;

    for (node, box_) in graph.nodes.iter().zip(&scene.boxes) {
        let text_width = node.label.len() as i32;
        let line = rendered
            .lines()
            .find(|line| line.contains(&node.label))
            .unwrap();
        let byte_x = line.find(&node.label).unwrap();
        let text_x = line[..byte_x].width() as i32;
        let text_center2 = 2 * text_x + text_width - 1;
        assert!(
            (text_center2 - arrow_x2).abs() <= 1,
            "{} text={text_center2} arrow={arrow_x2}",
            node.id
        );
        assert!(text_center2 >= box_.rect.center2().x, "{}", node.id);
    }
}

#[test]
fn non_reconverging_horizontal_forks_widen_to_keep_every_edge_straight() {
    let source = include_str!("cases/shapes.mmd");
    let (_, placed) = placed_source(source);

    for segments in &placed.segs {
        for segment in segments {
            assert_eq!(segment.from.1, segment.to.1, "{source}");
            assert_eq!(segment.track, None, "{source}");
        }
    }
}

#[test]
fn vertical_forks_and_merges_widen_junction_boxes_to_avoid_sidewinding() {
    for source in [
        include_str!("cases/forkmerge.mmd"),
        include_str!("cases/ignored-directives.mmd"),
        include_str!("cases/nested-merge.mmd"),
    ] {
        let (_, placed) = placed_source(source);
        for segments in &placed.segs {
            for segment in segments {
                assert_eq!(segment.from.1, segment.to.1, "{source}");
                assert_eq!(segment.track, None, "{source}");
            }
        }
    }
}

#[test]
fn root_vertical_fork_keeps_two_cells_between_ports_and_box_corners() {
    let source = include_str!("cases/forkmerge.mmd");
    let (graph, placed) = placed_source(source);
    let ast = graph
        .nodes
        .iter()
        .position(|node| node.id == "ast")
        .unwrap();
    let box_ = &placed.boxes[ast];

    for (edge, candidate) in graph.edges.iter().enumerate() {
        if candidate.from != ast {
            continue;
        }
        let port = placed.segs[edge].first().unwrap().from.1;
        assert!(port >= box_.c + 2);
        assert!(port + 2 < box_.c + box_.clen);
    }
}

#[test]
fn group_boundary_fork_staggers_external_child_after_internal_content() {
    let source = include_str!("cases/ignored-directives.mmd");
    let (graph, placed) = placed_source(source);
    let fork = graph.nodes.iter().position(|node| node.id == "A").unwrap();
    let internal = graph.nodes.iter().position(|node| node.id == "B").unwrap();
    let external = graph.nodes.iter().position(|node| node.id == "C").unwrap();

    assert_eq!(placed.boxes[internal].rank, placed.boxes[fork].rank + 1);
    assert!(placed.boxes[external].rank > placed.boxes[internal].rank);
}

#[test]
fn asymmetric_graph_is_not_graded_as_a_mirror_candidate() {
    let audit = audit_source("flowchart TB\nA --> B\nA --> C\nB --> D\nD --> E\nC --> E\n");

    assert_eq!(audit.diamond_motifs, 0, "{audit:#?}");
    assert_eq!(audit.diamond_mirror_residual2, 0, "{audit:#?}");
}

#[test]
fn forkmerge_quality_contract_centers_each_merge_on_its_parents() {
    let audit = audit_source(
        "flowchart TB\n\
         ast[AST] --> compile & walk[walk tree]\n\
         compile --> chunk[Chunk] --> vm[VM stack]\n\
         walk --> vm\n\
         vm --> value[Value]\n",
    );

    assert_eq!(audit.hard_violations.len(), 0, "{audit:#?}");
    assert_eq!(audit.merge_barycenter_residual2, 0, "{audit:#?}");
}

#[test]
fn opposite_directions_have_equal_normalized_quality() {
    for (forward, reverse) in [("LR", "RL"), ("TB", "BT")] {
        let source = |direction: &str| format!("flowchart {direction}\nA --> B & C\nB & C --> D\n");
        let a = audit_source(&source(forward));
        let b = audit_source(&source(reverse));

        assert_eq!(a.comparable_signature(), b.comparable_signature());
    }
}

#[test]
fn class_adornments_are_exactly_adjacent_to_their_semantic_endpoint() {
    let diagram = class::parse(
        "classDiagram\ndirection LR\nclass Whole {\n+id\n}\nWhole \"1\" *-- \"0..*\" Part : contains\n",
    )
    .unwrap();
    let scene = class::scene(&diagram, 100);
    let decoration = &scene.endpoint_decorations[0];
    assert_eq!(decoration.kind, EndpointDecorationKind::FilledDiamond);
    assert_eq!(
        (decoration.at.x - decoration.toward.x).abs()
            + (decoration.at.y - decoration.toward.y).abs(),
        1
    );
    assert!(scene.boxes[0].rect.contains(decoration.toward));
    assert!(!scene.boxes[0].rect.contains(decoration.at));
    assert_eq!(scene.texts.len(), 2);
    assert_eq!(scene.boxes[0].table.as_ref().unwrap().rows.len(), 1);
    assert_eq!(scene.boxes[0].rect.h, 5);
}

#[test]
fn er_cardinalities_are_exactly_adjacent_and_tables_keep_one_row_per_attribute() {
    let diagram = er::parse(
        "erDiagram\ndirection LR\nA {\nstring id PK\nstring parent_id FK\n}\nA ||--o{ B : owns\n",
    )
    .unwrap();
    let scene = er::scene(&diagram, 100);
    assert_eq!(scene.endpoint_decorations.len(), 2);
    assert_eq!(
        scene.endpoint_decorations[0].kind,
        EndpointDecorationKind::Cardinality {
            minimum: CardinalityMinimum::One,
            maximum: CardinalityMaximum::One,
        }
    );
    assert_eq!(
        scene.endpoint_decorations[1].kind,
        EndpointDecorationKind::Cardinality {
            minimum: CardinalityMinimum::Zero,
            maximum: CardinalityMaximum::Many,
        }
    );
    for (decoration, box_) in scene.endpoint_decorations.iter().zip(&scene.boxes) {
        assert_eq!(
            (decoration.at.x - decoration.toward.x).abs()
                + (decoration.at.y - decoration.toward.y).abs(),
            1
        );
        assert!(box_.rect.contains(decoration.toward));
        assert!(!box_.rect.contains(decoration.at));
    }
    assert_eq!(scene.boxes[0].table.as_ref().unwrap().rows.len(), 2);
    assert_eq!(scene.boxes[0].rect.h, 7);
}
