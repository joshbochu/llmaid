//! Exact, topology-aware geometry quality audit.
//!
//! Correctness and relational residuals use doubled integer coordinates:
//! `center2 = 2 * origin + extent - 1`. This represents cell and half-cell
//! centers without floats or parity loss. Global aesthetics are deliberately
//! not collapsed into one score; callers compare the vector lexicographically.

use crate::layout::Placed;
use crate::parse::Graph;
use crate::scene::{Point, Scene};

/// A correctness failure with a stable machine name and exact scene witness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeometryViolation {
    pub name: &'static str,
    pub edge: usize,
    pub node: usize,
    pub at: Point,
}

impl GeometryViolation {
    fn message(&self) -> String {
        format!(
            "edge {} intersects non-endpoint box {} at ({},{})",
            self.edge, self.node, self.at.x, self.at.y
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeometryAudit {
    pub width: usize,
    pub height: usize,
    pub area: usize,
    pub nodes: usize,
    pub edges: usize,
    pub ranks: usize,
    pub hard_violations: Vec<String>,
    /// Avoidable flow-axis centering error inside rank spans, in doubled cells.
    pub rank_axis_residual2: usize,
    /// Avoidable cross-axis error for true one-in/one-out edges.
    pub mono_centerline_residual2: usize,
    /// Exact barycenter numerator beyond the minimum forced by cell parity.
    pub fork_barycenter_residual2: usize,
    pub merge_barycenter_residual2: usize,
    /// Structurally eligible two-branch, one-rank fork/merge diamonds.
    pub diamond_motifs: usize,
    pub diamond_mirror_residual2: usize,
    /// Shared interior path cells between distinct routed edges.
    pub crossing_cells: usize,
    pub bends: usize,
    pub wire_length: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComparableSignature {
    pub hard_violations: usize,
    pub area: usize,
    pub rank_axis_residual2: usize,
    pub mono_centerline_residual2: usize,
    pub fork_barycenter_residual2: usize,
    pub merge_barycenter_residual2: usize,
    pub diamond_motifs: usize,
    pub diamond_mirror_residual2: usize,
    pub crossing_cells: usize,
    pub bends: usize,
    pub wire_length: usize,
}

impl GeometryAudit {
    /// Direction-independent values suitable for LR/RL and TB/BT metamorphic
    /// comparisons. Width and height are represented by their product.
    pub fn comparable_signature(&self) -> ComparableSignature {
        ComparableSignature {
            hard_violations: self.hard_violations.len(),
            area: self.area,
            rank_axis_residual2: self.rank_axis_residual2,
            mono_centerline_residual2: self.mono_centerline_residual2,
            fork_barycenter_residual2: self.fork_barycenter_residual2,
            merge_barycenter_residual2: self.merge_barycenter_residual2,
            diamond_motifs: self.diamond_motifs,
            diamond_mirror_residual2: self.diamond_mirror_residual2,
            crossing_cells: self.crossing_cells,
            bends: self.bends,
            wire_length: self.wire_length,
        }
    }
}

pub fn measure_graph(graph: &Graph, max_width: usize) -> GeometryAudit {
    let placed = crate::layout::layout(graph, max_width);
    let scene = crate::route::route(graph, &placed);
    measure(graph, &placed, &scene)
}

pub fn measure(graph: &Graph, placed: &Placed, scene: &Scene) -> GeometryAudit {
    let bounds = scene.bounds();
    let width = bounds.w.max(0) as usize;
    let height = bounds.h.max(0) as usize;
    let centers = node_cross_centers2(placed);
    let topology = Topology::new(graph, placed);
    let (fork_barycenter_residual2, merge_barycenter_residual2) =
        junction_barycenter_residuals(graph, placed, &centers, &topology);
    let (diamond_motifs, diamond_mirror_residual2) = diamond_residuals(&centers, &topology);

    let hard_violations = violations(scene)
        .into_iter()
        .map(|violation| violation.message())
        .collect();

    GeometryAudit {
        width,
        height,
        area: width.saturating_mul(height),
        nodes: graph.nodes.len(),
        edges: graph.edges.len(),
        ranks: placed.rank_span.len(),
        hard_violations,
        rank_axis_residual2: rank_axis_residual2(placed),
        mono_centerline_residual2: mono_centerline_residual2(graph, &centers, &topology),
        fork_barycenter_residual2,
        merge_barycenter_residual2,
        diamond_motifs,
        diamond_mirror_residual2,
        crossing_cells: crossing_cells(graph, scene),
        bends: scene
            .edges
            .iter()
            .map(|edge| bend_count(&edge.points))
            .sum(),
        wire_length: scene
            .edges
            .iter()
            .map(|edge| path_length(&edge.points))
            .sum(),
    }
}

/// Named hard geometry failures in deterministic edge/box traversal order.
pub fn violations(scene: &Scene) -> Vec<GeometryViolation> {
    scene
        .edge_box_intersections()
        .into_iter()
        .map(|intersection| GeometryViolation {
            name: "edge_intersects_non_endpoint_box",
            edge: intersection.edge,
            node: intersection.node,
            at: intersection.at,
        })
        .collect()
}

/// Serialize a complete audit as a compact, byte-stable JSON document.
///
/// The schema is deliberately handwritten: JSON is a CLI boundary, not a
/// reason to add a runtime serialization dependency to the renderer.
pub fn json(diagram: &crate::diagram::Diagram, max_width: usize) -> String {
    match diagram {
        crate::diagram::Diagram::Flowchart(graph) => {
            let placed = crate::layout::layout(graph, max_width);
            let scene = crate::route::route(graph, &placed);
            flowchart_json(graph, &placed, &scene)
        }
        crate::diagram::Diagram::Sequence(sequence) => {
            let mut scene = crate::sequence::scene(sequence, max_width);
            scene.normalize();
            let bounds = scene.bounds();
            let edges = sequence
                .events
                .iter()
                .filter(|event| matches!(event, crate::sequence::SequenceEvent::Message(_)))
                .count();
            let audit = GeometryAudit {
                width: bounds.w.max(0) as usize,
                height: bounds.h.max(0) as usize,
                area: (bounds.w.max(0) as usize).saturating_mul(bounds.h.max(0) as usize),
                nodes: sequence.participants.len(),
                edges,
                ranks: 0,
                hard_violations: violations(&scene)
                    .iter()
                    .map(GeometryViolation::message)
                    .collect(),
                rank_axis_residual2: 0,
                mono_centerline_residual2: 0,
                fork_barycenter_residual2: 0,
                merge_barycenter_residual2: 0,
                diamond_motifs: 0,
                diamond_mirror_residual2: 0,
                crossing_cells: 0,
                bends: scene
                    .edges
                    .iter()
                    .map(|edge| bend_count(&edge.points))
                    .sum(),
                wire_length: scene
                    .edges
                    .iter()
                    .map(|edge| path_length(&edge.points))
                    .sum(),
            };
            let generic = generic_invariant_failures(&scene);
            json_report("sequence", &audit, &violations(&scene), &generic, false)
        }
        crate::diagram::Diagram::State(state) => {
            generic_scene_json("state", crate::state::scene(state, max_width))
        }
        crate::diagram::Diagram::Class(class) => {
            generic_scene_json("class", crate::class::scene(class, max_width))
        }
        crate::diagram::Diagram::Er(er) => {
            generic_scene_json("er", crate::er::scene(er, max_width))
        }
    }
}

fn generic_scene_json(diagram: &str, mut scene: Scene) -> String {
    scene.normalize();
    let bounds = scene.bounds();
    let violations = violations(&scene);
    let audit = GeometryAudit {
        width: bounds.w.max(0) as usize,
        height: bounds.h.max(0) as usize,
        area: (bounds.w.max(0) as usize).saturating_mul(bounds.h.max(0) as usize),
        nodes: scene.boxes.len(),
        edges: scene.edges.len(),
        ranks: 0,
        hard_violations: violations.iter().map(GeometryViolation::message).collect(),
        rank_axis_residual2: 0,
        mono_centerline_residual2: 0,
        fork_barycenter_residual2: 0,
        merge_barycenter_residual2: 0,
        diamond_motifs: 0,
        diamond_mirror_residual2: 0,
        crossing_cells: 0,
        bends: scene
            .edges
            .iter()
            .map(|edge| bend_count(&edge.points))
            .sum(),
        wire_length: scene
            .edges
            .iter()
            .map(|edge| path_length(&edge.points))
            .sum(),
    };
    let generic = generic_invariant_failures(&scene);
    json_report(diagram, &audit, &violations, &generic, false)
}

/// Serialize caller-supplied flowchart geometry, primarily for layout tools
/// that need the same stable report as the CLI.
pub fn flowchart_json(graph: &Graph, placed: &Placed, scene: &Scene) -> String {
    let mut scene = scene.clone();
    scene.normalize();
    let mut audit = measure(graph, placed, &scene);
    if graph.nodes.is_empty() {
        audit.ranks = 0;
    }
    let generic = generic_invariant_failures(&scene);
    json_report("flowchart", &audit, &violations(&scene), &generic, true)
}

fn generic_invariant_failures(scene: &Scene) -> Vec<String> {
    let geometry_messages: Vec<String> = violations(scene)
        .iter()
        .map(GeometryViolation::message)
        .collect();
    let (_, failures) =
        crate::render::render_scene_with_checks(scene, crate::style::Style { ascii: false });
    failures
        .into_iter()
        .filter(|failure| !geometry_messages.contains(failure))
        .collect()
}

fn json_report(
    diagram: &str,
    audit: &GeometryAudit,
    violations: &[GeometryViolation],
    generic_violations: &[String],
    flowchart_metrics: bool,
) -> String {
    let mut output = format!(
        "{{\"schema\":\"llmaid.audit.v1\",\"diagram\":\"{diagram}\",\"bounds\":{{\"width\":{},\"height\":{},\"area\":{}}},\"elements\":{{\"nodes\":{},\"edges\":{},\"ranks\":{}}},\"violations\":[",
        audit.width, audit.height, audit.area, audit.nodes, audit.edges, audit.ranks
    );
    for (index, violation) in violations.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        output.push_str(&format!(
            "{{\"name\":\"{}\",\"message\":\"{}\",\"witness\":{{\"edge\":{},\"node\":{},\"at\":{{\"x\":{},\"y\":{}}}}}}}",
            violation.name,
            json_escape(&violation.message()),
            violation.edge,
            violation.node,
            violation.at.x,
            violation.at.y
        ));
    }
    for (index, message) in generic_violations.iter().enumerate() {
        if !violations.is_empty() || index != 0 {
            output.push(',');
        }
        output.push_str(&format!(
            "{{\"name\":\"scene_invariant\",\"message\":\"{}\",\"witness\":null}}",
            json_escape(message)
        ));
    }
    output.push_str("],\"metrics\":");
    if flowchart_metrics {
        output.push_str(&format!(
            "{{\"rank_axis_residual2\":{},\"mono_centerline_residual2\":{},\"fork_barycenter_residual2\":{},\"merge_barycenter_residual2\":{},\"diamond_motifs\":{},\"diamond_mirror_residual2\":{},\"crossing_cells\":{},\"bends\":{},\"wire_length\":{}}}",
            audit.rank_axis_residual2,
            audit.mono_centerline_residual2,
            audit.fork_barycenter_residual2,
            audit.merge_barycenter_residual2,
            audit.diamond_motifs,
            audit.diamond_mirror_residual2,
            audit.crossing_cells,
            audit.bends,
            audit.wire_length
        ));
    } else {
        output.push_str("null");
    }
    output.push_str("}\n");
    output
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch <= '\u{1f}' => {
                use std::fmt::Write;
                write!(escaped, "\\u{:04x}", ch as u32).expect("writing to String cannot fail");
            }
            ch => escaped.push(ch),
        }
    }
    escaped
}

struct Topology {
    incoming: Vec<Vec<usize>>,
    outgoing: Vec<Vec<usize>>,
    incoming_edges: Vec<usize>,
    outgoing_edges: Vec<usize>,
}

impl Topology {
    fn new(graph: &Graph, placed: &Placed) -> Self {
        let mut topology = Self {
            incoming: vec![Vec::new(); graph.nodes.len()],
            outgoing: vec![Vec::new(); graph.nodes.len()],
            incoming_edges: vec![0; graph.nodes.len()],
            outgoing_edges: vec![0; graph.nodes.len()],
        };
        for (edge_index, edge) in graph.edges.iter().enumerate() {
            if edge.from == edge.to || placed.back_edges.contains(&edge_index) {
                continue;
            }
            topology.outgoing_edges[edge.from] += 1;
            topology.incoming_edges[edge.to] += 1;
            push_unique(&mut topology.outgoing[edge.from], edge.to);
            push_unique(&mut topology.incoming[edge.to], edge.from);
        }
        topology
    }
}

fn push_unique(values: &mut Vec<usize>, value: usize) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn node_cross_centers2(placed: &Placed) -> Vec<i64> {
    placed
        .boxes
        .iter()
        .map(|box_| (2 * box_.c + box_.clen - 1) as i64)
        .collect()
}

fn rank_axis_residual2(placed: &Placed) -> usize {
    placed
        .boxes
        .iter()
        .map(|box_| {
            let (start, extent) = placed.rank_span[box_.rank];
            let box_center2 = 2 * box_.f + box_.flen - 1;
            let rank_center2 = 2 * start + extent - 1;
            parity_excess(box_center2 as i64 - rank_center2 as i64, 2)
        })
        .sum()
}

fn mono_centerline_residual2(graph: &Graph, centers: &[i64], topology: &Topology) -> usize {
    graph
        .edges
        .iter()
        .filter(|edge| {
            edge.from != edge.to
                && topology.outgoing_edges[edge.from] == 1
                && topology.incoming_edges[edge.to] == 1
        })
        .map(|edge| parity_excess(centers[edge.from] - centers[edge.to], 2))
        .sum()
}

fn junction_barycenter_residuals(
    graph: &Graph,
    placed: &Placed,
    centers: &[i64],
    topology: &Topology,
) -> (usize, usize) {
    let mut fork = 0usize;
    let mut merge = 0usize;
    for (node, &center) in centers.iter().enumerate() {
        if topology.outgoing[node].len() >= 2 {
            let mut destinations = Vec::new();
            let lanes: Vec<i64> = graph
                .edges
                .iter()
                .enumerate()
                .filter(|(edge_index, edge)| {
                    edge.from == node
                        && edge.from != edge.to
                        && !placed.back_edges.contains(edge_index)
                })
                .filter(|(_, edge)| {
                    if destinations.contains(&edge.to) {
                        false
                    } else {
                        destinations.push(edge.to);
                        true
                    }
                })
                .filter_map(|(edge_index, _)| placed.segs[edge_index].first())
                .map(|segment| (2 * segment.to.1) as i64)
                .collect();
            fork += lane_barycenter_numerator(center, &lanes);
        }
        if topology.incoming[node].len() >= 2 {
            let mut sources = Vec::new();
            let lanes: Vec<i64> = graph
                .edges
                .iter()
                .enumerate()
                .filter(|(edge_index, edge)| {
                    edge.to == node
                        && edge.from != edge.to
                        && !placed.back_edges.contains(edge_index)
                })
                .filter(|(_, edge)| {
                    if sources.contains(&edge.from) {
                        false
                    } else {
                        sources.push(edge.from);
                        true
                    }
                })
                .filter_map(|(edge_index, _)| placed.segs[edge_index].last())
                .map(|segment| (2 * segment.from.1) as i64)
                .collect();
            merge += lane_barycenter_numerator(center, &lanes);
        }
    }
    (fork, merge)
}

fn lane_barycenter_numerator(center: i64, lanes: &[i64]) -> usize {
    if lanes.len() < 2 {
        return 0;
    }
    let sum: i64 = lanes.iter().sum();
    let degree = lanes.len() as i64;
    parity_excess(center * degree - sum, 2 * degree)
}

fn diamond_residuals(centers: &[i64], topology: &Topology) -> (usize, usize) {
    let mut motifs = 0usize;
    let mut residual = 0usize;
    for fork in 0..centers.len() {
        let [left, right] = topology.outgoing[fork].as_slice() else {
            continue;
        };
        let Some(merge) = topology.outgoing[*left]
            .iter()
            .copied()
            .find(|candidate| topology.outgoing[*right].contains(candidate))
        else {
            continue;
        };
        if !topology.incoming[merge].contains(left) || !topology.incoming[merge].contains(right) {
            continue;
        }
        motifs += 1;
        let raw = centers[fork].abs_diff(centers[merge]) as usize
            + (centers[*left] + centers[*right]).abs_diff(centers[fork] + centers[merge]) as usize;
        residual += raw.saturating_sub(diamond_parity_floor(
            centers[fork],
            centers[merge],
            centers[*left] + centers[*right],
        ));
    }
    (motifs, residual)
}

fn crossing_cells(graph: &Graph, scene: &Scene) -> usize {
    let mut count = 0usize;
    for left in 0..scene.edges.len() {
        for right in left + 1..scene.edges.len() {
            let left_graph_edge = &graph.edges[scene.edges[left].edge];
            let right_graph_edge = &graph.edges[scene.edges[right].edge];
            if [left_graph_edge.from, left_graph_edge.to]
                .iter()
                .any(|node| *node == right_graph_edge.from || *node == right_graph_edge.to)
            {
                continue;
            }
            let mut shared = Vec::new();
            for left_segment in scene.edges[left].points.windows(2) {
                for right_segment in scene.edges[right].points.windows(2) {
                    if let Some(point) =
                        perpendicular_interior_crossing(left_segment, right_segment)
                        && !shared.contains(&point)
                    {
                        shared.push(point);
                    }
                }
            }
            count += shared.len();
        }
    }
    count
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

fn bend_count(points: &[Point]) -> usize {
    points
        .windows(3)
        .filter(|window| {
            let first_horizontal = window[0].y == window[1].y;
            let second_horizontal = window[1].y == window[2].y;
            first_horizontal != second_horizontal
        })
        .count()
}

fn path_length(points: &[Point]) -> usize {
    points
        .windows(2)
        .map(|pair| pair[0].x.abs_diff(pair[1].x) as usize + pair[0].y.abs_diff(pair[1].y) as usize)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossing_requires_two_segment_interiors() {
        let horizontal = [Point::new(0, 2), Point::new(4, 2)];
        let vertical = [Point::new(2, 0), Point::new(2, 4)];
        assert_eq!(
            perpendicular_interior_crossing(&horizontal, &vertical),
            Some(Point::new(2, 2))
        );

        let touches_endpoint = [Point::new(4, 2), Point::new(4, 5)];
        assert_eq!(
            perpendicular_interior_crossing(&horizontal, &touches_endpoint),
            None
        );
    }

    #[test]
    fn parity_excess_ignores_only_unavoidable_half_cell_error() {
        assert_eq!(parity_excess(1, 2), 0);
        assert_eq!(parity_excess(3, 2), 2);
        assert_eq!(parity_excess(2, 4), 0);
        assert_eq!(parity_excess(6, 4), 4);
    }
}
