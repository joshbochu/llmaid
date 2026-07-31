//! Sequence-specific integer layout lowered into the shared `Scene`.

use unicode_width::UnicodeWidthStr;

use super::ir::*;
use crate::scene::{
    Arrow, ArrowHead, EdgeKind, Point, Rect, RoutedEdge, Scene, SceneBox, SceneGroup,
    SceneGroupSeparator, ScenePath, SceneText, Shape,
};
use crate::wrapping::{self, MIN_READABLE_COLUMNS};

#[derive(Clone, Copy)]
struct Fit {
    compact: bool,
    label_columns: Option<usize>,
}

const NORMAL: Fit = Fit {
    compact: false,
    label_columns: None,
};
const COMPACT: Fit = Fit {
    compact: true,
    label_columns: None,
};

struct FragmentFrame<'a> {
    base_left: i32,
    base_right: i32,
    depth: usize,
    top: i32,
    bottom: i32,
    title: &'a str,
    separator: Option<(String, i32)>,
}

/// Lay out sequence content with the shared B9 degradation ladder.
pub fn scene(sequence: &SequenceDiagram, max_width: usize) -> Scene {
    if sequence.participants.is_empty() {
        return Scene::default();
    }

    let normal = lower(sequence, NORMAL);
    if scene_width(&normal) <= max_width {
        return normal;
    }
    let compact = lower(sequence, COMPACT);
    let compact_width = scene_width(&compact);
    if compact_width <= max_width {
        return compact;
    }

    let widest_label = sequence
        .participants
        .iter()
        .map(|participant| wrapping::max_line_width(&participant.label))
        .chain(sequence.events.iter().filter_map(|event| match event {
            SequenceEvent::Message(message) => Some(wrapping::max_line_width(&message.label)),
            SequenceEvent::Note(note) => Some(wrapping::max_line_width(&note.text)),
            SequenceEvent::Activation(_) => None,
        }))
        .max()
        .unwrap_or(0);
    if widest_label <= MIN_READABLE_COLUMNS {
        return compact;
    }

    let narrowest = lower(
        sequence,
        Fit {
            compact: true,
            label_columns: Some(MIN_READABLE_COLUMNS),
        },
    );
    let narrowest_width = scene_width(&narrowest);
    if narrowest_width > max_width {
        return if narrowest_width < compact_width {
            narrowest
        } else {
            compact
        };
    }

    let mut best = narrowest;
    let mut low = MIN_READABLE_COLUMNS + 1;
    let mut high = widest_label.saturating_sub(1);
    while low <= high {
        let columns = low + (high - low) / 2;
        let candidate = lower(
            sequence,
            Fit {
                compact: true,
                label_columns: Some(columns),
            },
        );
        if scene_width(&candidate) <= max_width {
            best = candidate;
            low = columns + 1;
        } else {
            high = columns.saturating_sub(1);
        }
    }
    best
}

fn scene_width(scene: &Scene) -> usize {
    scene.bounds().w.max(0) as usize
}

fn lower(sequence: &SequenceDiagram, fit: Fit) -> Scene {
    let participant_lines: Vec<Vec<String>> = sequence
        .participants
        .iter()
        .map(|participant| label_lines(&participant.label, fit))
        .collect();
    let event_lines: Vec<Option<Vec<String>>> = sequence
        .events
        .iter()
        .map(|event| match event {
            SequenceEvent::Message(message) => Some(label_lines(&message.label, fit)),
            SequenceEvent::Note(note) => Some(label_lines(&note.text, fit)),
            SequenceEvent::Activation(_) => None,
        })
        .collect();
    let widths: Vec<i32> = sequence
        .participants
        .iter()
        .enumerate()
        .map(|(participant, _)| line_width(&participant_lines[participant]) + 4)
        .collect();
    let header_height = participant_lines
        .iter()
        .map(|lines| lines.len() as i32 + 2)
        .max()
        .unwrap_or(3);
    let column_gap = if fit.compact { 3 } else { 6 };
    let mut gaps: Vec<i32> = widths
        .windows(2)
        .map(|pair| (pair[0] + 1) / 2 + pair[1] / 2 + column_gap)
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

    for (event_index, event) in sequence.events.iter().enumerate() {
        match event {
            SequenceEvent::Message(message) => {
                let low = message.from.min(message.to);
                let high = message.from.max(message.to);
                if low == high {
                    continue;
                }
                let current: i32 = gaps[low..high].iter().sum();
                let required = line_width(event_lines[event_index].as_ref().unwrap())
                    + if fit.compact { 4 } else { 6 };
                if required > current {
                    gaps[high - 1] += required - current;
                }
            }
            SequenceEvent::Note(note) => {
                let note_width = line_width(event_lines[event_index].as_ref().unwrap()) + 4;
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
                    gaps[gap] = gaps[gap].max(
                        note_width
                            + if fit.compact { 3 } else { 4 }
                            + max_activation_depths[participant] as i32 * 2,
                    );
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
    let mut next_top = header_height + 1;
    let mut last_content_bottom = header_height - 1;
    let mut lifeline_bottom = header_height + 2;
    let mut active_rows: Vec<Vec<i32>> = vec![Vec::new(); sequence.participants.len()];
    let mut event_rows = Vec::with_capacity(sequence.events.len());
    let mut control_rows = Vec::with_capacity(sequence.controls.len());
    let mut control_index = 0;
    let mut fragment_rows = Vec::new();
    let events_with_end = sequence
        .events
        .iter()
        .map(Some)
        .chain(std::iter::once(None));
    for (event_index, event) in events_with_end.enumerate() {
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
        let Some(event) = event else {
            break;
        };
        let row = match event {
            SequenceEvent::Message(message) => {
                let label_height = event_lines[event_index].as_ref().unwrap().len() as i32;
                let row = next_top + label_height;
                let self_loop = message.from == message.to;
                let bottom = row + if self_loop { 2 } else { 0 };
                last_content_bottom = bottom;
                next_top = row + if self_loop { 4 } else { 2 };
                lifeline_bottom = lifeline_bottom.max(bottom + 1);
                row
            }
            SequenceEvent::Note(_) => {
                let row = next_top;
                let height = event_lines[event_index].as_ref().unwrap().len() as i32 + 2;
                last_content_bottom = row + height - 1;
                next_top = row + height;
                lifeline_bottom = lifeline_bottom.max(next_top);
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
        .map(|(index, _)| SceneBox {
            node: index,
            rect: Rect::new(
                centers[index] - widths[index] / 2,
                0,
                widths[index],
                header_height,
            ),
            lines: participant_lines[index].clone(),
            shape: Shape::Rect,
            table: None,
        })
        .collect();
    let mut next_box_id = sequence.participants.len();
    let mut note_boxes = Vec::new();
    let mut active_note_depths = vec![0usize; sequence.participants.len()];
    for (event_index, (event, &y)) in sequence.events.iter().zip(&event_rows).enumerate() {
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
        let lines = event_lines[event_index].as_ref().unwrap();
        let text_width = line_width(lines);
        let height = lines.len() as i32 + 2;
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
            rect: Rect::new(x, y, width, height),
            lines: lines.clone(),
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
            points: vec![
                Point::new(center, header_height - 1),
                Point::new(center, lifeline_bottom),
            ],
            rounded: Vec::new(),
            kind: EdgeKind::Dotted,
        })
        .collect();

    let mut edges = Vec::new();
    let mut active_depths = vec![0; sequence.participants.len()];
    for (event_index, (event, &y)) in sequence.events.iter().zip(&event_rows).enumerate() {
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
            let label_width = line_width(event_lines[event_index].as_ref().unwrap());
            let loop_x = source_x + label_width + 5;
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
        let lines = event_lines[event_index].as_ref().unwrap();
        let label_width = line_width(lines);
        let label_x = if self_message {
            source_x + 2
        } else {
            (source_x + target_x - label_width) / 2
        };
        let edge = RoutedEdge {
            edge: edges.len(),
            points,
            rounded,
            kind: message.kind.scene_kind(),
            label: Some(SceneText::new(
                Point::new(label_x, y - lines.len() as i32),
                lines.join("\n"),
            )),
            arrow: Some(arrow),
        };
        edges.push(edge);
    }

    struct OpenFragment {
        kind: FragmentKind,
        title: String,
        segment_start: i32,
        depth: usize,
        separator: Option<(String, i32)>,
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
                let depth = open_fragments.len();
                open_fragments.push(OpenFragment {
                    kind: *kind,
                    title: format!("{} {label}", kind.keyword()),
                    segment_start: row,
                    depth,
                    separator: None,
                });
            }
            ControlKind::Else(label) => {
                let frame = open_fragments
                    .last_mut()
                    .expect("parser guarantees else is inside alt");
                debug_assert_eq!(frame.kind, FragmentKind::Alt);
                frame.separator = Some((format!("else {label}"), row));
            }
            ControlKind::End => {
                let frame = open_fragments
                    .pop()
                    .expect("parser guarantees balanced control blocks");
                push_fragment_group(
                    &mut groups,
                    FragmentFrame {
                        base_left: frame_left,
                        base_right,
                        depth: frame.depth,
                        top: frame.segment_start,
                        bottom: row,
                        title: &frame.title,
                        separator: frame.separator,
                    },
                );
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

fn label_lines(label: &str, fit: Fit) -> Vec<String> {
    match fit.label_columns {
        Some(columns) => wrapping::wrap_words(label, columns),
        None => label.split('\n').map(str::to_string).collect(),
    }
}

fn line_width(lines: &[String]) -> i32 {
    lines
        .iter()
        .map(|line| line.width() as i32)
        .max()
        .unwrap_or(0)
}

#[allow(clippy::too_many_arguments)]
fn push_fragment_group(groups: &mut Vec<SceneGroup>, frame: FragmentFrame<'_>) {
    let x = frame.base_left + frame.depth as i32 * 2;
    // Use a shallow right inset: enough to keep nested corners legible, while
    // retaining the header-sized margin around the destination lifeline.
    let natural_right = (frame.base_right - frame.depth as i32).max(x + 2);
    let right = natural_right.max(x + frame.title.width() as i32 + 4);
    groups.push(SceneGroup {
        subgraph: groups.len(),
        rect: Rect::new(x, frame.top, right - x, frame.bottom - frame.top + 1),
        title: SceneText::new(Point::new(x + 2, frame.top + 1), frame.title),
        separators: frame
            .separator
            .into_iter()
            .map(|(label, y)| SceneGroupSeparator {
                y,
                label: SceneText::new(Point::new(x + 2, y), format!(" {label} ")),
            })
            .collect(),
    });
}

fn active_attachment(center: i32, depth: usize, direction: i32) -> i32 {
    if depth == 0 {
        return center;
    }
    let left = center - 1 + (depth as i32 - 1) * 2;
    if direction < 0 { left } else { left + 2 }
}
