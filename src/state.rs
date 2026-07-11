//! Flat Mermaid state-diagram subset -> semantic IR.

use std::collections::HashMap;

use crate::boxed::{BoxDiagram, BoxNode, NodeId};
use crate::parse::{Dir, ParseError, Warning};
use crate::scene::{Scene, Shape};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endpoint {
    State(usize),
    Marker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    pub from: Endpoint,
    pub to: Endpoint,
    pub label: Option<String>,
    pub line: usize,
}

/// Declaration/first-use order is authoritative for deterministic layout.
#[derive(Debug, Default)]
pub struct StateDiagram {
    pub dir: Option<Dir>,
    pub states: Vec<State>,
    pub transitions: Vec<Transition>,
    pub warnings: Vec<Warning>,
    index: HashMap<String, usize>,
    explicitly_declared: Vec<bool>,
}

impl StateDiagram {
    pub fn direction(&self) -> Dir {
        self.dir.unwrap_or(Dir::TB)
    }

    fn state(&mut self, id: &str) -> usize {
        if let Some(&index) = self.index.get(id) {
            return index;
        }
        let index = self.states.len();
        self.states.push(State {
            id: id.to_string(),
            label: id.to_string(),
        });
        self.explicitly_declared.push(false);
        self.index.insert(id.to_string(), index);
        index
    }

    fn declare(&mut self, id: &str, label: String, line: usize) {
        let index = self.state(id);
        if self.explicitly_declared[index] {
            self.warnings.push(Warning {
                line,
                msg: format!(
                    "state `{id}` redeclared; last definition wins (was \"{}\")",
                    self.states[index].label.replace('"', "\\\"")
                ),
            });
        }
        self.states[index].label = label;
        self.explicitly_declared[index] = true;
    }
}

pub fn parse(src: &str) -> Result<StateDiagram, ParseError> {
    let mut diagram = StateDiagram::default();
    let mut seen_header = false;
    let mut seen_direction = false;

    for (line_index, raw_line) in src.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }

        if !seen_header {
            if !matches!(line, "stateDiagram" | "stateDiagram-v2") {
                return Err(error(
                    line_number,
                    "expected `stateDiagram` or `stateDiagram-v2` header",
                ));
            }
            seen_header = true;
            continue;
        }

        if matches!(line, "stateDiagram" | "stateDiagram-v2") {
            diagram.warnings.push(Warning {
                line: line_number,
                msg: format!("duplicate `{line}` header ignored"),
            });
            continue;
        }

        if line == "direction" || line.starts_with("direction ") {
            let token = line["direction".len()..].trim();
            let direction = parse_direction(token).ok_or_else(|| {
                error(
                    line_number,
                    if token.is_empty() {
                        "expected LR, RL, TB or BT after `direction`".to_string()
                    } else {
                        format!("unknown direction `{token}`; expected LR, RL, TB or BT")
                    },
                )
            })?;
            if seen_direction {
                diagram.warnings.push(Warning {
                    line: line_number,
                    msg: "direction redeclared; last value wins".to_string(),
                });
            }
            diagram.dir = Some(direction);
            seen_direction = true;
            continue;
        }

        if line.starts_with("state ") || line == "state" {
            parse_declaration(&mut diagram, line, line_number)?;
        } else if line.contains("-->") {
            parse_transition(&mut diagram, line, line_number)?;
        } else if line.contains('{') || line == "}" {
            return Err(error(
                line_number,
                "nested/composite states are not supported; expected a flat state declaration or transition",
            ));
        } else if valid_id(line) {
            diagram.declare(line, line.to_string(), line_number);
        } else {
            return Err(error(
                line_number,
                "expected a state identifier or `A --> B` transition",
            ));
        }
    }

    if !seen_header {
        return Err(error(
            1,
            "expected `stateDiagram` or `stateDiagram-v2` header",
        ));
    }
    Ok(diagram)
}

fn parse_declaration(
    diagram: &mut StateDiagram,
    line: &str,
    line_number: usize,
) -> Result<(), ParseError> {
    let body = line["state".len()..].trim();
    if body.is_empty() {
        return Err(error(
            line_number,
            "expected a state identifier after `state`",
        ));
    }
    if body.contains('{') || body.ends_with('}') {
        return Err(error(
            line_number,
            "nested/composite states are not supported; expected a flat state declaration",
        ));
    }

    let (id, label) = if let Some(quoted) = body.strip_prefix('"') {
        let Some(quote) = quoted.find('"') else {
            return Err(error(
                line_number,
                "expected `state \"Label\" as ID` with a closing quote",
            ));
        };
        let label = &quoted[..quote];
        let suffix = quoted[quote + 1..].trim();
        let Some(id) = suffix.strip_prefix("as ").map(str::trim) else {
            return Err(error(line_number, "expected `state \"Label\" as ID`"));
        };
        (id, label.to_string())
    } else {
        if body.split_whitespace().count() != 1 {
            return Err(error(
                line_number,
                "expected `state ID` or `state \"Label\" as ID`",
            ));
        }
        (body, body.to_string())
    };
    require_id(id, line_number, "state identifier")?;
    diagram.declare(id, label, line_number);
    Ok(())
}

fn parse_transition(
    diagram: &mut StateDiagram,
    line: &str,
    line_number: usize,
) -> Result<(), ParseError> {
    let (head, label) = match line.split_once(':') {
        Some((head, label)) => {
            let label = label.trim();
            if label.is_empty() {
                return Err(error(line_number, "expected a transition label after `:`"));
            }
            (head.trim(), Some(label.to_string()))
        }
        None => (line, None),
    };
    let mut pieces = head.split("-->");
    let from = pieces.next().unwrap_or("").trim();
    let right = pieces.next().unwrap_or("").trim();
    if pieces.next().is_some() {
        return Err(error(
            line_number,
            "expected exactly one `-->` in a state transition",
        ));
    }
    if from.is_empty() {
        return Err(error(
            line_number,
            "expected a transition source before `-->`",
        ));
    }
    if right.is_empty() {
        return Err(error(
            line_number,
            "expected a transition target after `-->`",
        ));
    }

    let to = right;
    if to.is_empty() {
        return Err(error(
            line_number,
            "expected a transition target after `-->`",
        ));
    }

    let from = parse_endpoint(diagram, from, line_number, "transition source")?;
    let to = parse_endpoint(diagram, to, line_number, "transition target")?;
    diagram.transitions.push(Transition {
        from,
        to,
        label,
        line: line_number,
    });
    Ok(())
}

fn parse_endpoint(
    diagram: &mut StateDiagram,
    token: &str,
    line: usize,
    expected: &str,
) -> Result<Endpoint, ParseError> {
    if token == "[*]" {
        return Ok(Endpoint::Marker);
    }
    require_id(token, line, expected)?;
    Ok(Endpoint::State(diagram.state(token)))
}

fn require_id(id: &str, line: usize, expected: &str) -> Result<(), ParseError> {
    if valid_id(id) {
        Ok(())
    } else {
        Err(error(
            line,
            format!("expected a valid {expected}; found `{id}`"),
        ))
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|character| character.is_alphanumeric() || matches!(character, '_' | '-'))
}

fn parse_direction(token: &str) -> Option<Dir> {
    match token {
        "LR" => Some(Dir::LR),
        "RL" => Some(Dir::RL),
        "TB" | "TD" => Some(Dir::TB),
        "BT" => Some(Dir::BT),
        _ => None,
    }
}

fn error(line: usize, msg: impl Into<String>) -> ParseError {
    ParseError {
        line,
        msg: msg.into(),
    }
}

pub fn is_empty(diagram: &StateDiagram) -> bool {
    diagram.states.is_empty() && diagram.transitions.is_empty()
}

pub fn labels(diagram: &StateDiagram) -> Vec<&str> {
    diagram
        .states
        .iter()
        .map(|state| state.label.as_str())
        .chain(
            diagram
                .transitions
                .iter()
                .filter_map(|transition| transition.label.as_deref()),
        )
        .collect()
}

pub fn dump(diagram: &StateDiagram) -> String {
    let mut output = format!(
        "type: state\ndirection: {}\nstates:\n",
        diagram.direction().name()
    );
    for state in &diagram.states {
        output.push_str(&format!(
            "  {} \"{}\"\n",
            state.id,
            state.label.replace('"', "\\\"")
        ));
    }
    output.push_str("transitions:\n");
    for transition in &diagram.transitions {
        let endpoint = |endpoint| match endpoint {
            Endpoint::Marker => "[*]",
            Endpoint::State(index) => diagram.states[index].id.as_str(),
        };
        output.push_str(&format!(
            "  {} --> {}",
            endpoint(transition.from),
            endpoint(transition.to)
        ));
        if let Some(label) = &transition.label {
            output.push_str(&format!(" \"{}\"", label.replace('"', "\\\"")));
        }
        output.push('\n');
    }
    if !diagram.warnings.is_empty() {
        output.push_str("warnings:\n");
        for warning in &diagram.warnings {
            output.push_str(&format!("  line {}: {}\n", warning.line, warning.msg));
        }
    }
    output
}

/// Lower the flat state IR through the shared deterministic boxed adapter.
/// Each `[*]` occurrence becomes its own pseudo-node so multiple initial or
/// final transitions retain their source order and never collapse together.
pub fn scene(diagram: &StateDiagram, width: usize) -> Scene {
    let mut boxed = BoxDiagram::new(diagram.direction());
    let states: Vec<NodeId> = diagram
        .states
        .iter()
        .map(|state| {
            boxed.add_node(BoxNode::new(
                state.id.clone(),
                state.label.clone(),
                Shape::Rounded,
            ))
        })
        .collect();

    let mut endpoints = Vec::with_capacity(diagram.transitions.len());
    for (index, transition) in diagram.transitions.iter().enumerate() {
        let from = match transition.from {
            Endpoint::State(state) => states[state],
            Endpoint::Marker => {
                boxed.add_node(BoxNode::new(format!("__start_{index}"), "*", Shape::Circle))
            }
        };
        let to = match transition.to {
            Endpoint::State(state) => states[state],
            Endpoint::Marker => {
                boxed.add_node(BoxNode::new(format!("__end_{index}"), "O", Shape::Circle))
            }
        };
        endpoints.push((from, to));
    }
    for (transition, (from, to)) in diagram.transitions.iter().zip(endpoints) {
        let mut edge = boxed.add_edge(from, to);
        if let Some(label) = &transition.label {
            edge.label(label.clone());
        }
    }
    boxed.scene(width)
}
