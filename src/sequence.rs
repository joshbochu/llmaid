//! Mermaid sequence-diagram core subset -> semantic IR -> shared `Scene`.

use std::collections::HashMap;

use unicode_width::UnicodeWidthStr;

use crate::parse::{ParseError, Warning};
use crate::scene::{
    Arrow, ArrowHead, EdgeKind, Point, Rect, RoutedEdge, Scene, SceneBox, ScenePath, SceneText,
    Shape,
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

/// Declaration order is authoritative for participant columns and messages.
#[derive(Debug, Default)]
pub struct SequenceDiagram {
    pub participants: Vec<Participant>,
    pub messages: Vec<Message>,
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
    Ok(sequence)
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
    sequence.messages.push(Message {
        from,
        to,
        label: label.to_string(),
        kind,
    });
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
    output.push_str("messages:\n");
    for message in &sequence.messages {
        output.push_str(&format!(
            "  {} {} {} \"{}\"\n",
            sequence.participants[message.from].id,
            message.kind.operator(),
            sequence.participants[message.to].id,
            message.label.replace('"', "\\\"")
        ));
    }
    if !sequence.warnings.is_empty() {
        output.push_str("warnings:\n");
        for warning in &sequence.warnings {
            output.push_str(&format!("  line {}: {}\n", warning.line, warning.msg));
        }
    }
    output
}

/// Lay out the core sequence subset directly into the shared terminal scene.
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

    for message in &sequence.messages {
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

    let mut centers = vec![widths[0] / 2];
    for gap in gaps {
        centers.push(centers.last().copied().unwrap() + gap);
    }

    let mut next_y = 5;
    let message_rows: Vec<i32> = sequence
        .messages
        .iter()
        .map(|message| {
            let row = next_y;
            next_y += if message.from == message.to { 5 } else { 3 };
            row
        })
        .collect();
    let last_message_y = sequence
        .messages
        .last()
        .zip(message_rows.last())
        .map(|(message, &row)| row + if message.from == message.to { 2 } else { 0 })
        .unwrap_or(4);
    let lifeline_bottom = last_message_y + 1;

    let boxes = sequence
        .participants
        .iter()
        .enumerate()
        .map(|(index, participant)| SceneBox {
            node: index,
            rect: Rect::new(centers[index] - widths[index] / 2, 0, widths[index], 3),
            lines: vec![participant.label.clone()],
            shape: Shape::Rect,
        })
        .collect();
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

    let edges = sequence
        .messages
        .iter()
        .enumerate()
        .map(|(index, message)| {
            let source_x = centers[message.from];
            let target_x = centers[message.to];
            let y = message_rows[index];
            let (points, rounded, arrow) = if source_x == target_x {
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
            let label_x = if source_x == target_x {
                source_x + 2
            } else {
                (source_x + target_x - message.label.width() as i32) / 2
            };
            RoutedEdge {
                edge: index,
                points,
                rounded,
                kind: message.kind.scene_kind(),
                label: Some(SceneText::new(Point::new(label_x, y - 1), &message.label)),
                arrow: Some(arrow),
            }
        })
        .collect();

    Scene {
        boxes,
        groups: Vec::new(),
        paths,
        edges,
    }
}
