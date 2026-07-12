use llmaid::audit;
use llmaid::scene::{CardinalityMaximum, CardinalityMinimum, EndpointDecorationKind};
use llmaid::style::Style;
use llmaid::{class, er, layout, mindmap, parse, render, route, temporal, timeline};
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
        2
    );
    assert!(scene.boxes[0].rect.contains(decoration.toward));
    assert!(!scene.boxes[0].rect.contains(decoration.at));
    assert_eq!(scene.texts.len(), 2);
    assert_eq!(scene.boxes[0].table.as_ref().unwrap().rows.len(), 1);
    assert_eq!(scene.boxes[0].rect.h, 5);
    assert!(
        scene.boxes[1].rect.x - scene.boxes[0].rect.right() >= 17,
        "class relationship channel should reserve visible endpoint breathing room"
    );
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
            2
        );
        assert!(box_.rect.contains(decoration.toward));
        assert!(!box_.rect.contains(decoration.at));
    }
    assert_eq!(scene.boxes[0].table.as_ref().unwrap().rows.len(), 2);
    assert_eq!(scene.boxes[0].rect.h, 7);
}

#[test]
fn mindmap_parent_ports_are_centered_on_their_ordered_child_span() {
    let diagram = mindmap::parse(
        "mindmap\n  Root\n    Alpha\n      A1\n      A2\n    Beta\n      B1\n      B2\n",
    )
    .unwrap();
    let scene = mindmap::scene(&diagram, 100);

    for parent in 0..diagram.nodes.len() {
        let children: Vec<usize> = diagram
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, child)| (child.parent == Some(parent)).then_some(index))
            .collect();
        if children.is_empty() {
            continue;
        }
        let parent_center2 = scene.boxes[parent].rect.center2().y;
        let first_center2 = scene.boxes[children[0]].rect.center2().y;
        let last_center2 = scene.boxes[*children.last().unwrap()].rect.center2().y;
        assert_eq!(2 * parent_center2, first_center2 + last_center2);
    }
}

#[test]
fn mindmap_edges_attach_exactly_to_parent_and_child_center_rows() {
    let diagram = mindmap::parse("mindmap\n  Root\n    A\n      A1\n    B\n").unwrap();
    let scene = mindmap::scene(&diagram, 100);

    for edge in &scene.edges {
        let child = edge.edge + 1;
        let parent = diagram.nodes[child].parent.unwrap();
        let source = scene.boxes[parent].rect;
        let target = scene.boxes[child].rect;
        assert_eq!(edge.points.first().unwrap().x, source.right() - 1);
        assert_eq!(2 * edge.points.first().unwrap().y, source.center2().y);
        assert_eq!(edge.points.last().unwrap().x, target.x);
        assert_eq!(2 * edge.points.last().unwrap().y, target.center2().y);
        assert!(edge.arrow.is_none());
    }
}

#[test]
fn mindmap_boxes_keep_one_visible_padding_cell_beside_every_label() {
    let diagram = mindmap::parse("mindmap\n  Agent loop\n    Read contracts\n    解析\n").unwrap();
    let scene = mindmap::scene(&diagram, 100);
    for (node, box_) in diagram.nodes.iter().zip(&scene.boxes) {
        assert!(
            box_.rect.w >= node.label.width() as i32 + 4,
            "{} needs border + visible left/right padding",
            node.label
        );
    }
}

#[test]
fn temporal_ranks_have_strict_chronology_one_spine_exact_attachments_and_separate_bands() {
    use temporal::{Extent, TemporalBand, TemporalEntry, TemporalGaps};

    let entries = [
        TemporalEntry {
            leading: Extent {
                width: 8,
                height: 1,
            },
            trailing: vec![
                Extent {
                    width: 10,
                    height: 1,
                },
                Extent {
                    width: 7,
                    height: 3,
                },
            ],
            band: Some(0),
        },
        TemporalEntry {
            leading: Extent {
                width: 6,
                height: 3,
            },
            trailing: vec![Extent {
                width: 12,
                height: 1,
            }],
            band: Some(1),
        },
    ];
    let bands = [
        TemporalBand {
            first_entry: 0,
            entry_count: 1,
            title_width: 10,
        },
        TemporalBand {
            first_entry: 1,
            entry_count: 1,
            title_width: 8,
        },
    ];
    let placed = temporal::layout(&entries, &bands, TemporalGaps::compact());

    assert!(placed.anchors.windows(2).all(|pair| pair[0].y < pair[1].y));
    assert!(
        placed
            .anchors
            .iter()
            .all(|anchor| anchor.x == placed.spine_x)
    );
    for (leading, trailing) in placed.leading_boxes.iter().zip(&placed.trailing_boxes) {
        assert_eq!(placed.spine_x - leading.right(), 2);
        assert!(trailing.iter().all(|rect| rect.x - placed.spine_x == 3));
        if let Some(first) = trailing.first() {
            let connector_midpoint2 = leading.right() - 1 + first.x;
            assert_eq!(
                connector_midpoint2,
                2 * placed.spine_x,
                "the compact connector stroke must center exactly on the spine cell"
            );
        }
    }
    for entry in 0..entries.len() {
        let lead = &placed.connectors[placed
            .connectors
            .iter()
            .position(|connector| connector.entry == entry && connector.trailing.is_none())
            .unwrap()];
        assert_eq!(lead.points[0].x, placed.leading_boxes[entry].right() - 1);
        assert_eq!(lead.points[0].y, placed.anchors[entry].y);
        assert_eq!(lead.points.last().unwrap(), &placed.anchors[entry]);
        assert_eq!(
            placed.leading_boxes[entry].center2().y,
            2 * placed.anchors[entry].y
        );
        for (trailing, rect) in placed.trailing_boxes[entry].iter().enumerate() {
            let connector = placed
                .connectors
                .iter()
                .find(|connector| connector.entry == entry && connector.trailing == Some(trailing))
                .unwrap();
            assert_eq!(connector.points[0].x, placed.spine_x);
            assert_eq!(connector.points.last().unwrap().x, rect.x);
            assert_eq!(2 * connector.points.last().unwrap().y, rect.center2().y);
        }
        if let (Some(first), Some(last)) = (
            placed.trailing_boxes[entry].first(),
            placed.trailing_boxes[entry].last(),
        ) {
            let span_sum = first.center2().y / 2 + last.center2().y / 2;
            assert_eq!(
                2 * placed.anchors[entry].y,
                span_sum + span_sum.rem_euclid(2),
                "period anchors use an exact deterministic lower bias only when the event span midpoint is a half-cell"
            );
        }
    }
    assert!(placed.band_rects[0].bottom() < placed.band_rects[1].y);
    for (band, rect) in placed.band_rects.iter().enumerate() {
        let entry = bands[band].first_entry;
        assert!(rect.contains(placed.anchors[entry]));
        for corner in rect_corners(placed.leading_boxes[entry]) {
            assert!(rect.contains(corner));
        }
        for trailing in &placed.trailing_boxes[entry] {
            for corner in rect_corners(*trailing) {
                assert!(rect.contains(corner));
            }
        }
    }
}

#[test]
fn timeline_titles_center_on_the_compact_chronological_spine() {
    let unsectioned = timeline::parse(
        "timeline\n  title Product launch\n  Plan : Define scope\n  Build : Implement core\n  Ship : Release\n",
    )
    .unwrap();
    let scene = timeline::scene(&unsectioned, 100);
    let spine_x = scene.paths[0].points[0].x;
    let title = scene
        .texts
        .iter()
        .find(|text| text.text == "Product launch")
        .unwrap();
    let title_center2 = 2 * title.at.x + title.text.width() as i32 - 1;
    assert!(
        (title_center2 - 2 * spine_x).abs() <= 1,
        "title must center on the chronological spine"
    );

    let sectioned = timeline::parse(
        "timeline\n  title Delivery roadmap\n  section Foundation\n  Q1 : Parser\n  Q2 : Renderer\n  section Adoption\n  Q3 : Documentation\n  Q4 : Release\n",
    )
    .unwrap();
    let scene = timeline::scene(&sectioned, 100);
    let spine_x = scene.paths[0].points[0].x;
    let title = scene
        .texts
        .iter()
        .find(|text| text.text == "Delivery roadmap")
        .unwrap();
    let title_center2 = 2 * title.at.x + title.text.width() as i32 - 1;
    assert!((title_center2 - 2 * spine_x).abs() <= 1);
}

#[test]
fn timeline_labels_have_visible_connector_padding_and_period_connectors_only_share_the_spine() {
    let diagram = timeline::parse(
        "timeline\n  section Foundation\n  Q1 : Design : Prototype\n  Q2 : Build\n",
    )
    .unwrap();
    let scene = timeline::scene(&diagram, 100);
    assert!(
        scene.boxes.is_empty(),
        "timeline labels should stay compact plain text"
    );
    assert_eq!(scene.groups.len(), 1);
    assert_eq!(scene.paths.len(), 1);
    let spine_x = scene.paths[0].points[0].x;
    assert!(scene.paths[0].points.iter().all(|point| point.x == spine_x));

    // Comfortable-width lowering emits period then event text for each rank.
    let labels: Vec<_> = scene
        .texts
        .iter()
        .filter(|text| text.text != "Foundation")
        .collect();
    assert_eq!(
        labels
            .iter()
            .map(|text| text.text.as_str())
            .collect::<Vec<_>>(),
        ["Q1", "Design", "Prototype", "Q2", "Build"]
    );
    let leading_edges: Vec<_> = scene
        .edges
        .iter()
        .filter(|edge| {
            edge.points.last().unwrap().x == spine_x && edge.points.first().unwrap().x < spine_x
        })
        .collect();
    assert_eq!(leading_edges.len(), 2);
    for (text, edge) in [labels[0], labels[3]].into_iter().zip(leading_edges) {
        assert!(text.at.x + (text.text.width() as i32) < edge.points[0].x);
    }
    let trailing_edges: Vec<_> = scene
        .edges
        .iter()
        .filter(|edge| {
            edge.points.first().unwrap().x == spine_x && edge.points.last().unwrap().x > spine_x
        })
        .collect();
    for (text, edge) in [labels[1], labels[2], labels[4]]
        .into_iter()
        .zip(trailing_edges)
    {
        assert!(text.at.x >= edge.points.last().unwrap().x + 2);
    }

    let group = &scene.groups[0];
    for text in &scene.texts {
        for cell in text_cells(text) {
            assert!(
                group.rect.contains(cell),
                "{} escaped its section",
                text.text
            );
        }
    }
    for edge in &scene.edges {
        for cell in path_cells(&edge.points) {
            assert!(cell.x > group.rect.x && cell.x < group.rect.right() - 1);
            assert!(cell.y > group.title.at.y && cell.y < group.rect.bottom() - 1);
        }
    }
    assert!(scene.edge_box_intersections().is_empty());

    let q1_y = scene.edges[0].points.last().unwrap().y;
    let q2_edge = scene
        .edges
        .iter()
        .find(|edge| {
            edge.points.last().unwrap().x == spine_x && edge.points.last().unwrap().y > q1_y
        })
        .unwrap();
    for left in &scene.edges[..3] {
        for right in scene
            .edges
            .iter()
            .filter(|edge| edge.points.last().unwrap().y >= q2_edge.points.last().unwrap().y)
        {
            for cell in path_cells(&left.points) {
                if path_cells(&right.points).contains(&cell) {
                    assert_eq!(
                        cell.x, spine_x,
                        "different periods overlap away from the spine"
                    );
                }
            }
        }
    }
}

fn rect_corners(rect: llmaid::scene::Rect) -> [llmaid::scene::Point; 4] {
    use llmaid::scene::Point;
    [
        Point::new(rect.x, rect.y),
        Point::new(rect.right() - 1, rect.y),
        Point::new(rect.x, rect.bottom() - 1),
        Point::new(rect.right() - 1, rect.bottom() - 1),
    ]
}

fn text_cells(text: &llmaid::scene::SceneText) -> Vec<llmaid::scene::Point> {
    use llmaid::scene::Point;
    (0..text.text.width() as i32)
        .map(|dx| Point::new(text.at.x + dx, text.at.y))
        .collect()
}

fn path_cells(points: &[llmaid::scene::Point]) -> Vec<llmaid::scene::Point> {
    use llmaid::scene::Point;
    let mut cells = Vec::new();
    for pair in points.windows(2) {
        let mut point = pair[0];
        if cells.last() != Some(&point) {
            cells.push(point);
        }
        while point != pair[1] {
            point = Point::new(
                point.x + (pair[1].x - point.x).signum(),
                point.y + (pair[1].y - point.y).signum(),
            );
            cells.push(point);
        }
    }
    cells
}
