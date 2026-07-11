//! Top-level Mermaid document dispatch across diagram-specific engines.

use crate::layout;
use crate::parse::{self, ParseError, Warning};
use crate::route;
use crate::scene::Scene;
use crate::sequence::{self, SequenceDiagram};

#[derive(Debug)]
pub enum Diagram {
    Flowchart(parse::Graph),
    Sequence(SequenceDiagram),
}

pub fn parse(src: &str) -> Result<Diagram, ParseError> {
    let first = src
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("%%"));
    if first.is_some_and(|line| line.split_whitespace().next() == Some("sequenceDiagram")) {
        sequence::parse(src).map(Diagram::Sequence)
    } else {
        parse::parse(src).map(Diagram::Flowchart)
    }
}

impl Diagram {
    pub fn warnings(&self) -> &[Warning] {
        match self {
            Diagram::Flowchart(graph) => &graph.warnings,
            Diagram::Sequence(sequence) => &sequence.warnings,
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Diagram::Flowchart(graph) => graph.nodes.is_empty(),
            Diagram::Sequence(sequence) => sequence.participants.is_empty(),
        }
    }
}

pub fn dump(diagram: &Diagram) -> String {
    match diagram {
        Diagram::Flowchart(graph) => parse::dump(graph),
        Diagram::Sequence(sequence) => sequence::dump(sequence),
    }
}

pub fn scene(diagram: &Diagram, width: usize) -> Scene {
    match diagram {
        Diagram::Flowchart(graph) => {
            let placed = layout::layout(graph, width);
            route::route(graph, &placed)
        }
        Diagram::Sequence(sequence) => sequence::scene(sequence, width),
    }
}
