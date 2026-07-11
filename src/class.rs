//! Mermaid `classDiagram` subset -> deterministic, type-specific semantic IR.

use std::collections::HashMap;

use crate::boxed::{BoxDiagram, BoxNode, EdgeEnd, NodeId, annotate_endpoint, decorate_endpoint};
use crate::parse::{Dir, ParseError, Warning};
use crate::scene::{EdgeKind, EndpointDecorationKind, Scene, SceneTable, Shape};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub id: String,
    /// Members are retained verbatim apart from surrounding whitespace. This
    /// deliberately preserves Mermaid visibility markers such as `+` and `-`.
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationKind {
    Inheritance,
    Composition,
    Aggregation,
    Association,
    Link,
    Dependency,
    Realization,
}

impl RelationKind {
    pub fn operator(self) -> &'static str {
        match self {
            Self::Inheritance => "<|--",
            Self::Composition => "*--",
            Self::Aggregation => "o--",
            Self::Association => "-->",
            Self::Link => "--",
            Self::Dependency => "..>",
            Self::Realization => "..|>",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub from: usize,
    pub to: usize,
    pub kind: RelationKind,
    pub from_multiplicity: Option<String>,
    pub to_multiplicity: Option<String>,
    pub label: Option<String>,
    pub line: usize,
}

/// Declaration order, including first use in a relation, is authoritative.
#[derive(Debug, Default)]
pub struct ClassDiagram {
    pub direction: Option<Dir>,
    pub classes: Vec<Class>,
    pub relations: Vec<Relation>,
    pub warnings: Vec<Warning>,
    index: HashMap<String, usize>,
}

impl ClassDiagram {
    fn class(&mut self, id: &str) -> usize {
        if let Some(&index) = self.index.get(id) {
            return index;
        }
        let index = self.classes.len();
        self.classes.push(Class {
            id: id.to_string(),
            members: Vec::new(),
        });
        self.index.insert(id.to_string(), index);
        index
    }

    pub fn is_empty(&self) -> bool {
        self.classes.is_empty()
    }

    pub fn labels(&self) -> Vec<&str> {
        let mut labels = Vec::new();
        for class in &self.classes {
            labels.push(class.id.as_str());
            labels.extend(class.members.iter().map(String::as_str));
        }
        labels.extend(
            self.relations
                .iter()
                .filter_map(|relation| relation.label.as_deref()),
        );
        labels
    }
}

pub fn parse(src: &str) -> Result<ClassDiagram, ParseError> {
    let mut diagram = ClassDiagram::default();
    let mut seen_header = false;
    let mut open_class: Option<(usize, usize)> = None;

    for (line_index, raw) in src.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }

        if !seen_header {
            if line != "classDiagram" {
                return Err(error(line_number, "expected `classDiagram` header"));
            }
            seen_header = true;
            continue;
        }

        if let Some((class_index, _)) = open_class {
            if line == "}" {
                open_class = None;
            } else if line.contains('{') || line.contains('}') {
                return Err(error(line_number, "expected a class member or closing `}`"));
            } else {
                diagram.classes[class_index].members.push(line.to_string());
            }
            continue;
        }

        if line == "classDiagram" {
            diagram.warnings.push(Warning {
                line: line_number,
                msg: "duplicate `classDiagram` header ignored".to_string(),
            });
        } else if line == "direction" || line.starts_with("direction ") {
            parse_direction(&mut diagram, &line["direction".len()..], line_number)?;
        } else if line.starts_with("class ") {
            parse_class(&mut diagram, line, line_number, &mut open_class)?;
        } else if is_ignored_directive(line) {
            diagram.warnings.push(Warning {
                line: line_number,
                msg: format!("unsupported class styling directive ignored: `{line}`"),
            });
        } else {
            parse_relation(&mut diagram, line, line_number)?;
        }
    }

    if !seen_header {
        return Err(error(1, "expected `classDiagram` header"));
    }
    if let Some((class_index, line)) = open_class {
        return Err(error(
            line,
            format!(
                "expected a closing `}}` for class `{}`",
                diagram.classes[class_index].id
            ),
        ));
    }
    Ok(diagram)
}

fn parse_direction(diagram: &mut ClassDiagram, rest: &str, line: usize) -> Result<(), ParseError> {
    let token = rest.trim();
    let direction = match token {
        "LR" => Dir::LR,
        "RL" => Dir::RL,
        "TB" | "TD" => Dir::TB,
        "BT" => Dir::BT,
        _ => {
            return Err(error(
                line,
                "expected direction `LR`, `RL`, `TB`, `TD`, or `BT`",
            ));
        }
    };
    if diagram.direction.replace(direction).is_some() {
        diagram.warnings.push(Warning {
            line,
            msg: "duplicate direction; last value wins".to_string(),
        });
    }
    Ok(())
}

fn parse_class(
    diagram: &mut ClassDiagram,
    line: &str,
    line_number: usize,
    open_class: &mut Option<(usize, usize)>,
) -> Result<(), ParseError> {
    let rest = line["class".len()..].trim();
    let (id, opens) = match rest.strip_suffix('{') {
        Some(id) => (id.trim(), true),
        None => (rest, false),
    };
    if !valid_id(id) {
        return Err(error(
            line_number,
            "expected a class id, optionally followed by `{`",
        ));
    }
    let class = diagram.class(id);
    if opens {
        *open_class = Some((class, line_number));
    }
    Ok(())
}

fn parse_relation(
    diagram: &mut ClassDiagram,
    line: &str,
    line_number: usize,
) -> Result<(), ParseError> {
    let (head, label) = match line.split_once(':') {
        Some((head, label)) => {
            let label = label.trim();
            if label.is_empty() {
                return Err(error(line_number, "expected a relation label after `:`"));
            }
            (head.trim(), Some(label.to_string()))
        }
        None => (line, None),
    };
    // Longest/specialized operators first so `--` cannot consume `-->`.
    let operators = [
        ("<|--", RelationKind::Inheritance),
        ("..|>", RelationKind::Realization),
        ("*--", RelationKind::Composition),
        ("o--", RelationKind::Aggregation),
        ("-->", RelationKind::Association),
        ("..>", RelationKind::Dependency),
        ("--", RelationKind::Link),
    ];
    let Some((at, operator, kind)) = operators
        .iter()
        .find_map(|&(operator, kind)| head.find(operator).map(|at| (at, operator, kind)))
    else {
        return Err(error(
            line_number,
            "expected a class declaration or relation operator `<|--`, `*--`, `o--`, `-->`, `--`, `..>`, or `..|>`",
        ));
    };

    let left = head[..at].trim();
    let right = head[at + operator.len()..].trim();

    let (from_id, from_multiplicity) = parse_endpoint(left, true, line_number)?;
    let (to_id, to_multiplicity) = parse_endpoint(right, false, line_number)?;
    let from = diagram.class(from_id);
    let to = diagram.class(to_id);
    diagram.relations.push(Relation {
        from,
        to,
        kind,
        from_multiplicity,
        to_multiplicity,
        label,
        line: line_number,
    });
    Ok(())
}

fn parse_endpoint(
    text: &str,
    multiplicity_after_id: bool,
    line: usize,
) -> Result<(&str, Option<String>), ParseError> {
    let text = text.trim();
    let (id, multiplicity) = if multiplicity_after_id {
        if let Some(quote) = text.find('"') {
            let id = text[..quote].trim();
            let multiplicity = parse_quoted(&text[quote..], line)?;
            (id, Some(multiplicity))
        } else {
            (text, None)
        }
    } else if let Some(value) = text.strip_prefix('"') {
        let Some(end) = value.find('"') else {
            return Err(error(line, "expected a closing `\"` for multiplicity"));
        };
        let multiplicity = value[..end].to_string();
        let id = value[end + 1..].trim();
        (id, Some(multiplicity))
    } else {
        (text, None)
    };
    if !valid_id(id) {
        return Err(error(
            line,
            "expected class ids on both sides of the relation",
        ));
    }
    Ok((id, multiplicity))
}

fn parse_quoted(text: &str, line: usize) -> Result<String, ParseError> {
    let Some(value) = text.strip_prefix('"') else {
        unreachable!("caller starts at a quote")
    };
    let Some(end) = value.find('"') else {
        return Err(error(line, "expected a closing `\"` for multiplicity"));
    };
    if !value[end + 1..].trim().is_empty() {
        return Err(error(
            line,
            "expected multiplicity immediately beside the relation operator",
        ));
    }
    Ok(value[..end].to_string())
}

fn is_ignored_directive(line: &str) -> bool {
    matches!(
        line.split_whitespace().next().unwrap_or(""),
        "classDef" | "style" | "click" | "cssClass"
    )
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn error(line: usize, msg: impl Into<String>) -> ParseError {
    ParseError {
        line,
        msg: msg.into(),
    }
}

pub fn dump(diagram: &ClassDiagram) -> String {
    let direction = diagram.direction.unwrap_or(Dir::TB);
    let mut out = format!("type: class\ndirection: {}\nclasses:\n", direction.name());
    for (index, class) in diagram.classes.iter().enumerate() {
        out.push_str(&format!("  {index}: {}\n", class.id));
        for member in &class.members {
            out.push_str(&format!("    member: {member}\n"));
        }
    }
    out.push_str("relations:\n");
    for relation in &diagram.relations {
        let from = &diagram.classes[relation.from].id;
        let to = &diagram.classes[relation.to].id;
        out.push_str(&format!("  {from}"));
        if let Some(multiplicity) = &relation.from_multiplicity {
            out.push_str(&format!(" \"{multiplicity}\""));
        }
        out.push_str(&format!(" {} ", relation.kind.operator()));
        if let Some(multiplicity) = &relation.to_multiplicity {
            out.push_str(&format!("\"{multiplicity}\" "));
        }
        out.push_str(to);
        if let Some(label) = &relation.label {
            out.push_str(&format!(" : {label}"));
        }
        out.push('\n');
    }
    out
}

/// Lower class semantics into structured compartments and endpoint-aware UML
/// relationships while retaining the shared integer boxed geometry.
pub fn scene(diagram: &ClassDiagram, width: usize) -> Scene {
    let mut boxed = BoxDiagram::new(diagram.direction.unwrap_or(Dir::TB));
    let tables: Vec<SceneTable> = diagram
        .classes
        .iter()
        .map(|class| {
            SceneTable::new(
                class.id.clone(),
                class
                    .members
                    .iter()
                    .map(|member| vec![member.clone()])
                    .collect(),
            )
        })
        .collect();
    let nodes: Vec<NodeId> = diagram
        .classes
        .iter()
        .zip(&tables)
        .map(|(class, table)| {
            boxed.add_node(BoxNode::new(
                &class.id,
                table.layout_label(),
                Shape::Rounded,
            ))
        })
        .collect();

    for relation in &diagram.relations {
        let mut edge = boxed.add_edge(nodes[relation.from], nodes[relation.to]);
        edge.without_arrow();
        if let Some(label) = &relation.label {
            edge.label(label.clone());
        }
        match relation.kind {
            RelationKind::Dependency | RelationKind::Realization => {
                edge.kind(EdgeKind::Dotted);
            }
            _ => {}
        }
    }

    // Structured compartments cannot be meaningfully word-wrapped as plain
    // labels. Preserve their columns and allow B9's final over-width fallback.
    let mut scene = boxed.scene(width.max(10_000));
    for (box_, table) in scene.boxes.iter_mut().zip(tables) {
        box_.lines.clear();
        box_.table = Some(table);
    }
    for (edge_index, relation) in diagram.relations.iter().enumerate() {
        let decoration = match relation.kind {
            RelationKind::Inheritance => {
                Some((EdgeEnd::Source, EndpointDecorationKind::OpenTriangle))
            }
            RelationKind::Composition => {
                Some((EdgeEnd::Source, EndpointDecorationKind::FilledDiamond))
            }
            RelationKind::Aggregation => {
                Some((EdgeEnd::Source, EndpointDecorationKind::OpenDiamond))
            }
            RelationKind::Association | RelationKind::Dependency => {
                Some((EdgeEnd::Target, EndpointDecorationKind::OpenArrow))
            }
            RelationKind::Realization => {
                Some((EdgeEnd::Target, EndpointDecorationKind::OpenTriangle))
            }
            RelationKind::Link => None,
        };
        if let Some((end, kind)) = decoration {
            decorate_endpoint(&mut scene, edge_index, end, kind);
        }
        if let Some(multiplicity) = &relation.from_multiplicity {
            annotate_endpoint(&mut scene, edge_index, EdgeEnd::Source, multiplicity);
        }
        if let Some(multiplicity) = &relation.to_multiplicity {
            annotate_endpoint(&mut scene, edge_index, EdgeEnd::Target, multiplicity);
        }
    }
    scene.normalize();
    scene
}
