//! Stable semantic geometry and quality inspection for coding agents.

use std::fmt::Write as _;

use crate::diagram::{self, Diagram};
use crate::parse::Endpoint as FlowEndpoint;
use crate::quality::{QualityReport, WitnessValue};
use crate::scene::{
    ArrowHead, CardinalityMaximum, CardinalityMinimum, EdgeKind, EndpointDecorationKind, Point,
    Rect, Scene,
};
use crate::sequence::SequenceEvent;
use crate::style::Style;

/// Render `diagram` into a byte-stable `llmaid.inspect.v1` document.
///
/// Unlike `llmaid.audit.v1`, this intentionally includes semantic scene
/// geometry, raster rows, explicit check applicability, and per-element
/// witnesses. Audit v1 remains unchanged for existing consumers.
pub fn json(diagram: &Diagram, target_width: usize, style: Style) -> String {
    let mut scene = diagram::scene(diagram, target_width);
    scene.normalize();
    let quality = crate::quality::evaluate_with_style(diagram, &scene, target_width, style);
    let bounds = scene.bounds();
    // Inspection remains valid when the final Scene is too large to rasterize:
    // `quality::scene_integrity` records the exact ResourceLimit while this
    // report deliberately emits an empty bounded canvas.
    let raster = crate::render::try_render_scene(&scene, style).ok();

    let mut output = String::new();
    let _ = write!(
        output,
        "{{\"schema\":\"llmaid.inspect.v1\",\"diagram\":\"{}\",\"style\":\"{}\",\"bounds\":",
        diagram_name(diagram),
        if style.ascii { "ascii" } else { "unicode" }
    );
    push_rect_value(
        &mut output,
        Rect::new(0, 0, bounds.w.max(0), bounds.h.max(0)),
    );
    push_summary(&mut output, &quality);
    push_checks(&mut output, &quality);
    push_unclassified(&mut output, &quality);
    push_geometry(&mut output, diagram, &scene);
    push_canvas(&mut output, bounds, raster.as_deref());
    output.push_str("}\n");
    output
}

fn push_summary(output: &mut String, quality: &QualityReport) {
    let _ = write!(
        output,
        ",\"summary\":{{\"checks\":{},\"applicable_instances\":{},\"failed_checks\":{},\"invariant_failed_checks\":{},\"quality_failed_checks\":{},\"unclassified_compositions\":{}}}",
        quality.checks.len(),
        quality.applicable_instances(),
        quality.failed_checks(),
        quality.invariant_failed_checks(),
        quality.quality_failed_checks(),
        quality.unclassified.len()
    );
}

fn push_checks(output: &mut String, quality: &QualityReport) {
    output.push_str(",\"checks\":[");
    for (check_index, check) in quality.checks.iter().enumerate() {
        if check_index != 0 {
            output.push(',');
        }
        let _ = write!(
            output,
            "{{\"id\":\"{}\",\"class\":\"{}\",\"status\":\"{}\",\"applicable\":{},\"failures\":[",
            check.id,
            check.class.name(),
            check.status(),
            check.applicable
        );
        for (failure_index, failure) in check.failures.iter().enumerate() {
            if failure_index != 0 {
                output.push(',');
            }
            output.push_str("{\"elements\":[");
            push_string_values(output, failure.elements.iter().map(String::as_str));
            let _ = write!(
                output,
                "],\"message\":\"{}\",\"witness\":{{",
                json_escape(&failure.message)
            );
            for (field_index, field) in failure.witness.iter().enumerate() {
                if field_index != 0 {
                    output.push(',');
                }
                let _ = write!(output, "\"{}\":", field.name);
                match &field.value {
                    WitnessValue::Integer(value) => {
                        let _ = write!(output, "{value}");
                    }
                    WitnessValue::Text(value) => push_json_string(output, value),
                    WitnessValue::Point(value) => push_point_value(output, *value),
                    WitnessValue::Rect(value) => push_rect_value(output, *value),
                }
            }
            output.push_str("}}");
        }
        output.push_str("]}");
    }
    output.push(']');
}

fn push_unclassified(output: &mut String, quality: &QualityReport) {
    output.push_str(",\"unclassified\":[");
    for (index, value) in quality.unclassified.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        let _ = write!(output, "{{\"kind\":\"{}\",\"elements\":[", value.kind);
        push_string_values(output, value.elements.iter().map(String::as_str));
        let _ = write!(
            output,
            "],\"message\":\"{}\"}}",
            json_escape(&value.message)
        );
    }
    output.push(']');
}

fn push_geometry(output: &mut String, diagram: &Diagram, scene: &Scene) {
    output.push_str(",\"geometry\":{");
    push_boxes(output, diagram, scene);
    push_groups(output, diagram, scene);
    push_paths(output, diagram, scene);
    push_edges(output, diagram, scene);
    push_decorations(output, scene);
    push_texts(output, scene);
    output.push('}');
}

fn push_boxes(output: &mut String, diagram: &Diagram, scene: &Scene) {
    output.push_str("\"boxes\":[");
    let mut count = 0usize;
    for (foreground, boxes) in [(false, &scene.boxes), (true, &scene.foreground_boxes)] {
        for box_ in boxes {
            if count != 0 {
                output.push(',');
            }
            count += 1;
            let _ = write!(
                output,
                "{{\"element\":\"{}\",\"index\":{},\"layer\":\"{}\",\"shape\":\"{}\",\"rect\":",
                json_escape(&box_element(diagram, box_.node, foreground)),
                box_.node,
                if foreground { "foreground" } else { "normal" },
                box_.shape.name()
            );
            push_rect_value(output, box_.rect);
            output.push_str(",\"center2\":");
            push_point_value(output, box_.rect.center2());
            output.push_str(",\"lines\":[");
            push_string_values(output, box_.lines.iter().map(String::as_str));
            output.push(']');
            if let Some(table) = &box_.table {
                output.push_str(",\"table\":{\"title\":");
                push_json_string(output, &table.title);
                output.push_str(",\"row_dividers\":");
                output.push_str(if table.row_dividers { "true" } else { "false" });
                output.push_str(",\"rows\":[");
                for (row_index, row) in table.rows.iter().enumerate() {
                    if row_index != 0 {
                        output.push(',');
                    }
                    output.push('[');
                    push_string_values(output, row.iter().map(String::as_str));
                    output.push(']');
                }
                output.push_str("]}");
            } else {
                output.push_str(",\"table\":null");
            }
            output.push('}');
        }
    }
    output.push(']');
}

fn push_groups(output: &mut String, diagram: &Diagram, scene: &Scene) {
    output.push_str(",\"groups\":[");
    for (index, group) in scene.groups.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        let _ = write!(
            output,
            "{{\"element\":\"{}\",\"index\":{},\"rect\":",
            json_escape(&group_element(diagram, group.subgraph, &group.title.text)),
            group.subgraph
        );
        push_rect_value(output, group.rect);
        output.push_str(",\"title\":");
        push_scene_text(output, &group.title);
        output.push_str(",\"separators\":[");
        for (separator_index, separator) in group.separators.iter().enumerate() {
            if separator_index != 0 {
                output.push(',');
            }
            let _ = write!(output, "{{\"y\":{},\"label\":", separator.y);
            push_scene_text(output, &separator.label);
            output.push('}');
        }
        output.push_str("]}");
    }
    output.push(']');
}

fn push_paths(output: &mut String, diagram: &Diagram, scene: &Scene) {
    output.push_str(",\"paths\":[");
    for (index, path) in scene.paths.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        let _ = write!(
            output,
            "{{\"element\":\"{}\",\"index\":{},\"kind\":\"{}\",\"points\":",
            json_escape(&path_element(diagram, path.path)),
            path.path,
            edge_kind_name(path.kind)
        );
        push_points(output, &path.points);
        output.push_str(",\"rounded\":");
        push_points(output, &path.rounded);
        output.push('}');
    }
    output.push(']');
}

fn push_edges(output: &mut String, diagram: &Diagram, scene: &Scene) {
    output.push_str(",\"edges\":[");
    for (index, edge) in scene.edges.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        let identity = edge_identity(diagram, edge.edge);
        let _ = write!(
            output,
            "{{\"element\":\"{}\",\"index\":{},\"source\":",
            json_escape(&identity.element),
            edge.edge
        );
        push_optional_string(output, identity.source.as_deref());
        output.push_str(",\"target\":");
        push_optional_string(output, identity.target.as_deref());
        let _ = write!(
            output,
            ",\"kind\":\"{}\",\"points\":",
            edge_kind_name(edge.kind)
        );
        push_points(output, &edge.points);
        output.push_str(",\"rounded\":");
        push_points(output, &edge.rounded);
        output.push_str(",\"label\":");
        if let Some(label) = &edge.label {
            push_scene_text(output, label);
        } else {
            output.push_str("null");
        }
        output.push_str(",\"arrow\":");
        if let Some(arrow) = &edge.arrow {
            output.push_str("{\"at\":");
            push_point_value(output, arrow.at);
            output.push_str(",\"toward\":");
            push_point_value(output, arrow.toward);
            let _ = write!(
                output,
                ",\"head\":\"{}\"}}",
                match arrow.head {
                    ArrowHead::Filled => "filled",
                    ArrowHead::Open => "open",
                }
            );
        } else {
            output.push_str("null");
        }
        output.push('}');
    }
    output.push(']');
}

fn push_decorations(output: &mut String, scene: &Scene) {
    output.push_str(",\"decorations\":[");
    for (index, decoration) in scene.endpoint_decorations.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        let _ = write!(
            output,
            "{{\"edge\":{},\"kind\":\"{}\",\"at\":",
            decoration.edge,
            decoration_name(decoration.kind)
        );
        push_point_value(output, decoration.at);
        output.push_str(",\"toward\":");
        push_point_value(output, decoration.toward);
        if let EndpointDecorationKind::Cardinality { minimum, maximum } = decoration.kind {
            let _ = write!(
                output,
                ",\"minimum\":\"{}\",\"maximum\":\"{}\"",
                match minimum {
                    CardinalityMinimum::Zero => "zero",
                    CardinalityMinimum::One => "one",
                },
                match maximum {
                    CardinalityMaximum::One => "one",
                    CardinalityMaximum::Many => "many",
                }
            );
        }
        output.push('}');
    }
    output.push(']');
}

fn push_texts(output: &mut String, scene: &Scene) {
    output.push_str(",\"texts\":[");
    for (index, text) in scene.texts.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        push_scene_text(output, text);
    }
    output.push(']');
}

fn push_canvas(output: &mut String, bounds: Rect, raster: Option<&str>) {
    let (width, height) = if raster.is_some() {
        (bounds.w.max(0), bounds.h.max(0))
    } else {
        (0, 0)
    };
    let _ = write!(
        output,
        ",\"canvas\":{{\"width\":{},\"height\":{},\"rows\":[",
        width, height
    );
    if let Some(raster) = raster {
        push_string_values(output, raster.lines());
    }
    output.push_str("]}");
}

fn push_scene_text(output: &mut String, text: &crate::scene::SceneText) {
    output.push_str("{\"at\":");
    push_point_value(output, text.at);
    let _ = write!(
        output,
        ",\"width\":{},\"height\":{},\"text\":\"{}\"}}",
        text.width(),
        text.height(),
        json_escape(&text.text)
    );
}

fn push_points(output: &mut String, points: &[Point]) {
    output.push('[');
    for (index, point) in points.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        push_point_value(output, *point);
    }
    output.push(']');
}

fn push_point_value(output: &mut String, point: Point) {
    let _ = write!(output, "{{\"x\":{},\"y\":{}}}", point.x, point.y);
}

fn push_rect_value(output: &mut String, rect: Rect) {
    let _ = write!(
        output,
        "{{\"x\":{},\"y\":{},\"width\":{},\"height\":{}}}",
        rect.x, rect.y, rect.w, rect.h
    );
}

fn push_string_values<'a>(output: &mut String, values: impl IntoIterator<Item = &'a str>) {
    for (index, value) in values.into_iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        push_json_string(output, value);
    }
}

fn push_optional_string(output: &mut String, value: Option<&str>) {
    if let Some(value) = value {
        push_json_string(output, value);
    } else {
        output.push_str("null");
    }
}

fn push_json_string(output: &mut String, value: &str) {
    output.push('"');
    output.push_str(&json_escape(value));
    output.push('"');
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
                let _ = write!(escaped, "\\u{:04x}", ch as u32);
            }
            ch => escaped.push(ch),
        }
    }
    escaped
}

fn diagram_name(diagram: &Diagram) -> &'static str {
    match diagram {
        Diagram::Flowchart(_) => "flowchart",
        Diagram::Sequence(_) => "sequence",
        Diagram::State(_) => "state",
        Diagram::Class(_) => "class",
        Diagram::Er(_) => "er",
        Diagram::Mindmap(_) => "mindmap",
        Diagram::Timeline(_) => "timeline",
    }
}

fn box_element(diagram: &Diagram, node: usize, foreground: bool) -> String {
    match diagram {
        Diagram::Flowchart(graph) => graph
            .nodes
            .get(node)
            .map(|value| format!("node:{}", value.id))
            .unwrap_or_else(|| format!("box:{node}")),
        Diagram::Sequence(sequence) => sequence
            .participants
            .get(node)
            .map(|value| format!("participant:{}", value.id))
            .unwrap_or_else(|| {
                format!(
                    "{}:{node}",
                    if foreground {
                        "sequence_overlay"
                    } else {
                        "sequence_box"
                    }
                )
            }),
        Diagram::State(state) => state_box_element(state, node),
        Diagram::Class(class) => class
            .classes
            .get(node)
            .map(|value| format!("class:{}", value.id))
            .unwrap_or_else(|| format!("box:{node}")),
        Diagram::Er(er) => er
            .entities
            .get(node)
            .map(|value| format!("entity:{}", value.id))
            .unwrap_or_else(|| format!("box:{node}")),
        Diagram::Mindmap(_) => format!("mindmap_node:{node}"),
        Diagram::Timeline(_) => format!("timeline_box:{node}"),
    }
}

fn group_element(diagram: &Diagram, group: usize, title: &str) -> String {
    match diagram {
        Diagram::Flowchart(graph) => graph
            .subgraphs
            .get(group)
            .map(|value| format!("group:{}", value.id))
            .unwrap_or_else(|| format!("group:{group}")),
        Diagram::Sequence(_) => format!("fragment:{group}:{title}"),
        Diagram::Timeline(_) => format!("section:{group}"),
        _ => format!("group:{group}"),
    }
}

fn path_element(diagram: &Diagram, path: usize) -> String {
    match diagram {
        Diagram::Sequence(sequence) => sequence
            .participants
            .get(path)
            .map(|value| format!("lifeline:{}", value.id))
            .unwrap_or_else(|| format!("path:{path}")),
        Diagram::Timeline(_) => format!("timeline_spine:{path}"),
        _ => format!("path:{path}"),
    }
}

struct EdgeIdentity {
    element: String,
    source: Option<String>,
    target: Option<String>,
}

fn edge_identity(diagram: &Diagram, edge: usize) -> EdgeIdentity {
    match diagram {
        Diagram::Flowchart(graph) => graph.edges.get(edge).map_or_else(
            || anonymous_edge(edge),
            |value| EdgeIdentity {
                element: format!("edge:{edge}"),
                source: Some(flow_endpoint_element(graph, value.source)),
                target: Some(flow_endpoint_element(graph, value.target)),
            },
        ),
        Diagram::Sequence(sequence) => sequence
            .events
            .iter()
            .enumerate()
            .filter_map(|(event, value)| match value {
                SequenceEvent::Message(message) => Some((event, message)),
                _ => None,
            })
            .nth(edge)
            .map_or_else(
                || anonymous_edge(edge),
                |(event, message)| EdgeIdentity {
                    element: format!("message:{event}"),
                    source: Some(format!(
                        "participant:{}",
                        sequence.participants[message.from].id
                    )),
                    target: Some(format!(
                        "participant:{}",
                        sequence.participants[message.to].id
                    )),
                },
            ),
        Diagram::State(state) => state_edge_identity(state, edge),
        Diagram::Class(class) => class.relations.get(edge).map_or_else(
            || anonymous_edge(edge),
            |relation| EdgeIdentity {
                element: format!("relation:{edge}"),
                source: Some(format!("class:{}", class.classes[relation.from].id)),
                target: Some(format!("class:{}", class.classes[relation.to].id)),
            },
        ),
        Diagram::Er(er) => er.relationships.get(edge).map_or_else(
            || anonymous_edge(edge),
            |relationship| EdgeIdentity {
                element: format!("relationship:{edge}"),
                source: Some(format!("entity:{}", er.entities[relationship.from].id)),
                target: Some(format!("entity:{}", er.entities[relationship.to].id)),
            },
        ),
        Diagram::Mindmap(mindmap) => {
            let child = edge + 1;
            let parent = mindmap.nodes.get(child).and_then(|value| value.parent);
            EdgeIdentity {
                element: format!("mindmap_edge:{edge}"),
                source: parent.map(|node| format!("mindmap_node:{node}")),
                target: Some(format!("mindmap_node:{child}")),
            }
        }
        Diagram::Timeline(timeline) => timeline_edge_identity(timeline, edge),
    }
}

fn flow_endpoint_element(graph: &crate::parse::Graph, endpoint: FlowEndpoint) -> String {
    match endpoint {
        FlowEndpoint::Node(node) => format!("node:{}", graph.nodes[node].id),
        FlowEndpoint::Subgraph(group) => format!("group:{}", graph.subgraphs[group].id),
    }
}

fn anonymous_edge(edge: usize) -> EdgeIdentity {
    EdgeIdentity {
        element: format!("edge:{edge}"),
        source: None,
        target: None,
    }
}

fn state_edge_identity(state: &crate::state::StateDiagram, edge: usize) -> EdgeIdentity {
    let mut next_marker = state.states.len();
    for (index, transition) in state.transitions.iter().enumerate() {
        let source = match transition.from {
            crate::state::Endpoint::State(state) => state,
            crate::state::Endpoint::Marker => {
                let marker = next_marker;
                next_marker += 1;
                marker
            }
        };
        let target = match transition.to {
            crate::state::Endpoint::State(state) => state,
            crate::state::Endpoint::Marker => {
                let marker = next_marker;
                next_marker += 1;
                marker
            }
        };
        if index == edge {
            return EdgeIdentity {
                element: format!("transition:{edge}"),
                source: Some(state_box_element(state, source)),
                target: Some(state_box_element(state, target)),
            };
        }
    }
    anonymous_edge(edge)
}

fn state_box_element(state: &crate::state::StateDiagram, node: usize) -> String {
    state
        .states
        .get(node)
        .map(|value| format!("state:{}", value.id))
        .unwrap_or_else(|| format!("marker:{}", node - state.states.len()))
}

fn timeline_edge_identity(timeline: &crate::timeline::Timeline, edge: usize) -> EdgeIdentity {
    let mut candidate = 0usize;
    for (period, value) in timeline.periods.iter().enumerate() {
        if candidate == edge {
            return EdgeIdentity {
                element: format!("period_connector:{period}"),
                source: Some(format!("period:{period}")),
                target: Some("timeline:spine".to_string()),
            };
        }
        candidate += 1;
        for event in 0..value.events.len() {
            if candidate == edge {
                return EdgeIdentity {
                    element: format!("event_connector:{period}:{event}"),
                    source: Some("timeline:spine".to_string()),
                    target: Some(format!("event:{period}:{event}")),
                };
            }
            candidate += 1;
        }
    }
    anonymous_edge(edge)
}

fn edge_kind_name(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Solid => "solid",
        EdgeKind::Dotted => "dotted",
        EdgeKind::Thick => "thick",
    }
}

fn decoration_name(kind: EndpointDecorationKind) -> &'static str {
    match kind {
        EndpointDecorationKind::Arrow => "arrow",
        EndpointDecorationKind::Circle => "circle",
        EndpointDecorationKind::Cross => "cross",
        EndpointDecorationKind::OpenArrow => "open_arrow",
        EndpointDecorationKind::OpenTriangle => "open_triangle",
        EndpointDecorationKind::OpenDiamond => "open_diamond",
        EndpointDecorationKind::FilledDiamond => "filled_diamond",
        EndpointDecorationKind::Cardinality { .. } => "cardinality",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspection_is_stable_and_contains_semantic_geometry_and_raster_rows() {
        let diagram = crate::diagram::parse("flowchart LR\nA --> B\n").unwrap();
        let first = json(&diagram, 100, Style { ascii: false });
        let second = json(&diagram, 100, Style { ascii: false });

        assert_eq!(first, second);
        assert!(first.starts_with("{\"schema\":\"llmaid.inspect.v1\""));
        assert!(first.contains("\"id\":\"flow.mono_centerline\""));
        assert!(first.contains("\"element\":\"node:A\""));
        assert!(first.contains("\"source\":\"node:A\",\"target\":\"node:B\""));
        assert!(first.contains("\"canvas\":{\"width\":15,\"height\":3,\"rows\":["));
    }

    #[test]
    fn state_marker_edge_references_match_emitted_box_identities() {
        let diagram =
            crate::diagram::parse("stateDiagram-v2\n[*] --> Ready\nReady --> [*]\n").unwrap();
        let report = json(&diagram, 100, Style { ascii: false });

        assert!(report.contains("\"element\":\"marker:0\""), "{report}");
        assert!(report.contains("\"element\":\"marker:1\""), "{report}");
        assert!(
            report.contains("\"source\":\"marker:0\",\"target\":\"state:Ready\""),
            "{report}"
        );
        assert!(
            report.contains("\"source\":\"state:Ready\",\"target\":\"marker:1\""),
            "{report}"
        );
    }
}
