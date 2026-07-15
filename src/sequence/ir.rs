//! Ordered semantic representation of a Mermaid sequence diagram.

use std::collections::HashMap;

use crate::parse::Warning;
use crate::scene::EdgeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticipantKind {
    Participant,
    Actor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Participant {
    pub id: String,
    pub label: String,
    pub kind: ParticipantKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Solid,
    Dashed,
}

impl MessageKind {
    pub(super) fn operator(self) -> &'static str {
        match self {
            MessageKind::Solid => "->>",
            MessageKind::Dashed => "-->>",
        }
    }

    pub(super) fn scene_kind(self) -> EdgeKind {
        match self {
            MessageKind::Solid => EdgeKind::Solid,
            MessageKind::Dashed => EdgeKind::Solid,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub from: usize,
    pub to: usize,
    pub label: String,
    pub kind: MessageKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotePosition {
    LeftOf(usize),
    RightOf(usize),
    Over(usize, usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub position: NotePosition,
    pub text: String,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationKind {
    Activate,
    Deactivate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activation {
    pub participant: usize,
    pub kind: ActivationKind,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SequenceEvent {
    Message(Message),
    Note(Note),
    Activation(Activation),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FragmentKind {
    Loop,
    Alt,
    Opt,
}

impl FragmentKind {
    pub(super) fn keyword(self) -> &'static str {
        match self {
            Self::Loop => "loop",
            Self::Alt => "alt",
            Self::Opt => "opt",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlKind {
    Start(FragmentKind, String),
    Else(String),
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlDirective {
    /// Event boundary at which this directive occurs.
    pub at: usize,
    pub kind: ControlKind,
    pub line: usize,
}

/// Declaration order is authoritative for participant columns; source order is
/// authoritative for messages, notes, and activation events.
#[derive(Debug, Default)]
pub struct SequenceDiagram {
    pub participants: Vec<Participant>,
    pub events: Vec<SequenceEvent>,
    pub controls: Vec<ControlDirective>,
    pub warnings: Vec<Warning>,
    pub(super) index: HashMap<String, usize>,
}
