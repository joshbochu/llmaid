//! Top-level Mermaid document dispatch across diagram-specific engines.

use crate::class::{self, ClassDiagram};
use crate::er::{self, ErDiagram};
use crate::gitgraph::{self, GitGraph};
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
    GitGraph(GitGraph),
    Mindmap(Mindmap),
    Timeline(Timeline),
}

pub fn parse(src: &str) -> Result<Diagram, ParseError> {
    let first = src
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("%%"));
    match first.and_then(|line| line.split_whitespace().next()) {
        Some("sequenceDiagram") => sequence::parse(src).map(Diagram::Sequence),
        Some("stateDiagram" | "stateDiagram-v2") => state::parse(src).map(Diagram::State),
        Some("classDiagram") => class::parse(src).map(Diagram::Class),
        Some("erDiagram") => er::parse(src).map(Diagram::Er),
        Some("gitGraph") => gitgraph::parse(src).map(Diagram::GitGraph),
        Some("mindmap") => mindmap::parse(src).map(Diagram::Mindmap),
        Some("timeline") => timeline::parse(src).map(Diagram::Timeline),
        _ => parse::parse(src).map(Diagram::Flowchart),
    }
}

impl Diagram {
    pub fn warnings(&self) -> &[Warning] {
        match self {
            Diagram::Flowchart(graph) => &graph.warnings,
            Diagram::Sequence(sequence) => &sequence.warnings,
            Diagram::State(state) => &state.warnings,
            Diagram::Class(class) => &class.warnings,
            Diagram::Er(er) => &er.warnings,
            Diagram::GitGraph(graph) => &graph.warnings,
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
            Diagram::GitGraph(graph) => graph.is_empty(),
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
        Diagram::GitGraph(graph) => gitgraph::dump(graph),
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
        Diagram::GitGraph(graph) => gitgraph::scene(graph, width),
        Diagram::Mindmap(mindmap) => mindmap::scene(mindmap, width),
        Diagram::Timeline(timeline) => timeline::scene(timeline, width),
    }
}
