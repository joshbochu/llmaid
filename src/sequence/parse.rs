//! Mermaid sequence syntax parsing and validation.

use super::ir::*;
use crate::parse::{ParseError, Warning};

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
