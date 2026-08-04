//! Top-level Mermaid document dispatch across diagram-specific engines.

use crate::class::{self, ClassDiagram};
use crate::er::{self, ErDiagram};
use crate::layout;
use crate::limits::{self, ResourceLimit};
use crate::mindmap::{self, Mindmap};
use crate::parse::{self, ParseError, Warning};
use crate::route;
use crate::scene::Scene;
use crate::sequence::{self, SequenceDiagram};
use crate::state::{self, StateDiagram};
use crate::timeline::{self, Timeline};

#[derive(Debug)]
pub enum Diagram {
    Flowchart(parse::Graph),
    Sequence(SequenceDiagram),
    State(StateDiagram),
    Class(ClassDiagram),
    Er(ErDiagram),
    Mindmap(Mindmap),
    Timeline(Timeline),
}

pub fn parse(src: &str) -> Result<Diagram, ParseError> {
    limits::validate_source_bytes(src.len()).map_err(resource_parse_error)?;
    let first = src.lines().enumerate().find_map(|(index, line)| {
        let line = line.trim();
        (!line.is_empty() && !line.starts_with("%%")).then_some((index, line))
    });
    let first_line = first.map(|(index, line)| (index + 1, line));
    let diagram = match first_line.map(|(_, line)| line) {
        Some("sequenceDiagram") => sequence::parse(src).map(Diagram::Sequence),
        Some("stateDiagram" | "stateDiagram-v2") => state::parse(src).map(Diagram::State),
        Some("classDiagram") => class::parse(src).map(Diagram::Class),
        Some("erDiagram") => er::parse(src).map(Diagram::Er),
        Some("mindmap") => mindmap::parse(src).map(Diagram::Mindmap),
        Some("timeline") => timeline::parse(src).map(Diagram::Timeline),
        _ => {
            if let Some((line_number, line)) = first_line
                && let Some(header) = unsupported_header(line)
            {
                return Err(ParseError {
                    line: line_number,
                    msg: format!(
                        "diagram type `{header}` is not supported; rewrite it as a supported type, such as `flowchart LR`"
                    ),
                });
            }
            parse::parse(src).map(Diagram::Flowchart)
        }
    }?;
    validate_complexity(&diagram).map_err(resource_parse_error)?;
    Ok(diagram)
}

fn resource_parse_error(limit: ResourceLimit) -> ParseError {
    ParseError {
        // A resource limit can cover the entire document rather than one
        // malformed statement. Line 1 keeps the established source:line
        // diagnostic shape while the message names the precise resource.
        line: 1,
        msg: limit.to_string(),
    }
}

fn validate_complexity(diagram: &Diagram) -> Result<(), ResourceLimit> {
    let elements = match diagram {
        Diagram::Flowchart(graph) => {
            semantic_total([graph.nodes.len(), graph.edges.len(), graph.subgraphs.len()])
        }
        Diagram::Sequence(sequence) => semantic_total([
            sequence.participants.len(),
            sequence.events.len(),
            sequence.controls.len(),
        ]),
        Diagram::State(state) => semantic_total([state.states.len(), state.transitions.len()]),
        Diagram::Class(class) => semantic_total(
            [class.classes.len(), class.relations.len()]
                .into_iter()
                .chain(class.classes.iter().map(|class| class.members.len())),
        ),
        Diagram::Er(er) => semantic_total(
            [er.entities.len(), er.relationships.len()]
                .into_iter()
                .chain(er.entities.iter().map(|entity| entity.attributes.len())),
        ),
        Diagram::Mindmap(mindmap) => mindmap.nodes.len(),
        Diagram::Timeline(timeline) => semantic_total([
            timeline.periods.len(),
            timeline.sections.len(),
            timeline.event_count(),
        ]),
    };
    limits::validate_semantic_elements(elements)?;
    let depth = match diagram {
        Diagram::Flowchart(graph) => flowchart_nesting_depth(graph),
        Diagram::Sequence(sequence) => sequence_nesting_depth(sequence),
        Diagram::Mindmap(mindmap) => mindmap_nesting_depth(mindmap),
        Diagram::State(_) | Diagram::Class(_) | Diagram::Er(_) | Diagram::Timeline(_) => 0,
    };
    limits::validate_nesting_depth(depth)
}

fn semantic_total(values: impl IntoIterator<Item = usize>) -> usize {
    let mut total = 0usize;
    for value in values {
        match total.checked_add(value) {
            Some(next) => total = next,
            None => return usize::MAX,
        }
    }
    total
}

fn flowchart_nesting_depth(graph: &parse::Graph) -> usize {
    let mut maximum = 0usize;
    for index in 0..graph.subgraphs.len() {
        let mut depth = 0usize;
        let mut current = Some(index);
        // Parent indices come from the parser and are acyclic. The explicit
        // bound also prevents a hostile programmatic Graph from looping here.
        while let Some(group) = current {
            depth = depth.saturating_add(1);
            if depth > limits::MAX_NESTING_DEPTH || depth > graph.subgraphs.len() {
                return depth;
            }
            current = graph.subgraphs.get(group).and_then(|value| value.parent);
        }
        maximum = maximum.max(depth);
    }
    maximum
}

fn sequence_nesting_depth(sequence: &SequenceDiagram) -> usize {
    let mut depth = 0usize;
    let mut maximum = 0usize;
    for control in &sequence.controls {
        match control.kind {
            sequence::ControlKind::Start(_, _) => {
                depth = depth.saturating_add(1);
                maximum = maximum.max(depth);
                if maximum > limits::MAX_NESTING_DEPTH {
                    return maximum;
                }
            }
            sequence::ControlKind::End => depth = depth.saturating_sub(1),
            sequence::ControlKind::Else(_) => {}
        }
    }
    maximum
}

fn mindmap_nesting_depth(mindmap: &Mindmap) -> usize {
    let mut maximum = 0usize;
    for node in &mindmap.nodes {
        let depth = node.depth.saturating_add(1);
        maximum = maximum.max(depth);
        if depth > limits::MAX_NESTING_DEPTH {
            return depth;
        }
    }
    maximum
}

/// Return a known Mermaid document header that llmaid deliberately does not
/// implement. Detection stays conservative: a recognized type token followed
/// by flowchart node/edge syntax continues through the flowchart parser.
fn unsupported_header(line: &str) -> Option<&str> {
    let statement = line.split(';').next().unwrap_or(line).trim();
    let mut words = statement.split_whitespace();
    let keyword = words.next()?;
    let rest = statement[keyword.len()..].trim();

    const UNSUPPORTED_HEADERS: &[&str] = &[
        "architecture-beta",
        "block-beta",
        "C4Component",
        "C4Container",
        "C4Context",
        "C4Deployment",
        "C4Dynamic",
        "gantt",
        "gitGraph",
        "journey",
        "kanban",
        "packet-beta",
        "quadrantChart",
        "radar-beta",
        "requirementDiagram",
        "sankey-beta",
        "treemap",
        "treemap-beta",
        "xychart-beta",
        "zenuml",
    ];
    if UNSUPPORTED_HEADERS.contains(&keyword) && !looks_like_flowchart_statement(rest) {
        return Some(keyword);
    }

    if keyword == "pie" && !looks_like_flowchart_statement(rest) {
        return Some(keyword);
    }

    None
}

fn looks_like_flowchart_statement(rest: &str) -> bool {
    matches!(rest.chars().next(), Some('[' | '(' | '{' | '&'))
        || ["--", "-.", "==", "~~~"]
            .iter()
            .any(|operator| rest.starts_with(operator))
}

impl Diagram {
    pub fn warnings(&self) -> &[Warning] {
        match self {
            Diagram::Flowchart(graph) => &graph.warnings,
            Diagram::Sequence(sequence) => &sequence.warnings,
            Diagram::State(state) => &state.warnings,
            Diagram::Class(class) => &class.warnings,
            Diagram::Er(er) => &er.warnings,
            Diagram::Mindmap(mindmap) => &mindmap.warnings,
            Diagram::Timeline(timeline) => &timeline.warnings,
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Diagram::Flowchart(graph) => graph.nodes.is_empty(),
            Diagram::Sequence(sequence) => sequence.participants.is_empty(),
            Diagram::State(state) => state::is_empty(state),
            Diagram::Class(class) => class.is_empty(),
            Diagram::Er(er) => er.is_empty(),
            Diagram::Mindmap(mindmap) => mindmap.is_empty(),
            Diagram::Timeline(timeline) => timeline.is_empty(),
        }
    }
}

pub fn dump(diagram: &Diagram) -> String {
    match diagram {
        Diagram::Flowchart(graph) => parse::dump(graph),
        Diagram::Sequence(sequence) => sequence::dump(sequence),
        Diagram::State(state) => state::dump(state),
        Diagram::Class(class) => class::dump(class),
        Diagram::Er(er) => er::dump(er),
        Diagram::Mindmap(mindmap) => mindmap::dump(mindmap),
        Diagram::Timeline(timeline) => timeline::dump(timeline),
    }
}

pub fn scene(diagram: &Diagram, width: usize) -> Scene {
    match diagram {
        Diagram::Flowchart(graph) => {
            let placed = layout::layout(graph, width);
            route::route(graph, &placed)
        }
        Diagram::Sequence(sequence) => sequence::scene(sequence, width),
        Diagram::State(state) => state::scene(state, width),
        Diagram::Class(class) => class::scene(class, width),
        Diagram::Er(er) => er::scene(er, width),
        Diagram::Mindmap(mindmap) => mindmap::scene(mindmap, width),
        Diagram::Timeline(timeline) => timeline::scene(timeline, width),
    }
}
