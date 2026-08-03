//! Semantic quality checks over the final normalized [`Scene`](crate::scene::Scene).
//!
//! Layout engines are deliberately not consulted here. Diagram IR establishes
//! which relationships are meaningful; final scene geometry establishes
//! whether each relationship was rendered faithfully. Checks report exact
//! per-instance witnesses and applicability instead of a scalar beauty score.

use crate::class::{ClassDiagram, RelationKind};
use crate::diagram::Diagram;
use crate::er::{AttributeKey, ErDiagram};
use crate::mindmap::Mindmap;
use crate::parse::{Dir, Endpoint as FlowEndpoint, Graph};
use crate::scene::{
    EndpointDecorationKind, Point, Rect, RoutedEdge, Scene, SceneBox, SceneText, Shape,
};
use crate::sequence::{SequenceDiagram, SequenceEvent};
use crate::state::{Endpoint, StateDiagram};
use crate::style::Style;
use crate::timeline::Timeline;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckClass {
    Invariant,
    Preference,
    Budget,
}

impl CheckClass {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Invariant => "invariant",
            Self::Preference => "preference",
            Self::Budget => "budget",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WitnessValue {
    Integer(i64),
    Text(String),
    Point(Point),
    Rect(Rect),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WitnessField {
    pub name: &'static str,
    pub value: WitnessValue,
}

impl WitnessField {
    fn integer(name: &'static str, value: impl Into<i64>) -> Self {
        Self {
            name,
            value: WitnessValue::Integer(value.into()),
        }
    }

    fn text(name: &'static str, value: impl Into<String>) -> Self {
        Self {
            name,
            value: WitnessValue::Text(value.into()),
        }
    }

    fn point(name: &'static str, value: Point) -> Self {
        Self {
            name,
            value: WitnessValue::Point(value),
        }
    }

    fn rect(name: &'static str, value: Rect) -> Self {
        Self {
            name,
            value: WitnessValue::Rect(value),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckFailure {
    pub elements: Vec<String>,
    pub message: String,
    pub witness: Vec<WitnessField>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckReport {
    pub id: &'static str,
    pub class: CheckClass,
    pub applicable: usize,
    pub failures: Vec<CheckFailure>,
}

impl CheckReport {
    fn new(id: &'static str, class: CheckClass, applicable: usize) -> Self {
        Self {
            id,
            class,
            applicable,
            failures: Vec::new(),
        }
    }

    pub fn status(&self) -> &'static str {
        if !self.failures.is_empty() {
            "fail"
        } else if self.applicable == 0 {
            "not_applicable"
        } else {
            "pass"
        }
    }

    fn fail(
        &mut self,
        elements: impl IntoIterator<Item = String>,
        message: impl Into<String>,
        witness: Vec<WitnessField>,
    ) {
        self.failures.push(CheckFailure {
            elements: elements.into_iter().collect(),
            message: message.into(),
            witness,
        });
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnclassifiedComposition {
    pub kind: &'static str,
    pub elements: Vec<String>,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QualityReport {
    pub checks: Vec<CheckReport>,
    pub unclassified: Vec<UnclassifiedComposition>,
}

impl QualityReport {
    pub fn failed_checks(&self) -> usize {
        self.checks
            .iter()
            .filter(|check| !check.failures.is_empty())
            .count()
    }

    pub fn applicable_instances(&self) -> usize {
        self.checks.iter().map(|check| check.applicable).sum()
    }

    pub fn has_failures(&self) -> bool {
        self.failed_checks() != 0
    }

    /// Number of failed invariant or preference checks.
    ///
    /// Budget failures remain visible diagnostics, but do not imply that a
    /// renderer violated semantic fidelity or a declared layout preference.
    pub fn quality_failed_checks(&self) -> usize {
        self.checks
            .iter()
            .filter(|check| check.class != CheckClass::Budget && !check.failures.is_empty())
            .count()
    }

    pub fn has_quality_failures(&self) -> bool {
        self.quality_failed_checks() != 0
    }

    pub fn invariant_failed_checks(&self) -> usize {
        self.checks
            .iter()
            .filter(|check| check.class == CheckClass::Invariant && !check.failures.is_empty())
            .count()
    }

    pub fn failures(&self) -> impl Iterator<Item = (&CheckReport, &CheckFailure)> {
        self.checks
            .iter()
            .flat_map(|check| check.failures.iter().map(move |failure| (check, failure)))
    }

    pub fn quality_failures(&self) -> impl Iterator<Item = (&CheckReport, &CheckFailure)> {
        self.failures()
            .filter(|(check, _)| check.class != CheckClass::Budget)
    }

    pub fn invariant_failures(&self) -> impl Iterator<Item = (&CheckReport, &CheckFailure)> {
        self.failures()
            .filter(|(check, _)| check.class == CheckClass::Invariant)
    }
}

/// Evaluate every implemented invariant, preference, and viewport check.
///
/// The scene is normalized before inspection so every witness uses the same
/// coordinates as the terminal canvas and machine inspection report.
pub fn evaluate(diagram: &Diagram, scene: &Scene, target_width: usize) -> QualityReport {
    evaluate_with_style(diagram, scene, target_width, Style { ascii: false })
}

/// Evaluate quality while checking the raster with the requested glyph style.
pub fn evaluate_with_style(
    diagram: &Diagram,
    scene: &Scene,
    target_width: usize,
    style: Style,
) -> QualityReport {
    let mut scene = scene.clone();
    scene.normalize();
    let mut report = QualityReport::default();
    report.checks.push(scene_integrity(&scene, style));
    report.checks.push(width_budget(&scene, target_width));

    match diagram {
        Diagram::Flowchart(graph) => flowchart_checks(graph, &scene, &mut report),
        Diagram::Sequence(sequence) => sequence_checks(sequence, &scene, &mut report),
        Diagram::State(state) => state_checks(state, &scene, &mut report),
        Diagram::Class(class) => class_checks(class, &scene, &mut report),
        Diagram::Er(er) => er_checks(er, &scene, &mut report),
        Diagram::Mindmap(mindmap) => mindmap_checks(mindmap, &scene, &mut report),
        Diagram::Timeline(timeline) => timeline_checks(timeline, &scene, &mut report),
    }
    report
}

fn scene_integrity(scene: &Scene, style: Style) -> CheckReport {
    let (_, failures) = crate::render::render_scene_with_checks(scene, style);
    let mut check = CheckReport::new("scene.integrity", CheckClass::Invariant, 1);
    for message in failures {
        check.fail(
            Vec::new(),
            message.clone(),
            vec![WitnessField::text("detail", message)],
        );
    }
    check
}

fn width_budget(scene: &Scene, target_width: usize) -> CheckReport {
    let rendered_width = scene.bounds().w.max(0) as usize;
    let mut check = CheckReport::new("viewport.width", CheckClass::Budget, 1);
    if rendered_width > target_width {
        check.fail(
            Vec::new(),
            format!(
                "rendered width {rendered_width} exceeds target width {target_width} by {} columns",
                rendered_width - target_width
            ),
            vec![
                WitnessField::integer("target_width", target_width as i64),
                WitnessField::integer("rendered_width", rendered_width as i64),
                WitnessField::integer(
                    "overflow_columns",
                    rendered_width.saturating_sub(target_width) as i64,
                ),
            ],
        );
    }
    check
}

fn flowchart_checks(graph: &Graph, scene: &Scene, report: &mut QualityReport) {
    report.checks.push(edge_endpoint_check(
        "flow.edge_endpoints",
        flow_endpoints(graph),
        scene,
    ));
    let topology = FlowTopology::new(graph, scene);
    report
        .checks
        .push(flow_mono_centerlines(graph, scene, &topology));
    let (forks, fork_covered) = flow_forks(graph, scene, &topology);
    report.checks.push(forks);
    let (merges, merge_covered) = flow_merges(graph, scene, &topology);
    report.checks.push(merges);
    report.checks.push(flow_diamonds(graph, scene, &topology));
    report.checks.push(flow_crossings(graph, scene));
    report.checks.push(flow_groups(graph, scene));

    for node in 0..graph.nodes.len() {
        if topology.outgoing[node].len() >= 2 && !fork_covered[node] {
            report.unclassified.push(UnclassifiedComposition {
                kind: "flow.composite_fork",
                elements: std::iter::once(node_ref(graph, node))
                    .chain(topology.outgoing[node].iter().map(|&peer| node_ref(graph, peer)))
                    .collect(),
                message: "fork participates in overlapping or multi-parent topology; simple centering preference is not applied".to_string(),
            });
        }
        if topology.incoming[node].len() >= 2 && !merge_covered[node] {
            report.unclassified.push(UnclassifiedComposition {
                kind: "flow.composite_merge",
                elements: topology.incoming[node]
                    .iter()
                    .map(|&peer| node_ref(graph, peer))
                    .chain(std::iter::once(node_ref(graph, node)))
                    .collect(),
                message: "merge participates in overlapping or multi-child topology; simple centering preference is not applied".to_string(),
            });
        }
    }
    for (edge, value) in graph.edges.iter().enumerate() {
        if !matches!(value.source, FlowEndpoint::Node(_))
            || !matches!(value.target, FlowEndpoint::Node(_))
        {
            continue;
        }
        if value.from == value.to || !topology.forward[edge] {
            report.unclassified.push(UnclassifiedComposition {
                kind: if value.from == value.to {
                    "flow.self_loop_aesthetics"
                } else {
                    "flow.feedback_aesthetics"
                },
                elements: vec![edge_ref(graph, edge)],
                message: "path correctness is checked, but this feedback composition has no exact aesthetic predicate".to_string(),
            });
        }
    }
}

struct FlowTopology {
    incoming: Vec<Vec<usize>>,
    outgoing: Vec<Vec<usize>>,
    incoming_edges: Vec<Vec<usize>>,
    outgoing_edges: Vec<Vec<usize>>,
    forward: Vec<bool>,
}

impl FlowTopology {
    fn new(graph: &Graph, scene: &Scene) -> Self {
        let mut topology = Self {
            incoming: vec![Vec::new(); graph.nodes.len()],
            outgoing: vec![Vec::new(); graph.nodes.len()],
            incoming_edges: vec![Vec::new(); graph.nodes.len()],
            outgoing_edges: vec![Vec::new(); graph.nodes.len()],
            forward: vec![false; graph.edges.len()],
        };
        for (edge, value) in graph.edges.iter().enumerate() {
            if !matches!(value.source, FlowEndpoint::Node(_))
                || !matches!(value.target, FlowEndpoint::Node(_))
            {
                continue;
            }
            if value.from == value.to {
                continue;
            }
            let Some(source) = scene_box(scene, value.from) else {
                continue;
            };
            let Some(target) = scene_box(scene, value.to) else {
                continue;
            };
            if !is_forward(source.rect, target.rect, graph.direction()) {
                continue;
            }
            topology.forward[edge] = true;
            topology.outgoing_edges[value.from].push(edge);
            topology.incoming_edges[value.to].push(edge);
            push_unique(&mut topology.outgoing[value.from], value.to);
            push_unique(&mut topology.incoming[value.to], value.from);
        }
        topology
    }
}

fn flow_mono_centerlines(graph: &Graph, scene: &Scene, topology: &FlowTopology) -> CheckReport {
    let eligible: Vec<usize> = graph
        .edges
        .iter()
        .enumerate()
        .filter_map(|(edge, value)| {
            (topology.forward[edge]
                && topology.outgoing_edges[value.from].len() == 1
                && topology.incoming_edges[value.to].len() == 1
                && topology.incoming[value.from].len() <= 1
                && topology.outgoing[value.to].len() <= 1)
                .then_some(edge)
        })
        .collect();
    let mut check = CheckReport::new(
        "flow.mono_centerline",
        CheckClass::Preference,
        eligible.len(),
    );
    for edge in eligible {
        let value = &graph.edges[edge];
        let source = scene_box(scene, value.from).unwrap().rect;
        let target = scene_box(scene, value.to).unwrap().rect;
        let source_center2 = cross_center2(source, graph.direction());
        let target_center2 = cross_center2(target, graph.direction());
        let residual2 = parity_excess((source_center2 - target_center2) as i64, 2);
        if residual2 != 0 {
            check.fail(
                vec![
                    edge_ref(graph, edge),
                    node_ref(graph, value.from),
                    node_ref(graph, value.to),
                ],
                format!(
                    "{} is not on one avoidably exact centerline",
                    edge_ref(graph, edge)
                ),
                vec![
                    WitnessField::integer("source_center2", source_center2 as i64),
                    WitnessField::integer("target_center2", target_center2 as i64),
                    WitnessField::integer("residual2", residual2 as i64),
                ],
            );
        }
    }
    check
}

fn flow_forks(graph: &Graph, scene: &Scene, topology: &FlowTopology) -> (CheckReport, Vec<bool>) {
    let mut covered = vec![false; graph.nodes.len()];
    let eligible: Vec<usize> = (0..graph.nodes.len())
        .filter(|&node| {
            topology.outgoing[node].len() >= 2
                && topology.outgoing[node].iter().all(|&target| {
                    topology.incoming[target].as_slice() == [node]
                        && topology.incoming_edges[target].len() == 1
                })
        })
        .collect();
    let mut check = CheckReport::new("flow.fork_centered", CheckClass::Preference, eligible.len());
    for node in eligible {
        covered[node] = true;
        let center = cross_center2(scene_box(scene, node).unwrap().rect, graph.direction()) as i64;
        let peers: Vec<i64> = topology.outgoing[node]
            .iter()
            .map(|&peer| {
                cross_center2(scene_box(scene, peer).unwrap().rect, graph.direction()) as i64
            })
            .collect();
        let residual2 = barycenter_residual(center, &peers);
        if residual2 != 0 {
            check.fail(
                std::iter::once(node_ref(graph, node)).chain(
                    topology.outgoing[node]
                        .iter()
                        .map(|&peer| node_ref(graph, peer)),
                ),
                format!(
                    "fork at {} is not centered on its exclusive children",
                    node_ref(graph, node)
                ),
                vec![
                    WitnessField::integer("fork_center2", center),
                    WitnessField::integer("child_center2_sum", peers.iter().sum::<i64>()),
                    WitnessField::integer("degree", peers.len() as i64),
                    WitnessField::integer("residual2", residual2 as i64),
                ],
            );
        }
    }
    (check, covered)
}

fn flow_merges(graph: &Graph, scene: &Scene, topology: &FlowTopology) -> (CheckReport, Vec<bool>) {
    let mut covered = vec![false; graph.nodes.len()];
    let eligible: Vec<usize> = (0..graph.nodes.len())
        .filter(|&node| {
            topology.incoming[node].len() >= 2
                && topology.incoming[node].iter().all(|&source| {
                    topology.outgoing[source].as_slice() == [node]
                        && topology.outgoing_edges[source].len() == 1
                })
        })
        .collect();
    let mut check = CheckReport::new(
        "flow.merge_centered",
        CheckClass::Preference,
        eligible.len(),
    );
    for node in eligible {
        covered[node] = true;
        let center = cross_center2(scene_box(scene, node).unwrap().rect, graph.direction()) as i64;
        let peers: Vec<i64> = topology.incoming[node]
            .iter()
            .map(|&peer| {
                cross_center2(scene_box(scene, peer).unwrap().rect, graph.direction()) as i64
            })
            .collect();
        let residual2 = barycenter_residual(center, &peers);
        if residual2 != 0 {
            check.fail(
                topology.incoming[node]
                    .iter()
                    .map(|&peer| node_ref(graph, peer))
                    .chain(std::iter::once(node_ref(graph, node))),
                format!(
                    "merge at {} is not centered on its exclusive parents",
                    node_ref(graph, node)
                ),
                vec![
                    WitnessField::integer("merge_center2", center),
                    WitnessField::integer("parent_center2_sum", peers.iter().sum::<i64>()),
                    WitnessField::integer("degree", peers.len() as i64),
                    WitnessField::integer("residual2", residual2 as i64),
                ],
            );
        }
    }
    (check, covered)
}

fn flow_diamonds(graph: &Graph, scene: &Scene, topology: &FlowTopology) -> CheckReport {
    let mut motifs = Vec::new();
    for fork in 0..graph.nodes.len() {
        let [left, right] = topology.outgoing[fork].as_slice() else {
            continue;
        };
        if topology.incoming[*left].as_slice() != [fork]
            || topology.incoming[*right].as_slice() != [fork]
            || topology.outgoing[*left].len() != 1
            || topology.outgoing[*right].len() != 1
        {
            continue;
        }
        let merge = topology.outgoing[*left][0];
        if topology.outgoing[*right].as_slice() != [merge]
            || topology.incoming[merge].len() != 2
            || !topology.incoming[merge].contains(left)
            || !topology.incoming[merge].contains(right)
        {
            continue;
        }
        motifs.push((fork, *left, *right, merge));
    }
    let mut check = CheckReport::new("flow.diamond_mirror", CheckClass::Preference, motifs.len());
    for (fork, left, right, merge) in motifs {
        let centers = [fork, left, right, merge].map(|node| {
            cross_center2(scene_box(scene, node).unwrap().rect, graph.direction()) as i64
        });
        let raw = centers[0].abs_diff(centers[3]) as usize
            + (centers[1] + centers[2]).abs_diff(centers[0] + centers[3]) as usize;
        let residual2 = raw.saturating_sub(diamond_parity_floor(
            centers[0],
            centers[3],
            centers[1] + centers[2],
        ));
        if residual2 != 0 {
            check.fail(
                [fork, left, right, merge].map(|node| node_ref(graph, node)),
                format!(
                    "diamond from {} to {} is not mirrored",
                    node_ref(graph, fork),
                    node_ref(graph, merge)
                ),
                vec![
                    WitnessField::integer("fork_center2", centers[0]),
                    WitnessField::integer("left_center2", centers[1]),
                    WitnessField::integer("right_center2", centers[2]),
                    WitnessField::integer("merge_center2", centers[3]),
                    WitnessField::integer("residual2", residual2 as i64),
                ],
            );
        }
    }
    check
}

fn flow_crossings(graph: &Graph, scene: &Scene) -> CheckReport {
    let mut applicable = 0usize;
    let mut failures = Vec::new();
    for left in 0..scene.edges.len() {
        for right in left + 1..scene.edges.len() {
            let Some(left_semantic) = graph.edges.get(scene.edges[left].edge) else {
                continue;
            };
            let Some(right_semantic) = graph.edges.get(scene.edges[right].edge) else {
                continue;
            };
            if [left_semantic.from, left_semantic.to]
                .iter()
                .any(|node| *node == right_semantic.from || *node == right_semantic.to)
            {
                continue;
            }
            applicable += 1;
            let mut points = Vec::new();
            for a in scene.edges[left].points.windows(2) {
                for b in scene.edges[right].points.windows(2) {
                    if let Some(point) = perpendicular_interior_crossing(a, b)
                        && !points.contains(&point)
                    {
                        points.push(point);
                    }
                }
            }
            for point in points {
                failures.push((scene.edges[left].edge, scene.edges[right].edge, point));
            }
        }
    }
    let mut check = CheckReport::new("flow.edge_crossings", CheckClass::Preference, applicable);
    for (left, right, point) in failures {
        check.fail(
            vec![edge_ref(graph, left), edge_ref(graph, right)],
            format!(
                "unrelated edges {left} and {right} cross at ({},{})",
                point.x, point.y
            ),
            vec![WitnessField::point("at", point)],
        );
    }
    check
}

fn flow_groups(graph: &Graph, scene: &Scene) -> CheckReport {
    let mut check = CheckReport::new(
        "flow.group_containment",
        CheckClass::Invariant,
        graph.subgraphs.len(),
    );
    for (index, subgraph) in graph.subgraphs.iter().enumerate() {
        let Some(group) = scene.groups.iter().find(|group| group.subgraph == index) else {
            check.fail(
                vec![format!("group:{}", subgraph.id)],
                format!("group `{}` has no scene frame", subgraph.id),
                Vec::new(),
            );
            continue;
        };
        for &member in &subgraph.members {
            let Some(box_) = scene_box(scene, member) else {
                continue;
            };
            if !contains_rect(group.rect, box_.rect) {
                check.fail(
                    vec![format!("group:{}", subgraph.id), node_ref(graph, member)],
                    format!(
                        "group `{}` does not contain member `{}`",
                        subgraph.id, graph.nodes[member].id
                    ),
                    vec![
                        WitnessField::rect("group", group.rect),
                        WitnessField::rect("member", box_.rect),
                    ],
                );
            }
        }
        for child in graph
            .subgraphs
            .iter()
            .enumerate()
            .filter_map(|(child, value)| (value.parent == Some(index)).then_some(child))
        {
            if let Some(child_group) = scene.groups.iter().find(|group| group.subgraph == child)
                && !contains_rect(group.rect, child_group.rect)
            {
                check.fail(
                    vec![
                        format!("group:{}", subgraph.id),
                        format!("group:{}", graph.subgraphs[child].id),
                    ],
                    format!(
                        "group `{}` does not contain nested group `{}`",
                        subgraph.id, graph.subgraphs[child].id
                    ),
                    vec![
                        WitnessField::rect("parent", group.rect),
                        WitnessField::rect("child", child_group.rect),
                    ],
                );
            }
        }
    }
    check
}

fn sequence_checks(sequence: &SequenceDiagram, scene: &Scene, report: &mut QualityReport) {
    report.checks.push(sequence_lifelines(sequence, scene));
    report.checks.push(sequence_messages(sequence, scene));
    report.checks.push(sequence_fragments(sequence, scene));
    report
        .checks
        .push(sequence_final_fragment_termination(sequence, scene));
    if sequence
        .events
        .iter()
        .any(|event| matches!(event, SequenceEvent::Note(_) | SequenceEvent::Activation(_)))
    {
        report.unclassified.push(UnclassifiedComposition {
            kind: "sequence.note_activation_composition",
            elements: sequence
                .events
                .iter()
                .enumerate()
                .filter_map(|(event, value)| {
                    matches!(value, SequenceEvent::Note(_) | SequenceEvent::Activation(_))
                        .then_some(format!("event:{event}"))
                })
                .collect(),
            message: "note and activation integrity is checked, but their complete spacing composition is not yet classified by the machine preference audit".to_string(),
        });
    }
}

fn sequence_lifelines(sequence: &SequenceDiagram, scene: &Scene) -> CheckReport {
    let mut check = CheckReport::new(
        "sequence.header_lifeline_alignment",
        CheckClass::Preference,
        sequence.participants.len(),
    );
    for (participant, value) in sequence.participants.iter().enumerate() {
        let Some(header) = scene_box(scene, participant) else {
            check.fail(
                vec![format!("participant:{}", value.id)],
                format!("participant `{}` has no header box", value.id),
                Vec::new(),
            );
            continue;
        };
        let Some(path) = scene.paths.iter().find(|path| path.path == participant) else {
            check.fail(
                vec![format!("participant:{}", value.id)],
                format!("participant `{}` has no lifeline", value.id),
                Vec::new(),
            );
            continue;
        };
        let Some(first) = path.points.first() else {
            continue;
        };
        let residual2 = parity_excess((header.rect.center2().x - 2 * first.x) as i64, 2);
        if residual2 != 0 || first.y != header.rect.bottom() - 1 {
            check.fail(
                vec![
                    format!("participant:{}", value.id),
                    format!("lifeline:{}", value.id),
                ],
                format!(
                    "participant `{}` header and lifeline are not attached on one centerline",
                    value.id
                ),
                vec![
                    WitnessField::rect("header", header.rect),
                    WitnessField::point("lifeline_start", *first),
                    WitnessField::integer("residual2", residual2 as i64),
                ],
            );
        }
    }
    check
}

fn sequence_messages(sequence: &SequenceDiagram, scene: &Scene) -> CheckReport {
    let messages: Vec<(usize, &crate::sequence::Message)> = sequence
        .events
        .iter()
        .enumerate()
        .filter_map(|(event, value)| match value {
            SequenceEvent::Message(message) => Some((event, message)),
            _ => None,
        })
        .collect();
    let mut check = CheckReport::new(
        "sequence.message_geometry",
        CheckClass::Preference,
        messages.len(),
    );
    if scene.edges.len() != messages.len() {
        check.fail(
            Vec::new(),
            "semantic message count does not match routed message count",
            vec![
                WitnessField::integer("expected", messages.len() as i64),
                WitnessField::integer("actual", scene.edges.len() as i64),
            ],
        );
    }
    let mut previous_y = None;
    for (message_index, (event, message)) in messages.into_iter().enumerate() {
        let Some(edge) = scene.edges.get(message_index) else {
            continue;
        };
        let Some(first) = edge.points.first() else {
            continue;
        };
        if let Some(previous) = previous_y
            && first.y <= previous
        {
            check.fail(
                vec![format!("event:{event}"), format!("message:{message_index}")],
                "message rows do not preserve semantic event order",
                vec![
                    WitnessField::integer("previous_y", previous as i64),
                    WitnessField::integer("actual_y", first.y as i64),
                ],
            );
        }
        previous_y = Some(first.y);
        if message.from == message.to {
            continue;
        }
        let Some(arrow) = &edge.arrow else {
            continue;
        };
        let Some(label) = &edge.label else {
            continue;
        };
        let label_center2 = 2 * label.at.x + label.width() as i32 - 1;
        let shaft_center2 = first.x + arrow.toward.x;
        let residual2 = (label_center2 - shaft_center2).unsigned_abs() as usize;
        if residual2 > 1 {
            check.fail(
                vec![
                    format!("event:{event}"),
                    format!("participant:{}", sequence.participants[message.from].id),
                    format!("participant:{}", sequence.participants[message.to].id),
                ],
                format!(
                    "message label `{}` is not centered over its shaft",
                    message.label
                ),
                vec![
                    WitnessField::integer("label_center2", label_center2 as i64),
                    WitnessField::integer("shaft_center2", shaft_center2 as i64),
                    WitnessField::integer("residual2", residual2 as i64),
                ],
            );
        }
    }
    check
}

fn sequence_fragments(sequence: &SequenceDiagram, scene: &Scene) -> CheckReport {
    let starts = sequence
        .controls
        .iter()
        .filter(|control| matches!(control.kind, crate::sequence::ControlKind::Start(_, _)))
        .count();
    let mut check = CheckReport::new(
        "sequence.fragment_lifeline_span",
        CheckClass::Preference,
        starts,
    );
    let lifeline_x: Vec<i32> = scene
        .paths
        .iter()
        .filter_map(|path| path.points.first().map(|point| point.x))
        .collect();
    let Some(first) = lifeline_x.iter().min().copied() else {
        return check;
    };
    let last = lifeline_x.iter().max().copied().unwrap();
    for (index, group) in scene.groups.iter().enumerate() {
        if group.rect.x >= first || group.rect.right() <= last {
            check.fail(
                vec![format!("fragment:{index}")],
                format!(
                    "fragment `{}` does not visibly contain all lifelines",
                    group.title.text
                ),
                vec![
                    WitnessField::rect("fragment", group.rect),
                    WitnessField::integer("first_lifeline_x", first as i64),
                    WitnessField::integer("last_lifeline_x", last as i64),
                ],
            );
        }
    }
    check
}

/// Eligible only when the final semantic control closes the outermost frame at
/// the final event boundary. In that topology the frame is also the diagram's
/// visual terminus, so every lifeline ends on—not below—its bottom border.
fn sequence_final_fragment_termination(sequence: &SequenceDiagram, scene: &Scene) -> CheckReport {
    let eligible = sequence.controls.last().is_some_and(|control| {
        control.at == sequence.events.len()
            && matches!(control.kind, crate::sequence::ControlKind::End)
    });
    let mut check = CheckReport::new(
        "sequence.final_fragment_termination",
        CheckClass::Preference,
        usize::from(eligible),
    );
    if !eligible {
        return check;
    }
    let Some(frame) = scene.groups.last() else {
        check.fail(
            Vec::new(),
            "final semantic sequence fragment has no rendered frame",
            Vec::new(),
        );
        return check;
    };
    let expected_y = frame.rect.bottom() - 1;
    for (participant, path) in sequence.participants.iter().zip(&scene.paths) {
        let actual_y = path.points.last().map_or(i32::MIN, |point| point.y);
        if actual_y != expected_y {
            check.fail(
                vec![
                    format!("participant:{}", participant.id),
                    format!("lifeline:{}", participant.id),
                ],
                format!(
                    "final lifeline `{}` does not terminate on the outer fragment border",
                    participant.id
                ),
                vec![
                    WitnessField::integer("expected_y", expected_y as i64),
                    WitnessField::integer("actual_y", actual_y as i64),
                    WitnessField::rect("fragment", frame.rect),
                ],
            );
        }
    }
    check
}

fn state_checks(state: &StateDiagram, scene: &Scene, report: &mut QualityReport) {
    let endpoints = state_endpoints(state);
    report.checks.push(edge_endpoint_check(
        "state.transition_endpoints",
        endpoints,
        scene,
    ));
    let mut shapes = CheckReport::new(
        "state.node_shapes",
        CheckClass::Preference,
        scene.boxes.len(),
    );
    for (index, box_) in scene.boxes.iter().enumerate() {
        let expected = if index < state.states.len() {
            Shape::Rounded
        } else {
            Shape::Circle
        };
        if box_.shape != expected {
            shapes.fail(
                vec![state_box_ref(state, index)],
                format!(
                    "{} has shape {} instead of {}",
                    state_box_ref(state, index),
                    box_.shape.name(),
                    expected.name()
                ),
                vec![
                    WitnessField::text("expected", expected.name()),
                    WitnessField::text("actual", box_.shape.name()),
                ],
            );
        }
    }
    report.checks.push(shapes);
    if !state.transitions.is_empty() {
        report.unclassified.push(UnclassifiedComposition {
            kind: "state.boxed_layout_composition",
            elements: (0..state.transitions.len())
                .map(|transition| format!("transition:{transition}"))
                .collect(),
            message: "transition endpoints are verified; alignment and spacing still use generic boxed layout without a state-specific preference predicate".to_string(),
        });
    }
}

fn class_checks(class: &ClassDiagram, scene: &Scene, report: &mut QualityReport) {
    report.checks.push(edge_endpoint_check(
        "class.relation_endpoints",
        class
            .relations
            .iter()
            .enumerate()
            .map(|(edge, relation)| EndpointExpectation {
                edge,
                source: EndpointGeometry::Box(relation.from),
                target: EndpointGeometry::Box(relation.to),
                elements: vec![
                    format!("relation:{edge}"),
                    format!("class:{}", class.classes[relation.from].id),
                    format!("class:{}", class.classes[relation.to].id),
                ],
            })
            .collect(),
        scene,
    ));
    report.checks.push(class_tables(class, scene));
    report.checks.push(class_decorations(class, scene));
    report.checks.push(class_multiplicities(class, scene));
    if !class.relations.is_empty() {
        report.unclassified.push(UnclassifiedComposition {
            kind: "class.boxed_layout_composition",
            elements: (0..class.relations.len()).map(|edge| format!("relation:{edge}")).collect(),
            message: "structured boxes and endpoint notation are verified; boxed-graph alignment and spacing are not yet classified by a class-specific predicate".to_string(),
        });
    }
}

fn class_tables(class: &ClassDiagram, scene: &Scene) -> CheckReport {
    let mut check = CheckReport::new(
        "class.compartments",
        CheckClass::Preference,
        class.classes.len(),
    );
    for (index, value) in class.classes.iter().enumerate() {
        let Some(box_) = scene_box(scene, index) else {
            continue;
        };
        let expected_rows: Vec<Vec<String>> = value
            .members
            .iter()
            .map(|member| vec![member.clone()])
            .collect();
        let actual = box_.table.as_ref().map_or(0, |table| table.rows.len());
        let exact = box_.table.as_ref().is_some_and(|table| {
            table.title == value.id && table.rows == expected_rows && !table.row_dividers
        });
        if box_.shape != Shape::Rounded || !exact {
            check.fail(
                vec![format!("class:{}", value.id)],
                format!(
                    "class `{}` compartments do not match its semantic members",
                    value.id
                ),
                vec![
                    WitnessField::integer("expected_rows", value.members.len() as i64),
                    WitnessField::integer("actual_rows", actual as i64),
                    WitnessField::text("expected_shape", Shape::Rounded.name()),
                    WitnessField::text("actual_shape", box_.shape.name()),
                    WitnessField::rect("box", box_.rect),
                ],
            );
        }
    }
    check
}

fn class_decorations(class: &ClassDiagram, scene: &Scene) -> CheckReport {
    let expected: Vec<(usize, bool, EndpointDecorationKind)> = class
        .relations
        .iter()
        .enumerate()
        .filter_map(|(edge, relation)| {
            let value = match relation.kind {
                RelationKind::Inheritance => (true, EndpointDecorationKind::OpenTriangle),
                RelationKind::Composition => (true, EndpointDecorationKind::FilledDiamond),
                RelationKind::Aggregation => (true, EndpointDecorationKind::OpenDiamond),
                RelationKind::Association | RelationKind::Dependency => {
                    (false, EndpointDecorationKind::OpenArrow)
                }
                RelationKind::Realization => (false, EndpointDecorationKind::OpenTriangle),
                RelationKind::Link => return None,
            };
            Some((edge, value.0, value.1))
        })
        .collect();
    let mut check = CheckReport::new(
        "class.endpoint_decorations",
        CheckClass::Preference,
        expected.len(),
    );
    for (edge, source_end, kind) in expected {
        let relation = &class.relations[edge];
        let box_index = if source_end {
            relation.from
        } else {
            relation.to
        };
        let decoration = scene
            .endpoint_decorations
            .iter()
            .find(|value| value.edge == edge);
        let Some(decoration) = decoration else {
            check.fail(
                vec![format!("relation:{edge}")],
                format!("relation {edge} is missing endpoint decoration"),
                vec![WitnessField::text("expected", decoration_kind_name(kind))],
            );
            continue;
        };
        let box_rect = scene_box(scene, box_index).unwrap().rect;
        let distance = manhattan(decoration.at, decoration.toward);
        if decoration.kind != kind || distance != 2 || !box_rect.contains(decoration.toward) {
            check.fail(
                vec![format!("relation:{edge}"), format!("class:{}", class.classes[box_index].id)],
                format!("relation {edge} decoration is not the expected endpoint notation with one connector cell"),
                vec![
                    WitnessField::text("expected_kind", decoration_kind_name(kind)),
                    WitnessField::text("actual_kind", decoration_kind_name(decoration.kind)),
                    WitnessField::integer("distance", distance as i64),
                    WitnessField::point("at", decoration.at),
                    WitnessField::point("toward", decoration.toward),
                ],
            );
        }
    }
    check
}

fn class_multiplicities(class: &ClassDiagram, scene: &Scene) -> CheckReport {
    let expected: Vec<(usize, &str)> = class
        .relations
        .iter()
        .enumerate()
        .flat_map(|(edge, relation)| {
            relation
                .from_multiplicity
                .iter()
                .map(move |value| (edge, value.as_str()))
                .chain(
                    relation
                        .to_multiplicity
                        .iter()
                        .map(move |value| (edge, value.as_str())),
                )
        })
        .collect();
    let mut check = CheckReport::new(
        "class.multiplicity_labels",
        CheckClass::Preference,
        expected.len(),
    );
    let mut used = vec![false; scene.texts.len()];
    for (edge, expected) in expected {
        let found = scene
            .texts
            .iter()
            .enumerate()
            .find(|(index, text)| !used[*index] && text.text == expected)
            .map(|(index, _)| index);
        if let Some(index) = found {
            used[index] = true;
        } else {
            check.fail(
                vec![format!("relation:{edge}")],
                format!("relation {edge} is missing multiplicity `{expected}`"),
                vec![WitnessField::text("expected", expected)],
            );
        }
    }
    check
}

fn er_checks(er: &ErDiagram, scene: &Scene, report: &mut QualityReport) {
    report.checks.push(edge_endpoint_check(
        "er.relationship_endpoints",
        er.relationships
            .iter()
            .enumerate()
            .map(|(edge, relation)| EndpointExpectation {
                edge,
                source: EndpointGeometry::Box(relation.from),
                target: EndpointGeometry::Box(relation.to),
                elements: vec![
                    format!("relationship:{edge}"),
                    format!("entity:{}", er.entities[relation.from].id),
                    format!("entity:{}", er.entities[relation.to].id),
                ],
            })
            .collect(),
        scene,
    ));
    report.checks.push(er_tables(er, scene));
    report.checks.push(er_cardinalities(er, scene));
    report.checks.push(er_relationship_labels(er, scene));
    report.checks.push(er_cardinality_lanes(er, scene));
    if !er.relationships.is_empty() {
        report.unclassified.push(UnclassifiedComposition {
            kind: "er.boxed_layout_composition",
            elements: (0..er.relationships.len())
                .map(|edge| format!("relationship:{edge}"))
                .collect(),
            message: "entity tables, endpoint cardinalities, label attachment, and shared terminal-lane separation are verified; remaining boxed-graph alignment is not yet classified by an ER-specific predicate".to_string(),
        });
    }
}

fn er_tables(er: &ErDiagram, scene: &Scene) -> CheckReport {
    let mut check = CheckReport::new(
        "er.attribute_tables",
        CheckClass::Preference,
        er.entities.len(),
    );
    for (index, entity) in er.entities.iter().enumerate() {
        let Some(box_) = scene_box(scene, index) else {
            continue;
        };
        let expected_rows: Vec<Vec<String>> = entity
            .attributes
            .iter()
            .map(|attribute| {
                vec![
                    attribute.data_type.clone(),
                    attribute.name.clone(),
                    attribute
                        .keys
                        .iter()
                        .map(|key| match key {
                            AttributeKey::Primary => "PK",
                            AttributeKey::Foreign => "FK",
                            AttributeKey::Unique => "UK",
                        })
                        .collect::<Vec<_>>()
                        .join(" "),
                    attribute.comment.clone().unwrap_or_default(),
                ]
            })
            .collect();
        let actual = box_.table.as_ref().map_or(0, |table| table.rows.len());
        let exact = box_.table.as_ref().is_some_and(|table| {
            table.title == entity.label && table.rows == expected_rows && table.row_dividers
        });
        if box_.shape != Shape::Rounded || !exact {
            check.fail(
                vec![format!("entity:{}", entity.id)],
                format!(
                    "entity `{}` table does not match its semantic attributes",
                    entity.id
                ),
                vec![
                    WitnessField::integer("expected_rows", entity.attributes.len() as i64),
                    WitnessField::integer("actual_rows", actual as i64),
                    WitnessField::text("expected_shape", Shape::Rounded.name()),
                    WitnessField::text("actual_shape", box_.shape.name()),
                    WitnessField::rect("box", box_.rect),
                ],
            );
        }
    }
    check
}

fn er_cardinalities(er: &ErDiagram, scene: &Scene) -> CheckReport {
    let mut check = CheckReport::new(
        "er.cardinality_endpoints",
        CheckClass::Preference,
        er.relationships.len() * 2,
    );
    for (edge, relationship) in er.relationships.iter().enumerate() {
        let values = [
            (
                relationship.from,
                cardinality_kind(&relationship.left_cardinality),
                "source",
            ),
            (
                relationship.to,
                cardinality_kind(&relationship.right_cardinality),
                "target",
            ),
        ];
        let decorations: Vec<_> = scene
            .endpoint_decorations
            .iter()
            .filter(|value| value.edge == edge)
            .collect();
        for (ordinal, (box_index, kind, end)) in values.into_iter().enumerate() {
            let decoration = decorations.get(ordinal).copied();
            let Some(decoration) = decoration else {
                check.fail(
                    vec![
                        format!("relationship:{edge}"),
                        format!("entity:{}", er.entities[box_index].id),
                    ],
                    format!("relationship {edge} is missing its {end} cardinality"),
                    vec![WitnessField::text("expected", decoration_kind_name(kind))],
                );
                continue;
            };
            let box_rect = scene_box(scene, box_index).unwrap().rect;
            let distance = manhattan(decoration.at, decoration.toward);
            let path_cells = scene
                .edges
                .iter()
                .find(|candidate| candidate.edge == edge)
                .map(|candidate| crate::scene::path_cells(&candidate.points))
                .unwrap_or_default();
            let off_path = decoration
                .paint_cells()
                .into_iter()
                .filter(|cell| !path_cells.contains(cell))
                .count();
            if decoration.kind != kind
                || distance != 2
                || !box_rect.contains(decoration.toward)
                || off_path != 0
            {
                check.fail(
                    vec![
                        format!("relationship:{edge}"),
                        format!("entity:{}", er.entities[box_index].id),
                    ],
                    format!("relationship {edge} {end} cardinality is not correctly attached"),
                    vec![
                        WitnessField::text("expected_kind", decoration_kind_name(kind)),
                        WitnessField::text("actual_kind", decoration_kind_name(decoration.kind)),
                        WitnessField::integer("distance", distance as i64),
                        WitnessField::point("at", decoration.at),
                        WitnessField::point("toward", decoration.toward),
                        WitnessField::integer("off_path_cells", off_path as i64),
                    ],
                );
            }
        }
    }
    check
}

fn er_relationship_labels(er: &ErDiagram, scene: &Scene) -> CheckReport {
    let mut check = CheckReport::new(
        "er.relationship_label_attachment",
        CheckClass::Preference,
        er.relationships.len(),
    );
    for (edge_index, relationship) in er.relationships.iter().enumerate() {
        let Some(edge) = scene.edges.iter().find(|edge| edge.edge == edge_index) else {
            continue;
        };
        let Some(label) = &edge.label else {
            check.fail(
                vec![format!("relationship:{edge_index}")],
                format!(
                    "relationship `{}` has no rendered label",
                    relationship.label
                ),
                Vec::new(),
            );
            continue;
        };
        let bounds = label.bounds();
        let nearest = crate::scene::path_cells(&edge.points)
            .into_iter()
            .map(|point| point_rect_distance(point, bounds))
            .min()
            .unwrap_or(usize::MAX);
        if nearest > 1 {
            check.fail(
                vec![format!("relationship:{edge_index}")],
                format!(
                    "relationship label `{}` is detached from its routed path",
                    relationship.label
                ),
                vec![
                    WitnessField::rect("label", bounds),
                    WitnessField::integer("nearest_path_distance", nearest as i64),
                ],
            );
        }
    }
    check
}

/// Eligible for each pair of relationship endpoints attached to the same
/// entity. Paint cells must remain disjoint so every semantic relationship has
/// a visible, independently readable cardinality lane.
fn er_cardinality_lanes(er: &ErDiagram, scene: &Scene) -> CheckReport {
    let mut shared = Vec::new();
    for left in 0..er.relationships.len() {
        for right in left + 1..er.relationships.len() {
            let a = &er.relationships[left];
            let b = &er.relationships[right];
            for entity in 0..er.entities.len() {
                let a_end = (a.from == entity)
                    .then_some(0)
                    .or((a.to == entity).then_some(1));
                let b_end = (b.from == entity)
                    .then_some(0)
                    .or((b.to == entity).then_some(1));
                if let (Some(a_end), Some(b_end)) = (a_end, b_end) {
                    shared.push((left, a_end, right, b_end, entity));
                }
            }
        }
    }
    let mut check = CheckReport::new(
        "er.cardinality_lane_separation",
        CheckClass::Preference,
        shared.len(),
    );
    for (left, left_end, right, right_end, entity) in shared {
        let decorations = |edge: usize| {
            scene
                .endpoint_decorations
                .iter()
                .filter(|decoration| decoration.edge == edge)
                .collect::<Vec<_>>()
        };
        let left_decorations = decorations(left);
        let right_decorations = decorations(right);
        let (Some(a), Some(b)) = (
            left_decorations.get(left_end),
            right_decorations.get(right_end),
        ) else {
            continue;
        };
        let a_cells = a.paint_cells();
        let b_cells = b.paint_cells();
        let overlap = a_cells.iter().filter(|cell| b_cells.contains(cell)).count();
        if overlap != 0 {
            check.fail(
                vec![
                    format!("relationship:{left}"),
                    format!("relationship:{right}"),
                    format!("entity:{}", er.entities[entity].id),
                ],
                "converging ER cardinalities occupy the same terminal lane",
                vec![
                    WitnessField::integer("overlap_cells", overlap as i64),
                    WitnessField::point("left_anchor", a.at),
                    WitnessField::point("right_anchor", b.at),
                ],
            );
        }
    }
    check
}

fn mindmap_checks(mindmap: &Mindmap, scene: &Scene, report: &mut QualityReport) {
    report.checks.push(mindmap_parent_spans(mindmap, scene));
    report.checks.push(mindmap_edge_attachments(mindmap, scene));
    report.checks.push(mindmap_box_padding(mindmap, scene));
    report.checks.push(mindmap_depth_columns(mindmap, scene));
}

fn mindmap_parent_spans(mindmap: &Mindmap, scene: &Scene) -> CheckReport {
    let parents: Vec<(usize, Vec<usize>)> = (0..mindmap.nodes.len())
        .filter_map(|parent| {
            let children: Vec<usize> = mindmap
                .nodes
                .iter()
                .enumerate()
                .filter_map(|(child, value)| (value.parent == Some(parent)).then_some(child))
                .collect();
            (!children.is_empty()).then_some((parent, children))
        })
        .collect();
    let mut check = CheckReport::new(
        "mindmap.parent_child_span",
        CheckClass::Preference,
        parents.len(),
    );
    for (parent, children) in parents {
        let parent_center2 = scene_box(scene, parent).unwrap().rect.center2().y;
        let first_center2 = scene_box(scene, children[0]).unwrap().rect.center2().y;
        let last_center2 = scene_box(scene, *children.last().unwrap())
            .unwrap()
            .rect
            .center2()
            .y;
        let expected_sum = first_center2 + last_center2;
        let actual_sum = 2 * parent_center2;
        if actual_sum != expected_sum {
            check.fail(
                std::iter::once(mindmap_node_ref(parent))
                    .chain(children.iter().map(|&child| mindmap_node_ref(child))),
                format!("mindmap parent {parent} is not centered on its ordered child span"),
                vec![
                    WitnessField::integer("parent_center2_times_2", actual_sum as i64),
                    WitnessField::integer("first_last_center2_sum", expected_sum as i64),
                    WitnessField::integer("residual2", actual_sum.abs_diff(expected_sum) as i64),
                ],
            );
        }
    }
    check
}

fn mindmap_edge_attachments(mindmap: &Mindmap, scene: &Scene) -> CheckReport {
    let mut check = CheckReport::new(
        "mindmap.edge_attachments",
        CheckClass::Preference,
        mindmap.nodes.len().saturating_sub(1),
    );
    for edge in &scene.edges {
        let child = edge.edge + 1;
        let Some(node) = mindmap.nodes.get(child) else {
            continue;
        };
        let Some(parent) = node.parent else {
            continue;
        };
        let source = scene_box(scene, parent).unwrap().rect;
        let target = scene_box(scene, child).unwrap().rect;
        let Some(first) = edge.points.first() else {
            continue;
        };
        let Some(last) = edge.points.last() else {
            continue;
        };
        if first.x != source.right() - 1
            || 2 * first.y != source.center2().y
            || last.x != target.x
            || 2 * last.y != target.center2().y
            || edge.arrow.is_some()
        {
            check.fail(
                vec![
                    mindmap_node_ref(parent),
                    mindmap_node_ref(child),
                    format!("edge:{}", edge.edge),
                ],
                format!(
                    "mindmap edge {} is not attached to parent and child center rows",
                    edge.edge
                ),
                vec![
                    WitnessField::point("source_attachment", *first),
                    WitnessField::point("target_attachment", *last),
                    WitnessField::rect("parent", source),
                    WitnessField::rect("child", target),
                ],
            );
        }
    }
    check
}

fn mindmap_box_padding(mindmap: &Mindmap, scene: &Scene) -> CheckReport {
    let mut check = CheckReport::new(
        "mindmap.box_padding",
        CheckClass::Preference,
        mindmap.nodes.len(),
    );
    for (node, box_) in scene.boxes.iter().enumerate() {
        let content = box_
            .lines
            .iter()
            .map(|line| line.width())
            .max()
            .unwrap_or(0) as i32;
        if box_.rect.w < content + 4 {
            check.fail(
                vec![mindmap_node_ref(node)],
                format!("mindmap node {node} lacks one visible padding cell beside its label"),
                vec![
                    WitnessField::integer("content_width", content as i64),
                    WitnessField::integer("box_width", box_.rect.w as i64),
                ],
            );
        }
    }
    check
}

fn mindmap_depth_columns(mindmap: &Mindmap, scene: &Scene) -> CheckReport {
    let levels = mindmap.levels();
    let mut check = CheckReport::new("mindmap.depth_columns", CheckClass::Preference, levels);
    let mut previous_right = None;
    for depth in 0..levels {
        let boxes: Vec<Rect> = mindmap
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, value)| value.depth == depth)
            .map(|(node, _)| scene_box(scene, node).unwrap().rect)
            .collect();
        let Some(first) = boxes.first().copied() else {
            continue;
        };
        let consistent = boxes
            .iter()
            .all(|rect| rect.x == first.x && rect.w == first.w);
        let separated = previous_right.is_none_or(|right| first.x > right);
        if !consistent || !separated {
            check.fail(
                vec![format!("depth:{depth}")],
                format!("mindmap depth {depth} is not one stable separated column"),
                vec![WitnessField::rect("first_box", first)],
            );
        }
        previous_right = Some(first.right() - 1);
    }
    check
}

fn timeline_checks(timeline: &Timeline, scene: &Scene, report: &mut QualityReport) {
    report.checks.push(timeline_spine(timeline, scene));
    report.checks.push(timeline_chronology(timeline, scene));
    report
        .checks
        .push(timeline_connector_padding(timeline, scene));
    report.checks.push(timeline_sections(timeline, scene));
    report.checks.push(timeline_title(timeline, scene));
}

fn timeline_spine(timeline: &Timeline, scene: &Scene) -> CheckReport {
    let mut check = CheckReport::new(
        "timeline.common_spine",
        CheckClass::Preference,
        usize::from(!timeline.periods.is_empty()),
    );
    let Some(spine_x) = scene
        .paths
        .first()
        .and_then(|path| path.points.first())
        .map(|point| point.x)
    else {
        if !timeline.periods.is_empty() {
            check.fail(
                Vec::new(),
                "timeline has no chronological spine",
                Vec::new(),
            );
        }
        return check;
    };
    if scene
        .paths
        .iter()
        .flat_map(|path| &path.points)
        .any(|point| point.x != spine_x)
    {
        check.fail(
            vec!["timeline:spine".to_string()],
            "timeline paths do not share one vertical spine",
            vec![WitnessField::integer("expected_x", spine_x as i64)],
        );
    }
    check
}

fn timeline_chronology(timeline: &Timeline, scene: &Scene) -> CheckReport {
    let leading = timeline_leading_edges(timeline, scene);
    let mut check = CheckReport::new(
        "timeline.chronology",
        CheckClass::Preference,
        timeline.periods.len(),
    );
    if leading.len() != timeline.periods.len() {
        check.fail(
            Vec::new(),
            "timeline period count does not match leading connector count",
            vec![
                WitnessField::integer("periods", timeline.periods.len() as i64),
                WitnessField::integer("connectors", leading.len() as i64),
            ],
        );
    }
    for pair in leading.windows(2) {
        let left = pair[0].points.last().unwrap().y;
        let right = pair[1].points.last().unwrap().y;
        if left >= right {
            check.fail(
                vec![
                    format!("period_connector:{}", pair[0].edge),
                    format!("period_connector:{}", pair[1].edge),
                ],
                "timeline period anchors are not strictly chronological",
                vec![
                    WitnessField::integer("first_y", left as i64),
                    WitnessField::integer("second_y", right as i64),
                ],
            );
        }
    }
    check
}

fn timeline_connector_padding(timeline: &Timeline, scene: &Scene) -> CheckReport {
    let expected = timeline.periods.len() + timeline.event_count();
    let mut check = CheckReport::new(
        "timeline.connector_padding",
        CheckClass::Preference,
        expected,
    );
    let Some(spine_x) = scene
        .paths
        .first()
        .and_then(|path| path.points.first())
        .map(|point| point.x)
    else {
        return check;
    };
    for edge in &scene.edges {
        let Some(first) = edge.points.first() else {
            continue;
        };
        let Some(last) = edge.points.last() else {
            continue;
        };
        let (side, attachment, candidates): (&str, Point, Vec<&SceneText>) = if last.x == spine_x {
            (
                "leading",
                *first,
                scene
                    .texts
                    .iter()
                    .filter(|text| text_covers_row(text, first.y) && text_right(text) <= first.x)
                    .collect(),
            )
        } else if first.x == spine_x {
            (
                "trailing",
                *last,
                scene
                    .texts
                    .iter()
                    .filter(|text| text_covers_row(text, last.y) && text.at.x >= last.x)
                    .collect(),
            )
        } else {
            continue;
        };
        let blank_cells = if side == "leading" {
            candidates
                .iter()
                .map(|text| attachment.x - text_right(text))
                .min()
        } else {
            candidates
                .iter()
                .map(|text| text.at.x - attachment.x - 1)
                .min()
        };
        if blank_cells.is_none_or(|value| value < 1) {
            check.fail(
                vec![format!("connector:{}", edge.edge)],
                format!(
                    "timeline {side} connector {} lacks one visible blank cell before its label",
                    edge.edge
                ),
                vec![
                    WitnessField::point("attachment", attachment),
                    WitnessField::integer("blank_cells", blank_cells.unwrap_or(-1) as i64),
                ],
            );
        }
    }
    check
}

fn timeline_sections(timeline: &Timeline, scene: &Scene) -> CheckReport {
    let mut check = CheckReport::new(
        "timeline.section_containment",
        CheckClass::Invariant,
        timeline.sections.len(),
    );
    let leading = timeline_leading_edges(timeline, scene);
    for (section, value) in timeline.sections.iter().enumerate() {
        let Some(group) = scene.groups.iter().find(|group| group.subgraph == section) else {
            check.fail(
                vec![format!("section:{section}")],
                format!("timeline section `{}` has no frame", value.label),
                Vec::new(),
            );
            continue;
        };
        for period in value.first_period..value.first_period + value.period_count {
            if let Some(anchor) = leading
                .get(period)
                .and_then(|edge| edge.points.last())
                .copied()
                && !group.rect.contains(anchor)
            {
                check.fail(
                    vec![format!("section:{section}"), format!("period:{period}")],
                    format!(
                        "timeline section `{}` does not contain period {period}",
                        value.label
                    ),
                    vec![
                        WitnessField::rect("section", group.rect),
                        WitnessField::point("anchor", anchor),
                    ],
                );
            }
        }
    }
    check
}

fn timeline_title(timeline: &Timeline, scene: &Scene) -> CheckReport {
    let mut check = CheckReport::new(
        "timeline.title_centered",
        CheckClass::Preference,
        usize::from(timeline.title.is_some()),
    );
    let Some(title) = &timeline.title else {
        return check;
    };
    let Some(spine_x) = scene
        .paths
        .first()
        .and_then(|path| path.points.first())
        .map(|point| point.x)
    else {
        return check;
    };
    let Some(text) = scene
        .texts
        .iter()
        .filter(|text| text.text == *title)
        .min_by_key(|text| text.at.y)
    else {
        check.fail(
            vec!["timeline:title".to_string()],
            "timeline title is missing from scene text",
            vec![WitnessField::text("title", title)],
        );
        return check;
    };
    let center2 = 2 * text.at.x + text.width() as i32 - 1;
    let residual2 = (center2 - 2 * spine_x).unsigned_abs() as usize;
    if residual2 > 1 {
        check.fail(
            vec!["timeline:title".to_string(), "timeline:spine".to_string()],
            "timeline title is not centered on the chronological spine",
            vec![
                WitnessField::integer("title_center2", center2 as i64),
                WitnessField::integer("spine_center2", (2 * spine_x) as i64),
                WitnessField::integer("residual2", residual2 as i64),
            ],
        );
    }
    check
}

#[derive(Clone, Copy)]
enum EndpointGeometry {
    Box(usize),
    Group(usize),
}

#[derive(Clone)]
struct EndpointExpectation {
    edge: usize,
    source: EndpointGeometry,
    target: EndpointGeometry,
    elements: Vec<String>,
}

fn edge_endpoint_check(
    id: &'static str,
    expectations: Vec<EndpointExpectation>,
    scene: &Scene,
) -> CheckReport {
    let mut check = CheckReport::new(id, CheckClass::Invariant, expectations.len());
    for expectation in expectations {
        let Some(edge) = scene
            .edges
            .iter()
            .find(|edge| edge.edge == expectation.edge)
        else {
            check.fail(
                expectation.elements,
                format!(
                    "semantic edge {} has no routed scene edge",
                    expectation.edge
                ),
                Vec::new(),
            );
            continue;
        };
        let Some(source) = endpoint_rect(scene, expectation.source) else {
            continue;
        };
        let Some(target) = endpoint_rect(scene, expectation.target) else {
            continue;
        };
        let first = edge.points.first().copied();
        let last = semantic_target(edge);
        if first.is_none_or(|point| !endpoint_contains(source, expectation.source, point))
            || last.is_none_or(|point| !endpoint_contains(target, expectation.target, point))
        {
            check.fail(
                expectation.elements,
                format!(
                    "semantic edge {} does not attach to its declared endpoints",
                    expectation.edge
                ),
                vec![
                    WitnessField::rect("source", source),
                    WitnessField::rect("target", target),
                    WitnessField::point("actual_source", first.unwrap_or_default()),
                    WitnessField::point("actual_target", last.unwrap_or_default()),
                ],
            );
        }
    }
    check
}

fn flow_endpoints(graph: &Graph) -> Vec<EndpointExpectation> {
    graph
        .edges
        .iter()
        .enumerate()
        .map(|(edge, value)| EndpointExpectation {
            edge,
            source: flow_endpoint_geometry(value.source),
            target: flow_endpoint_geometry(value.target),
            elements: vec![
                edge_ref(graph, edge),
                flow_endpoint_ref(graph, value.source),
                flow_endpoint_ref(graph, value.target),
            ],
        })
        .collect()
}

fn state_endpoints(state: &StateDiagram) -> Vec<EndpointExpectation> {
    let mut next_marker = state.states.len();
    state
        .transitions
        .iter()
        .enumerate()
        .map(|(edge, transition)| {
            let source = match transition.from {
                Endpoint::State(state) => state,
                Endpoint::Marker => {
                    let marker = next_marker;
                    next_marker += 1;
                    marker
                }
            };
            let target = match transition.to {
                Endpoint::State(state) => state,
                Endpoint::Marker => {
                    let marker = next_marker;
                    next_marker += 1;
                    marker
                }
            };
            EndpointExpectation {
                edge,
                source: EndpointGeometry::Box(source),
                target: EndpointGeometry::Box(target),
                elements: vec![
                    format!("transition:{edge}"),
                    state_box_ref(state, source),
                    state_box_ref(state, target),
                ],
            }
        })
        .collect()
}

fn state_box_ref(state: &StateDiagram, node: usize) -> String {
    state
        .states
        .get(node)
        .map(|value| format!("state:{}", value.id))
        .unwrap_or_else(|| format!("marker:{}", node - state.states.len()))
}

fn scene_box(scene: &Scene, node: usize) -> Option<&SceneBox> {
    scene.boxes.iter().find(|box_| box_.node == node)
}

fn endpoint_rect(scene: &Scene, endpoint: EndpointGeometry) -> Option<Rect> {
    match endpoint {
        EndpointGeometry::Box(node) => scene_box(scene, node).map(|value| value.rect),
        EndpointGeometry::Group(group) => scene
            .groups
            .iter()
            .find(|value| value.subgraph == group)
            .map(|value| value.rect),
    }
}

fn endpoint_contains(rect: Rect, endpoint: EndpointGeometry, point: Point) -> bool {
    match endpoint {
        EndpointGeometry::Box(_) => rect.contains(point),
        EndpointGeometry::Group(_) => {
            rect.contains(point)
                && (point.x == rect.x
                    || point.x == rect.right() - 1
                    || point.y == rect.y
                    || point.y == rect.bottom() - 1)
        }
    }
}

fn flow_endpoint_geometry(endpoint: FlowEndpoint) -> EndpointGeometry {
    match endpoint {
        FlowEndpoint::Node(node) => EndpointGeometry::Box(node),
        FlowEndpoint::Subgraph(group) => EndpointGeometry::Group(group),
    }
}

fn flow_endpoint_ref(graph: &Graph, endpoint: FlowEndpoint) -> String {
    match endpoint {
        FlowEndpoint::Node(node) => node_ref(graph, node),
        FlowEndpoint::Subgraph(group) => format!("group:{}", graph.subgraphs[group].id),
    }
}

fn semantic_target(edge: &RoutedEdge) -> Option<Point> {
    edge.arrow
        .as_ref()
        .map(|arrow| arrow.toward)
        .or_else(|| edge.points.last().copied())
}

fn node_ref(graph: &Graph, node: usize) -> String {
    format!("node:{}", graph.nodes[node].id)
}

fn edge_ref(graph: &Graph, edge: usize) -> String {
    let value = &graph.edges[edge];
    format!(
        "edge:{edge}({}->{})",
        flow_endpoint_ref(graph, value.source),
        flow_endpoint_ref(graph, value.target)
    )
}

fn mindmap_node_ref(node: usize) -> String {
    format!("mindmap_node:{node}")
}

fn is_forward(source: Rect, target: Rect, direction: Dir) -> bool {
    match direction {
        Dir::LR => source.center2().x < target.center2().x,
        Dir::RL => source.center2().x > target.center2().x,
        Dir::TB => source.center2().y < target.center2().y,
        Dir::BT => source.center2().y > target.center2().y,
    }
}

fn cross_center2(rect: Rect, direction: Dir) -> i32 {
    match direction {
        Dir::LR | Dir::RL => rect.center2().y,
        Dir::TB | Dir::BT => rect.center2().x,
    }
}

fn push_unique(values: &mut Vec<usize>, value: usize) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn barycenter_residual(center: i64, peers: &[i64]) -> usize {
    if peers.len() < 2 {
        return 0;
    }
    let degree = peers.len() as i64;
    parity_excess(center * degree - peers.iter().sum::<i64>(), 2 * degree)
}

fn parity_excess(difference: i64, step: i64) -> usize {
    let raw = difference.unsigned_abs() as usize;
    let remainder = difference.rem_euclid(step);
    let floor = remainder.min(step - remainder) as usize;
    raw.saturating_sub(floor)
}

fn diamond_parity_floor(fork: i64, merge: i64, branch_sum: i64) -> usize {
    let target = branch_sum / 2;
    let parity = |value: i64| value.rem_euclid(2);
    let mut best = usize::MAX;
    for candidate_fork in target - 4..=target + 4 {
        if parity(candidate_fork) != parity(fork) {
            continue;
        }
        for candidate_merge in target - 4..=target + 4 {
            if parity(candidate_merge) != parity(merge) {
                continue;
            }
            let score = candidate_fork.abs_diff(candidate_merge) as usize
                + branch_sum.abs_diff(candidate_fork + candidate_merge) as usize;
            best = best.min(score);
        }
    }
    best
}

fn perpendicular_interior_crossing(left: &[Point], right: &[Point]) -> Option<Point> {
    let left_horizontal = left[0].y == left[1].y && left[0].x != left[1].x;
    let left_vertical = left[0].x == left[1].x && left[0].y != left[1].y;
    let right_horizontal = right[0].y == right[1].y && right[0].x != right[1].x;
    let right_vertical = right[0].x == right[1].x && right[0].y != right[1].y;
    let (horizontal, vertical) = if left_horizontal && right_vertical {
        (left, right)
    } else if right_horizontal && left_vertical {
        (right, left)
    } else {
        return None;
    };
    let point = Point::new(vertical[0].x, horizontal[0].y);
    let inside_horizontal = point.x > horizontal[0].x.min(horizontal[1].x)
        && point.x < horizontal[0].x.max(horizontal[1].x);
    let inside_vertical =
        point.y > vertical[0].y.min(vertical[1].y) && point.y < vertical[0].y.max(vertical[1].y);
    (inside_horizontal && inside_vertical).then_some(point)
}

fn contains_rect(outer: Rect, inner: Rect) -> bool {
    outer.contains(Point::new(inner.x, inner.y))
        && outer.contains(Point::new(inner.right() - 1, inner.bottom() - 1))
}

fn manhattan(left: Point, right: Point) -> usize {
    left.x.abs_diff(right.x) as usize + left.y.abs_diff(right.y) as usize
}

fn point_rect_distance(point: Point, rect: Rect) -> usize {
    let dx = if point.x < rect.x {
        rect.x - point.x
    } else if point.x >= rect.right() {
        point.x - rect.right() + 1
    } else {
        0
    };
    let dy = if point.y < rect.y {
        rect.y - point.y
    } else if point.y >= rect.bottom() {
        point.y - rect.bottom() + 1
    } else {
        0
    };
    (dx + dy) as usize
}

fn decoration_kind_name(kind: EndpointDecorationKind) -> &'static str {
    match kind {
        EndpointDecorationKind::OpenArrow => "open_arrow",
        EndpointDecorationKind::OpenTriangle => "open_triangle",
        EndpointDecorationKind::OpenDiamond => "open_diamond",
        EndpointDecorationKind::FilledDiamond => "filled_diamond",
        EndpointDecorationKind::Cardinality { .. } => "cardinality",
    }
}

fn cardinality_kind(token: &str) -> EndpointDecorationKind {
    use crate::scene::{CardinalityMaximum, CardinalityMinimum};
    let (minimum, maximum) = match token {
        "||" => (CardinalityMinimum::One, CardinalityMaximum::One),
        "|o" | "o|" => (CardinalityMinimum::Zero, CardinalityMaximum::One),
        "}o" | "o{" => (CardinalityMinimum::Zero, CardinalityMaximum::Many),
        "}|" | "|{" => (CardinalityMinimum::One, CardinalityMaximum::Many),
        _ => unreachable!("ER parser validates cardinality"),
    };
    EndpointDecorationKind::Cardinality { minimum, maximum }
}

fn timeline_leading_edges<'a>(timeline: &Timeline, scene: &'a Scene) -> Vec<&'a RoutedEdge> {
    let mut edges = Vec::with_capacity(timeline.periods.len());
    let mut index = 0usize;
    for period in &timeline.periods {
        if let Some(edge) = scene.edges.get(index) {
            edges.push(edge);
        }
        index += 1 + period.events.len();
    }
    edges
}

fn text_covers_row(text: &SceneText, y: i32) -> bool {
    y >= text.at.y && y < text.at.y + text.height() as i32
}

fn text_right(text: &SceneText) -> i32 {
    text.at.x + text.width() as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram;

    #[test]
    fn shifted_final_scene_is_caught_without_consulting_placed_layout() {
        let parsed = diagram::parse("flowchart LR\nA --> B\n").unwrap();
        let mut scene = diagram::scene(&parsed, 100);
        scene.boxes[1].rect.y += 2;

        let report = evaluate(&parsed, &scene, 100);
        let check = report
            .checks
            .iter()
            .find(|check| check.id == "flow.mono_centerline")
            .unwrap();
        assert_eq!(check.status(), "fail");
        assert_eq!(check.failures.len(), 1);
        assert!(
            check.failures[0]
                .elements
                .iter()
                .any(|value| value == "node:B")
        );
    }

    #[test]
    fn composite_biclique_is_unclassified_instead_of_falsely_failed() {
        let parsed = diagram::parse("flowchart LR\nA --> C\nA --> D\nB --> C\nB --> D\n").unwrap();
        let scene = diagram::scene(&parsed, 100);
        let report = evaluate(&parsed, &scene, 100);

        assert!(!report.has_failures(), "{report:#?}");
        assert!(
            report
                .unclassified
                .iter()
                .any(|value| value.kind == "flow.composite_fork")
        );
        assert!(
            report
                .unclassified
                .iter()
                .any(|value| value.kind == "flow.composite_merge")
        );
    }

    #[test]
    fn corrupt_final_box_border_is_caught_by_independent_scene_gate() {
        let parsed = diagram::parse("mindmap\n  Root\n    Child\n").unwrap();
        let mut scene = diagram::scene(&parsed, 100);
        scene.boxes[0].rect.w = 1;

        let report = evaluate(&parsed, &scene, 100);
        let check = report
            .checks
            .iter()
            .find(|check| check.id == "scene.integrity")
            .unwrap();
        assert_eq!(check.status(), "fail");
    }

    #[test]
    fn timeline_title_identity_prefers_the_topmost_duplicate_label() {
        let parsed = diagram::parse("timeline\ntitle Q1\nQ1 : Build\n").unwrap();
        let scene = diagram::scene(&parsed, 100);
        let report = evaluate(&parsed, &scene, 100);
        let check = report
            .checks
            .iter()
            .find(|check| check.id == "timeline.title_centered")
            .unwrap();

        assert_eq!(check.status(), "pass", "{check:#?}");
    }

    #[test]
    fn altered_structured_cell_fails_semantic_compartment_check() {
        let parsed = diagram::parse("classDiagram\nclass User {\n+name\n}\n").unwrap();
        let mut scene = diagram::scene(&parsed, 100);
        scene.boxes[0].table.as_mut().unwrap().rows[0][0] = "+other".to_string();

        let report = evaluate(&parsed, &scene, 100);
        let check = report
            .checks
            .iter()
            .find(|check| check.id == "class.compartments")
            .unwrap();
        assert_eq!(check.status(), "fail");
        assert_eq!(check.failures[0].elements, ["class:User"]);
    }

    #[test]
    fn boxed_layout_aesthetics_are_explicitly_unclassified() {
        let parsed = diagram::parse("classDiagram\nA --> B\n").unwrap();
        let scene = diagram::scene(&parsed, 100);
        let report = evaluate(&parsed, &scene, 100);

        assert!(!report.has_quality_failures(), "{report:#?}");
        assert!(
            report
                .unclassified
                .iter()
                .any(|value| value.kind == "class.boxed_layout_composition"),
            "{report:#?}"
        );
    }

    #[test]
    fn extended_final_lifeline_fails_final_fragment_termination_check() {
        let parsed = diagram::parse(
            "sequenceDiagram\nparticipant A\nparticipant B\nloop Retry\nA->>B: go\nend\n",
        )
        .unwrap();
        let mut scene = diagram::scene(&parsed, 100);
        scene.paths[0].points.last_mut().unwrap().y += 1;

        let report = evaluate(&parsed, &scene, 100);
        let check = report
            .checks
            .iter()
            .find(|check| check.id == "sequence.final_fragment_termination")
            .unwrap();
        assert_eq!(check.status(), "fail");
        assert_eq!(check.failures[0].elements, ["participant:A", "lifeline:A"]);
    }

    #[test]
    fn overlapping_final_er_decorations_fail_lane_separation_check() {
        let parsed =
            diagram::parse("erDiagram\ndirection TB\nA ||--o{ C : first\nB ||--o{ C : second\n")
                .unwrap();
        let mut scene = diagram::scene(&parsed, 100);
        scene.endpoint_decorations[3].at = scene.endpoint_decorations[1].at;
        scene.endpoint_decorations[3].toward = scene.endpoint_decorations[1].toward;

        let report = evaluate(&parsed, &scene, 100);
        let check = report
            .checks
            .iter()
            .find(|check| check.id == "er.cardinality_lane_separation")
            .unwrap();
        assert_eq!(check.status(), "fail");
        assert_eq!(
            check.failures[0].elements,
            ["relationship:0", "relationship:1", "entity:C"]
        );
    }

    #[test]
    fn detached_er_label_fails_path_attachment_check() {
        let parsed = diagram::parse("erDiagram\ndirection TB\nA ||--o{ B : owns\n").unwrap();
        let mut scene = diagram::scene(&parsed, 100);
        scene.edges[0].label.as_mut().unwrap().at.x += 20;

        let report = evaluate(&parsed, &scene, 100);
        let check = report
            .checks
            .iter()
            .find(|check| check.id == "er.relationship_label_attachment")
            .unwrap();
        assert_eq!(check.status(), "fail");
        assert_eq!(check.failures[0].elements, ["relationship:0"]);
    }
}
