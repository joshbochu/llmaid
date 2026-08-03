//! Top-level Mermaid document dispatch across diagram-specific engines.

use crate::class::{self, ClassDiagram};
use crate::er::{self, ErDiagram};
use crate::layout;
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
    let first = src.lines().enumerate().find_map(|(index, line)| {
        let line = line.trim();
        (!line.is_empty() && !line.starts_with("%%")).then_some((index, line))
    });
    let first_line = first.map(|(index, line)| (index + 1, line));
    match first_line.map(|(_, line)| line) {
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
    }
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
