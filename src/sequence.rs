//! Mermaid sequence-diagram subset -> ordered semantic events -> shared `Scene`.

use std::collections::HashMap;

use unicode_width::UnicodeWidthStr;

use crate::parse::{ParseError, Warning};
use crate::scene::{
    Arrow, ArrowHead, EdgeKind, Point, Rect, RoutedEdge, Scene, SceneBox, SceneGroup, ScenePath,
    SceneText, Shape,
};

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
    fn operator(self) -> &'static str {
        match self {
            MessageKind::Solid => "->>",
            MessageKind::Dashed => "-->>",
        }
    }

    fn scene_kind(self) -> EdgeKind {
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
    fn keyword(self) -> &'static str {
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
    index: HashMap<String, usize>,
}

impl SequenceDiagram {
    fn participant(
        &mut self,
        id: &str,
        declaration: Option<(String, ParticipantKind)>,
        line: usize,
    ) -> usize {
        if let Some(&index) = self.index.get(id) {
            if let Some((label, kind)) = declaration {
                let participant = &mut self.participants[index];
                if participant.label != id && participant.label != label {
                    self.warnings.push(Warning {
                        line,
                        msg: format!(
                            "participant `{id}` redeclared; last label wins (was \"{}\")",
                            participant.label
                        ),
                    });
                }
                participant.label = label;
                participant.kind = kind;
            }
            return index;
        }

        let (label, kind) =
            declaration.unwrap_or_else(|| (id.to_string(), ParticipantKind::Participant));
        let index = self.participants.len();
        self.participants.push(Participant {
            id: id.to_string(),
            label,
            kind,
        });
        self.index.insert(id.to_string(), index);
        index
    }
}

pub fn parse(src: &str) -> Result<SequenceDiagram, ParseError> {
    let mut sequence = SequenceDiagram::default();
    let mut seen_header = false;
    let mut active: Vec<Vec<usize>> = Vec::new();
    let mut fragments: Vec<(FragmentKind, usize, bool)> = Vec::new();

    for (line_index, raw_line) in src.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }

        if !seen_header {
            if line != "sequenceDiagram" {
                return Err(ParseError {
                    line: line_number,
                    msg: "expected `sequenceDiagram` header".to_string(),
                });
            }
            seen_header = true;
            continue;
        }

        if line == "sequenceDiagram" {
            sequence.warnings.push(Warning {
                line: line_number,
                msg: "duplicate `sequenceDiagram` header ignored".to_string(),
            });
            continue;
        }

        let keyword = line.split_whitespace().next().unwrap_or("");
        if keyword == "participant" || keyword == "actor" {
            parse_participant(&mut sequence, line, line_number, keyword)?;
        } else if keyword == "Note" {
            parse_note(&mut sequence, line, line_number)?;
        } else if matches!(keyword, "loop" | "alt" | "opt") {
            parse_fragment_start(&mut sequence, &mut fragments, line, line_number, keyword)?;
        } else if keyword == "else" {
            parse_fragment_else(&mut sequence, &mut fragments, line, line_number)?;
        } else if keyword == "end" {
            parse_fragment_end(&mut sequence, &mut fragments, line, line_number)?;
        } else if keyword == "activate" || keyword == "deactivate" {
            parse_activation(&mut sequence, &mut active, line, line_number, keyword)?;
        } else {
            parse_message(&mut sequence, line, line_number)?;
        }
    }

    if !seen_header {
        return Err(ParseError {
            line: 1,
            msg: "expected `sequenceDiagram` header".to_string(),
        });
    }
    if let Some((kind, line, _)) = fragments.first() {
        return Err(ParseError {
            line: *line,
            msg: format!("expected a matching `end` for `{}` block", kind.keyword()),
        });
    }
    if let Some((participant, line)) = active
        .iter()
        .enumerate()
        .flat_map(|(participant, lines)| lines.iter().map(move |&line| (participant, line)))
        .min_by_key(|&(_, line)| line)
    {
        return Err(ParseError {
            line,
            msg: format!(
                "expected a matching `deactivate {}`",
                sequence.participants[participant].id
            ),
        });
    }
    Ok(sequence)
}

fn parse_fragment_start(
    sequence: &mut SequenceDiagram,
    fragments: &mut Vec<(FragmentKind, usize, bool)>,
    line: &str,
    line_number: usize,
    keyword: &str,
) -> Result<(), ParseError> {
    let label = line[keyword.len()..].trim();
    if label.is_empty() {
        return Err(ParseError {
            line: line_number,
            msg: format!("expected a label after `{keyword}`"),
        });
    }
    let kind = match keyword {
        "loop" => FragmentKind::Loop,
        "alt" => FragmentKind::Alt,
        "opt" => FragmentKind::Opt,
        _ => unreachable!(),
    };
    sequence.controls.push(ControlDirective {
        at: sequence.events.len(),
        kind: ControlKind::Start(kind, label.to_string()),
        line: line_number,
    });
    fragments.push((kind, line_number, false));
    Ok(())
}

fn parse_fragment_else(
    sequence: &mut SequenceDiagram,
    fragments: &mut [(FragmentKind, usize, bool)],
    line: &str,
    line_number: usize,
) -> Result<(), ParseError> {
    let label = line["else".len()..].trim();
    if label.is_empty() {
        return Err(ParseError {
            line: line_number,
            msg: "expected a label after `else`".to_string(),
        });
    }
    let Some((kind, _, seen_else)) = fragments.last_mut() else {
        return Err(ParseError {
            line: line_number,
            msg: "`else` is only valid inside an `alt` block".to_string(),
        });
    };
    if *kind != FragmentKind::Alt {
        return Err(ParseError {
            line: line_number,
            msg: "`else` is only valid inside an `alt` block".to_string(),
        });
    }
    if *seen_else {
        return Err(ParseError {
            line: line_number,
            msg: "expected only one `else` in an `alt` block".to_string(),
        });
    }
    *seen_else = true;
    sequence.controls.push(ControlDirective {
        at: sequence.events.len(),
        kind: ControlKind::Else(label.to_string()),
        line: line_number,
    });
    Ok(())
}

fn parse_fragment_end(
    sequence: &mut SequenceDiagram,
    fragments: &mut Vec<(FragmentKind, usize, bool)>,
    line: &str,
    line_number: usize,
) -> Result<(), ParseError> {
    if line != "end" {
        return Err(ParseError {
            line: line_number,
            msg: "expected `end` with no trailing text".to_string(),
        });
    }
    if fragments.pop().is_none() {
        return Err(ParseError {
            line: line_number,
            msg: "expected `loop`, `alt`, or `opt` before `end`".to_string(),
        });
    }
    sequence.controls.push(ControlDirective {
        at: sequence.events.len(),
        kind: ControlKind::End,
        line: line_number,
    });
    Ok(())
}

fn parse_participant(
    sequence: &mut SequenceDiagram,
    line: &str,
    line_number: usize,
    keyword: &str,
) -> Result<(), ParseError> {
    let rest = line[keyword.len()..].trim();
    let (id, label) = if let Some((id, label)) = rest.split_once(" as ") {
        (id.trim(), unquote(label.trim()))
    } else {
        (rest, rest.to_string())
    };
    if !valid_id(id) {
        return Err(ParseError {
            line: line_number,
            msg: "expected a participant id such as `Client`".to_string(),
        });
    }
    if label.is_empty() {
        return Err(ParseError {
            line: line_number,
            msg: "expected a participant label after `as`".to_string(),
        });
    }
    let kind = if keyword == "actor" {
        ParticipantKind::Actor
    } else {
        ParticipantKind::Participant
    };
    sequence.participant(id, Some((label, kind)), line_number);
    Ok(())
}

fn parse_note(
    sequence: &mut SequenceDiagram,
    line: &str,
    line_number: usize,
) -> Result<(), ParseError> {
    let Some((head, text)) = line.split_once(':') else {
        return Err(ParseError {
            line: line_number,
            msg: "expected `:` followed by a note label".to_string(),
        });
    };
    let rest = head.strip_prefix("Note").unwrap_or("").trim();
    let (placement, target) = if let Some(target) = rest.strip_prefix("left of ") {
        ("left", target.trim())
    } else if let Some(target) = rest.strip_prefix("right of ") {
        ("right", target.trim())
    } else if let Some(target) = rest.strip_prefix("over ") {
        ("over", target.trim())
    } else {
        return Err(ParseError {
            line: line_number,
            msg: "expected `left of`, `right of`, or `over` after `Note`".to_string(),
        });
    };
    let text = text.trim();
    if text.is_empty() {
        return Err(ParseError {
            line: line_number,
            msg: "expected a note label after `:`".to_string(),
        });
    }

    let ids: Vec<&str> = target.split(',').map(str::trim).collect();
    let expected_count = if placement == "over" { 1..=2 } else { 1..=1 };
    if !expected_count.contains(&ids.len()) || ids.iter().any(|id| !valid_id(id)) {
        return Err(ParseError {
            line: line_number,
            msg: if placement == "over" {
                "expected one participant or two comma-separated participants after `over`"
                    .to_string()
            } else {
                format!("expected one participant after `{placement} of`")
            },
        });
    }
    let mut participants = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(&participant) = sequence.index.get(id) else {
            return Err(ParseError {
                line: line_number,
                msg: format!(
                    "unknown participant `{id}`; declare it or reference it in a message first"
                ),
            });
        };
        participants.push(participant);
    }
    let position = match placement {
        "left" => NotePosition::LeftOf(participants[0]),
        "right" => NotePosition::RightOf(participants[0]),
        "over" => NotePosition::Over(
            participants[0],
            *participants.get(1).unwrap_or(&participants[0]),
        ),
        _ => unreachable!(),
    };
    sequence.events.push(SequenceEvent::Note(Note {
        position,
        text: text.to_string(),
        line: line_number,
    }));
    Ok(())
}

fn parse_activation(
    sequence: &mut SequenceDiagram,
    active: &mut Vec<Vec<usize>>,
    line: &str,
    line_number: usize,
    keyword: &str,
) -> Result<(), ParseError> {
    let mut words = line.split_whitespace();
    let _ = words.next();
    let id = words.next().unwrap_or("");
    if !valid_id(id) || words.next().is_some() {
        return Err(ParseError {
            line: line_number,
            msg: format!("expected one participant id after `{keyword}`"),
        });
    }
    let Some(&participant) = sequence.index.get(id) else {
        return Err(ParseError {
            line: line_number,
            msg: format!(
                "unknown participant `{id}`; declare it or reference it in a message first"
            ),
        });
    };
    active.resize_with(sequence.participants.len(), Vec::new);
    let kind = if keyword == "activate" {
        active[participant].push(line_number);
        ActivationKind::Activate
    } else {
        if active[participant].pop().is_none() {
            return Err(ParseError {
                line: line_number,
                msg: format!("expected a matching `activate {id}` before `deactivate {id}`"),
            });
        }
        ActivationKind::Deactivate
    };
    sequence.events.push(SequenceEvent::Activation(Activation {
        participant,
        kind,
        line: line_number,
    }));
    Ok(())
}

fn parse_message(
    sequence: &mut SequenceDiagram,
    line: &str,
    line_number: usize,
) -> Result<(), ParseError> {
    let Some((head, label)) = line.split_once(':') else {
        if line.contains("->>") || line.contains("-->>") {
            return Err(ParseError {
                line: line_number,
                msg: "expected `:` followed by a message label".to_string(),
            });
        }
        return Err(message_arrow_error(line_number, line));
    };

    let (from, to, kind) = if let Some((from, to)) = head.split_once("-->>") {
        (from.trim(), to.trim(), MessageKind::Dashed)
    } else if let Some((from, to)) = head.split_once("->>") {
        (from.trim(), to.trim(), MessageKind::Solid)
    } else {
        return Err(message_arrow_error(line_number, head));
    };

    if !valid_id(from) || !valid_id(to) {
        return Err(ParseError {
            line: line_number,
            msg: "expected participant ids on both sides of the message arrow".to_string(),
        });
    }
    let label = label.trim();
    if label.is_empty() {
        return Err(ParseError {
            line: line_number,
            msg: "expected a message label after `:`".to_string(),
        });
    }

    let from = sequence.participant(from, None, line_number);
    let to = sequence.participant(to, None, line_number);
    sequence.events.push(SequenceEvent::Message(Message {
        from,
        to,
        label: label.to_string(),
        kind,
    }));
    Ok(())
}

fn message_arrow_error(line: usize, found: &str) -> ParseError {
    ParseError {
        line,
        msg: format!(
            "expected a message arrow `->>` or `-->>`, found `{}`",
            found.trim()
        ),
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

fn unquote(text: &str) -> String {
    text.strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .unwrap_or(text)
        .to_string()
}

pub fn dump(sequence: &SequenceDiagram) -> String {
    let mut output = String::from("type: sequence\nparticipants:\n");
    for participant in &sequence.participants {
        let kind = match participant.kind {
            ParticipantKind::Participant => "participant",
            ParticipantKind::Actor => "actor",
        };
        output.push_str(&format!(
            "  {kind} {} \"{}\"\n",
            participant.id,
            participant.label.replace('"', "\\\"")
        ));
    }
    output.push_str("events:\n");
    let mut controls = sequence.controls.iter().peekable();
    for (event_index, event) in sequence.events.iter().enumerate() {
        while controls
            .peek()
            .is_some_and(|control| control.at == event_index)
        {
            dump_control(&mut output, controls.next().unwrap());
        }
        match event {
            SequenceEvent::Message(message) => output.push_str(&format!(
                "  message {} {} {} \"{}\"\n",
                sequence.participants[message.from].id,
                message.kind.operator(),
                sequence.participants[message.to].id,
                message.label.replace('"', "\\\"")
            )),
            SequenceEvent::Note(note) => {
                let position = match note.position {
                    NotePosition::LeftOf(participant) => {
                        format!("left of {}", sequence.participants[participant].id)
                    }
                    NotePosition::RightOf(participant) => {
                        format!("right of {}", sequence.participants[participant].id)
                    }
                    NotePosition::Over(first, second) if first == second => {
                        format!("over {}", sequence.participants[first].id)
                    }
                    NotePosition::Over(first, second) => format!(
                        "over {},{}",
                        sequence.participants[first].id, sequence.participants[second].id
                    ),
                };
                output.push_str(&format!(
                    "  note {position} \"{}\"\n",
                    note.text.replace('"', "\\\"")
                ));
            }
            SequenceEvent::Activation(activation) => {
                let keyword = match activation.kind {
                    ActivationKind::Activate => "activate",
                    ActivationKind::Deactivate => "deactivate",
                };
                output.push_str(&format!(
                    "  {keyword} {}\n",
                    sequence.participants[activation.participant].id
                ));
            }
        }
    }
    for control in controls {
        dump_control(&mut output, control);
    }
    if !sequence.warnings.is_empty() {
        output.push_str("warnings:\n");
        for warning in &sequence.warnings {
            output.push_str(&format!("  line {}: {}\n", warning.line, warning.msg));
        }
    }
    output
}

fn dump_control(output: &mut String, control: &ControlDirective) {
    match &control.kind {
        ControlKind::Start(kind, label) => output.push_str(&format!(
            "  {} \"{}\"\n",
            kind.keyword(),
            label.replace('"', "\\\"")
        )),
        ControlKind::Else(label) => {
            output.push_str(&format!("  else \"{}\"\n", label.replace('"', "\\\"")))
        }
        ControlKind::End => output.push_str("  end\n"),
    }
}

/// Lay out the sequence subset directly into the shared terminal scene.
/// Width is accepted at the engine boundary; the first subset uses B9's final
/// overflow step rather than truncating or rejecting intrinsically wide input.
pub fn scene(sequence: &SequenceDiagram, _width: usize) -> Scene {
    if sequence.participants.is_empty() {
        return Scene::default();
    }

    let widths: Vec<i32> = sequence
        .participants
        .iter()
        .map(|participant| participant.label.width() as i32 + 4)
        .collect();
    let mut gaps: Vec<i32> = widths
        .windows(2)
        .map(|pair| (pair[0] + 1) / 2 + pair[1] / 2 + 6)
        .collect();

    let mut activation_depths = vec![0usize; sequence.participants.len()];
    let mut max_activation_depths = activation_depths.clone();
    for event in &sequence.events {
        let SequenceEvent::Activation(activation) = event else {
            continue;
        };
        match activation.kind {
            ActivationKind::Activate => {
                activation_depths[activation.participant] += 1;
                max_activation_depths[activation.participant] = max_activation_depths
                    [activation.participant]
                    .max(activation_depths[activation.participant]);
            }
            ActivationKind::Deactivate => activation_depths[activation.participant] -= 1,
        }
    }

    for event in &sequence.events {
        match event {
            SequenceEvent::Message(message) => {
                let low = message.from.min(message.to);
                let high = message.from.max(message.to);
                if low == high {
                    continue;
                }
                let current: i32 = gaps[low..high].iter().sum();
                let required = message.label.width() as i32 + 6;
                if required > current {
                    gaps[high - 1] += required - current;
                }
            }
            SequenceEvent::Note(note) => {
                let note_width = note.text.width() as i32 + 4;
                let gap = match note.position {
                    NotePosition::LeftOf(participant) if participant > 0 => Some(participant - 1),
                    NotePosition::RightOf(participant)
                        if participant + 1 < sequence.participants.len() =>
                    {
                        Some(participant)
                    }
                    _ => None,
                };
                if let Some(gap) = gap {
                    let participant = match note.position {
                        NotePosition::LeftOf(participant) | NotePosition::RightOf(participant) => {
                            participant
                        }
                        NotePosition::Over(_, _) => unreachable!(),
                    };
                    gaps[gap] = gaps[gap]
                        .max(note_width + 4 + max_activation_depths[participant] as i32 * 2);
                }
            }
            SequenceEvent::Activation(_) => {}
        }
    }

    let mut centers = vec![widths[0] / 2];
    for gap in gaps {
        centers.push(centers.last().copied().unwrap() + gap);
    }

    // `next_top` is the first row available to an event. Message labels own
    // that row and their shafts use the next one; note boxes own three rows.
    // This keeps the core message cadence unchanged while packing adjacent
    // notes without empty event rows.
    let mut next_top = 4;
    let mut last_content_bottom = 2;
    let mut lifeline_bottom = 5;
    let mut active_rows: Vec<Vec<i32>> = vec![Vec::new(); sequence.participants.len()];
    let mut event_rows = Vec::with_capacity(sequence.events.len());
    let mut control_rows = Vec::with_capacity(sequence.controls.len());
    let mut control_index = 0;
    let mut fragment_rows = Vec::new();
    for event_index in 0..=sequence.events.len() {
        while control_index < sequence.controls.len()
            && sequence.controls[control_index].at == event_index
        {
            let row = match &sequence.controls[control_index].kind {
                ControlKind::Start(_, _) => {
                    let row = next_top;
                    fragment_rows.push(row);
                    next_top = row + 2;
                    last_content_bottom = row + 1;
                    lifeline_bottom = lifeline_bottom.max(row + 1);
                    row
                }
                ControlKind::Else(_) => {
                    let row = next_top.max(last_content_bottom + 2);
                    next_top = row + 2;
                    last_content_bottom = row + 1;
                    lifeline_bottom = lifeline_bottom.max(row + 1);
                    row
                }
                ControlKind::End => {
                    let start = fragment_rows
                        .pop()
                        .expect("parser guarantees balanced control blocks");
                    let row = next_top.max(last_content_bottom + 2).max(start + 2);
                    next_top = row + 1;
                    last_content_bottom = row;
                    lifeline_bottom = lifeline_bottom.max(row + 1);
                    row
                }
            };
            control_rows.push(row);
            control_index += 1;
        }
        let Some(event) = sequence.events.get(event_index) else {
            break;
        };
        let row = match event {
            SequenceEvent::Message(message) => {
                let row = next_top + 1;
                let self_loop = message.from == message.to;
                let bottom = row + if self_loop { 2 } else { 0 };
                last_content_bottom = bottom;
                next_top += if self_loop { 5 } else { 3 };
                lifeline_bottom = lifeline_bottom.max(bottom + 1);
                row
            }
            SequenceEvent::Note(_) => {
                let row = next_top;
                last_content_bottom = row + 2;
                next_top = row + 3;
                lifeline_bottom = lifeline_bottom.max(row + 3);
                row
            }
            SequenceEvent::Activation(activation) => match activation.kind {
                ActivationKind::Activate => {
                    let row = last_content_bottom + 1;
                    active_rows[activation.participant].push(row);
                    next_top = next_top.max(row + 1);
                    row
                }
                ActivationKind::Deactivate => {
                    let start = active_rows[activation.participant]
                        .pop()
                        .expect("parser guarantees balanced activation events");
                    let row = (last_content_bottom + 1).max(start + 2);
                    next_top = next_top.max(row + 1);
                    row
                }
            },
        };
        event_rows.push(row);
    }

    let boxes = sequence
        .participants
        .iter()
        .enumerate()
        .map(|(index, participant)| SceneBox {
            node: index,
            rect: Rect::new(centers[index] - widths[index] / 2, 0, widths[index], 3),
            lines: vec![participant.label.clone()],
            shape: Shape::Rect,
            table: None,
        })
        .collect();
    let mut next_box_id = sequence.participants.len();
    let mut note_boxes = Vec::new();
    let mut active_note_depths = vec![0usize; sequence.participants.len()];
    for (event, &y) in sequence.events.iter().zip(&event_rows) {
        if let SequenceEvent::Activation(activation) = event {
            match activation.kind {
                ActivationKind::Activate => active_note_depths[activation.participant] += 1,
                ActivationKind::Deactivate => active_note_depths[activation.participant] -= 1,
            }
            continue;
        }
        let SequenceEvent::Note(note) = event else {
            continue;
        };
        let text_width = note.text.width() as i32;
        let (x, width) = match note.position {
            NotePosition::LeftOf(participant) => {
                let width = text_width + 4;
                let left = if active_note_depths[participant] == 0 {
                    centers[participant]
                } else {
                    centers[participant] - 1
                };
                (left - width - 2, width)
            }
            NotePosition::RightOf(participant) => {
                let width = text_width + 4;
                let right = if active_note_depths[participant] == 0 {
                    centers[participant]
                } else {
                    centers[participant] + 1 + (active_note_depths[participant] as i32 - 1) * 2
                };
                (right + 2, width)
            }
            NotePosition::Over(first, second) => {
                let left = centers[first].min(centers[second]);
                let right = centers[first].max(centers[second]);
                let width = (text_width + 4).max(right - left + 5);
                ((left + right - width) / 2, width)
            }
        };
        note_boxes.push(SceneBox {
            node: next_box_id,
            rect: Rect::new(x, y, width, 3),
            lines: vec![note.text.clone()],
            shape: Shape::Rect,
            table: None,
        });
        next_box_id += 1;
    }

    let mut active: Vec<Vec<(i32, usize)>> = vec![Vec::new(); sequence.participants.len()];
    let mut activation_boxes = Vec::new();
    for (event, &y) in sequence.events.iter().zip(&event_rows) {
        let SequenceEvent::Activation(activation) = event else {
            continue;
        };
        match activation.kind {
            ActivationKind::Activate => {
                let depth = active[activation.participant].len();
                active[activation.participant].push((y, depth));
            }
            ActivationKind::Deactivate => {
                let (start, depth) = active[activation.participant]
                    .pop()
                    .expect("parser guarantees balanced activation events");
                let bottom = y.max(start + 2);
                let rect = Rect::new(
                    centers[activation.participant] - 1 + depth as i32 * 2,
                    start,
                    3,
                    bottom - start + 1,
                );
                lifeline_bottom = lifeline_bottom.max(rect.bottom());
                activation_boxes.push((
                    depth,
                    SceneBox {
                        node: next_box_id,
                        rect,
                        lines: Vec::new(),
                        shape: Shape::Rect,
                        table: None,
                    },
                ));
                next_box_id += 1;
            }
        }
    }
    activation_boxes.sort_by_key(|(depth, box_)| (*depth, box_.node));
    let mut foreground_boxes: Vec<SceneBox> =
        activation_boxes.into_iter().map(|(_, box_)| box_).collect();
    foreground_boxes.extend(note_boxes);

    let paths = centers
        .iter()
        .enumerate()
        .map(|(index, &center)| ScenePath {
            path: index,
            points: vec![Point::new(center, 2), Point::new(center, lifeline_bottom)],
            rounded: Vec::new(),
            kind: EdgeKind::Dotted,
        })
        .collect();

    let mut edges = Vec::new();
    let mut active_depths = vec![0; sequence.participants.len()];
    for (event, &y) in sequence.events.iter().zip(&event_rows) {
        if let SequenceEvent::Activation(activation) = event {
            match activation.kind {
                ActivationKind::Activate => active_depths[activation.participant] += 1,
                ActivationKind::Deactivate => active_depths[activation.participant] -= 1,
            }
            continue;
        }
        let SequenceEvent::Message(message) = event else {
            continue;
        };
        let raw_source_x = centers[message.from];
        let raw_target_x = centers[message.to];
        let self_message = message.from == message.to;
        let direction = if self_message {
            1
        } else {
            (raw_target_x - raw_source_x).signum()
        };
        let source_x = active_attachment(raw_source_x, active_depths[message.from], direction);
        let target_x = if self_message {
            source_x
        } else {
            active_attachment(raw_target_x, active_depths[message.to], -direction)
        };
        let (points, rounded, arrow) = if self_message {
            let loop_x = source_x + message.label.width() as i32 + 5;
            let arrow_at = Point::new(source_x + 1, y + 2);
            (
                vec![
                    Point::new(source_x, y),
                    Point::new(loop_x, y),
                    Point::new(loop_x, y + 2),
                    arrow_at,
                ],
                vec![Point::new(loop_x, y), Point::new(loop_x, y + 2)],
                Arrow {
                    at: arrow_at,
                    toward: Point::new(source_x, y + 2),
                    head: if message.kind == MessageKind::Dashed {
                        ArrowHead::Open
                    } else {
                        ArrowHead::Filled
                    },
                },
            )
        } else {
            let step = (target_x - source_x).signum();
            let arrow_at = Point::new(target_x - step, y);
            (
                vec![Point::new(source_x, y), arrow_at],
                Vec::new(),
                Arrow {
                    at: arrow_at,
                    toward: Point::new(target_x, y),
                    head: if message.kind == MessageKind::Dashed {
                        ArrowHead::Open
                    } else {
                        ArrowHead::Filled
                    },
                },
            )
        };
        let label_x = if self_message {
            source_x + 2
        } else {
            (source_x + target_x - message.label.width() as i32) / 2
        };
        let edge = RoutedEdge {
            edge: edges.len(),
            points,
            rounded,
            kind: message.kind.scene_kind(),
            label: Some(SceneText::new(Point::new(label_x, y - 1), &message.label)),
            arrow: Some(arrow),
        };
        edges.push(edge);
    }

    struct OpenFragment {
        kind: FragmentKind,
        title: String,
        segment_start: i32,
        depth: usize,
        else_branch: Option<(String, i32)>,
    }
    let base_left = sequence
        .participants
        .iter()
        .enumerate()
        .map(|(index, _)| centers[index] - widths[index] / 2)
        .min()
        .unwrap()
        - 2;
    let base_right = sequence
        .participants
        .iter()
        .enumerate()
        .map(|(index, _)| centers[index] - widths[index] / 2 + widths[index])
        .max()
        .unwrap()
        + 2;
    let max_control_title_width = sequence
        .controls
        .iter()
        .filter_map(|control| match &control.kind {
            ControlKind::Start(kind, label) => Some(kind.keyword().width() + 1 + label.width()),
            ControlKind::Else(label) => Some("else".width() + 1 + label.width()),
            ControlKind::End => None,
        })
        .max()
        .unwrap_or(0) as i32;
    let frame_left = base_left - max_control_title_width - 2;
    let mut groups = Vec::new();
    let mut open_fragments: Vec<OpenFragment> = Vec::new();
    for (control, &row) in sequence.controls.iter().zip(&control_rows) {
        match &control.kind {
            ControlKind::Start(kind, label) => {
                let branch_depth = open_fragments
                    .iter()
                    .filter(|fragment| fragment.else_branch.is_some())
                    .count();
                let depth = open_fragments.len() + branch_depth;
                open_fragments.push(OpenFragment {
                    kind: *kind,
                    title: format!("{} {label}", kind.keyword()),
                    segment_start: row,
                    depth,
                    else_branch: None,
                });
            }
            ControlKind::Else(label) => {
                let frame = open_fragments
                    .last_mut()
                    .expect("parser guarantees else is inside alt");
                debug_assert_eq!(frame.kind, FragmentKind::Alt);
                frame.else_branch = Some((format!("else {label}"), row));
            }
            ControlKind::End => {
                let frame = open_fragments
                    .pop()
                    .expect("parser guarantees balanced control blocks");
                push_fragment_group(
                    &mut groups,
                    frame_left,
                    base_right,
                    frame.depth,
                    frame.segment_start,
                    row,
                    &frame.title,
                );
                if let Some((title, start)) = frame.else_branch {
                    push_fragment_group(
                        &mut groups,
                        frame_left,
                        base_right,
                        frame.depth + 1,
                        start,
                        row - 1,
                        &title,
                    );
                }
            }
        }
    }

    Scene {
        boxes,
        foreground_boxes,
        groups,
        paths,
        edges,
        endpoint_decorations: Vec::new(),
        texts: Vec::new(),
    }
}

fn push_fragment_group(
    groups: &mut Vec<SceneGroup>,
    base_left: i32,
    base_right: i32,
    depth: usize,
    top: i32,
    bottom: i32,
    title: &str,
) {
    let x = base_left + depth as i32 * 2;
    // Use a shallow right inset: enough to keep nested corners legible, while
    // retaining the header-sized margin around the destination lifeline.
    let natural_right = (base_right - depth as i32).max(x + 2);
    let right = natural_right.max(x + title.width() as i32 + 4);
    groups.push(SceneGroup {
        subgraph: groups.len(),
        rect: Rect::new(x, top, right - x, bottom - top + 1),
        title: SceneText::new(Point::new(x + 2, top + 1), title),
    });
}

fn active_attachment(center: i32, depth: usize, direction: i32) -> i32 {
    if depth == 0 {
        return center;
    }
    let left = center - 1 + (depth as i32 - 1) * 2;
    if direction < 0 { left } else { left + 2 }
}
