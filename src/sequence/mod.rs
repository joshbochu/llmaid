//! Mermaid sequence-diagram engine: semantic IR, parser, layout, and dump.

mod dump;
mod ir;
mod layout;
mod parse;

pub use dump::dump;
pub use ir::{
    Activation, ActivationKind, ControlDirective, ControlKind, FragmentKind, Message, MessageHead,
    MessageKind, Note, NotePosition, Participant, ParticipantKind, SequenceDiagram, SequenceEvent,
};
pub use layout::scene;
pub use parse::parse;
