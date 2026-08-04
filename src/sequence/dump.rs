//! Deterministic textual dump of the parsed sequence IR.

use super::ir::*;

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
            SequenceEvent::Message(message) => {
                let number = message
                    .number
                    .map(|number| format!(" number {number}"))
                    .unwrap_or_default();
                output.push_str(&format!(
                    "  message {} {} {}{} \"{}\"\n",
                    sequence.participants[message.from].id,
                    message.operator(),
                    sequence.participants[message.to].id,
                    number,
                    message.label.replace('"', "\\\"")
                ));
            }
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
