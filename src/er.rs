//! Mermaid `erDiagram` subset -> deterministic entity/relationship IR.

use std::collections::HashMap;

use crate::boxed::{BoxDiagram, BoxNode, EdgeEnd, NodeId, decorate_endpoint};
use crate::parse::{Dir, ParseError, Warning, validate_terminal_text};
use crate::scene::{
    CardinalityMaximum, CardinalityMinimum, EdgeKind, EndpointDecorationKind, Scene, SceneTable,
    Shape,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeKey {
    Primary,
    Foreign,
    Unique,
}

impl AttributeKey {
    fn token(self) -> &'static str {
        match self {
            Self::Primary => "PK",
            Self::Foreign => "FK",
            Self::Unique => "UK",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub data_type: String,
    pub name: String,
    pub keys: Vec<AttributeKey>,
    pub comment: Option<String>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entity {
    pub id: String,
    pub label: String,
    pub attributes: Vec<Attribute>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationshipKind {
    Identifying,
    NonIdentifying,
}

impl RelationshipKind {
    pub fn connector(self) -> &'static str {
        match self {
            Self::Identifying => "--",
            Self::NonIdentifying => "..",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relationship {
    pub from: usize,
    pub to: usize,
    /// Exact Mermaid cardinality at the left endpoint (`||`, `|o`, `}o`, `}|`).
    pub left_cardinality: String,
    /// Exact Mermaid cardinality at the right endpoint (`||`, `o|`, `o{`, `|{`).
    pub right_cardinality: String,
    pub kind: RelationshipKind,
    pub label: String,
    pub line: usize,
}

/// Entity order is first declaration/use order, never map iteration order.
#[derive(Debug, Default)]
pub struct ErDiagram {
    pub direction: Option<Dir>,
    pub entities: Vec<Entity>,
    pub relationships: Vec<Relationship>,
    pub warnings: Vec<Warning>,
    index: HashMap<String, usize>,
}

impl ErDiagram {
    fn entity(&mut self, id: &str, label: Option<String>, line: usize) -> usize {
        if let Some(&index) = self.index.get(id) {
            if let Some(label) = label {
                if self.entities[index].label != id && self.entities[index].label != label {
                    self.warnings.push(Warning {
                        line,
                        msg: format!("entity `{id}` redeclared; last alias wins"),
                    });
                }
                self.entities[index].label = label;
            }
            return index;
        }
        let index = self.entities.len();
        self.entities.push(Entity {
            id: id.to_string(),
            label: label.unwrap_or_else(|| id.to_string()),
            attributes: Vec::new(),
        });
        self.index.insert(id.to_string(), index);
        index
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    pub fn labels(&self) -> Vec<&str> {
        let mut labels = Vec::new();
        for entity in &self.entities {
            labels.push(entity.label.as_str());
            for attribute in &entity.attributes {
                labels.push(attribute.data_type.as_str());
                labels.push(attribute.name.as_str());
                if let Some(comment) = &attribute.comment {
                    labels.push(comment.as_str());
                }
            }
        }
        labels.extend(
            self.relationships
                .iter()
                .map(|relationship| relationship.label.as_str()),
        );
        labels
    }
}

pub fn parse(src: &str) -> Result<ErDiagram, ParseError> {
    crate::parse::validate_terminal_source(src)?;
    let mut diagram = ErDiagram::default();
    let mut seen_header = false;
    let mut open_entity: Option<(usize, usize)> = None;

    for (line_index, raw) in src.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }

        if !seen_header {
            if line != "erDiagram" {
                return Err(error(line_number, "expected `erDiagram` header"));
            }
            seen_header = true;
            continue;
        }

        if let Some((entity, _)) = open_entity {
            if line == "}" {
                open_entity = None;
            } else if line.contains('{') || line.contains('}') {
                return Err(error(
                    line_number,
                    "expected an entity attribute or closing `}`",
                ));
            } else {
                let attribute = parse_attribute(line, line_number)?;
                diagram.entities[entity].attributes.push(attribute);
            }
            continue;
        }

        if line == "erDiagram" {
            diagram.warnings.push(Warning {
                line: line_number,
                msg: "duplicate `erDiagram` header ignored".to_string(),
            });
        } else if line == "direction" || line.starts_with("direction ") {
            parse_direction(&mut diagram, &line["direction".len()..], line_number)?;
        } else if is_ignored_directive(line) {
            diagram.warnings.push(Warning {
                line: line_number,
                msg: format!("unsupported ER styling directive ignored: `{line}`"),
            });
        } else if relationship_operator(line).is_some() {
            parse_relationship(&mut diagram, line, line_number)?;
        } else {
            parse_entity(&mut diagram, line, line_number, &mut open_entity)?;
        }
    }

    if !seen_header {
        return Err(error(1, "expected `erDiagram` header"));
    }
    if let Some((entity, line)) = open_entity {
        return Err(error(
            line,
            format!(
                "expected a closing `}}` for entity `{}`",
                diagram.entities[entity].id
            ),
        ));
    }
    Ok(diagram)
}

fn parse_direction(diagram: &mut ErDiagram, rest: &str, line: usize) -> Result<(), ParseError> {
    let direction = match rest.trim() {
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

fn parse_entity(
    diagram: &mut ErDiagram,
    line: &str,
    line_number: usize,
    open_entity: &mut Option<(usize, usize)>,
) -> Result<(), ParseError> {
    let (head, opens) = match line.strip_suffix('{') {
        Some(head) => (head.trim(), true),
        None => (line, false),
    };
    let (id, label) = if let Some(open) = head.find('[') {
        if !head.ends_with(']') {
            return Err(error(
                line_number,
                "expected a closing `]` for the entity alias",
            ));
        }
        let id = head[..open].trim();
        let label = head[open + 1..head.len() - 1].trim();
        if label.is_empty() {
            return Err(error(line_number, "expected a non-empty entity alias"));
        }
        if label.starts_with('"') != label.ends_with('"') {
            return Err(error(
                line_number,
                "expected a closing `\"` for the entity alias",
            ));
        }
        let label = unquote(label);
        validate_terminal_text(label, line_number)?;
        (id, Some(label.to_string()))
    } else {
        (head, None)
    };
    if !valid_id(id) {
        return Err(error(
            line_number,
            "expected an entity id, optional `[alias]`, and optional `{`",
        ));
    }
    let entity = diagram.entity(id, label, line_number);
    if opens {
        *open_entity = Some((entity, line_number));
    }
    Ok(())
}

fn parse_attribute(line: &str, line_number: usize) -> Result<Attribute, ParseError> {
    let mut parts = line.splitn(3, char::is_whitespace);
    let data_type = parts.next().unwrap_or("").trim();
    let name = parts.next().unwrap_or("").trim();
    let rest = parts.next().unwrap_or("").trim();
    if data_type.is_empty()
        || name.is_empty()
        || !valid_attribute_word(data_type)
        || !valid_id(name)
    {
        return Err(error(
            line_number,
            "expected an attribute type and name, optional PK/FK/UK keys, and optional quoted comment",
        ));
    }

    let (key_text, comment) = if let Some(quote) = rest.find('"') {
        let key_text = rest[..quote].trim();
        let quoted = &rest[quote..];
        if !quoted.ends_with('"') || quoted.len() < 2 {
            return Err(error(
                line_number,
                "expected a closing `\"` for the attribute comment",
            ));
        }
        let comment = &quoted[1..quoted.len() - 1];
        validate_terminal_text(comment, line_number)?;
        (key_text, Some(comment.to_string()))
    } else {
        (rest, None)
    };

    let mut keys = Vec::new();
    for token in key_text.split(|character: char| character == ',' || character.is_whitespace()) {
        if token.is_empty() {
            continue;
        }
        let key = match token {
            "PK" => AttributeKey::Primary,
            "FK" => AttributeKey::Foreign,
            "UK" => AttributeKey::Unique,
            _ => {
                return Err(error(
                    line_number,
                    "expected only PK, FK, or UK attribute key markers before the quoted comment",
                ));
            }
        };
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    Ok(Attribute {
        data_type: data_type.to_string(),
        name: name.to_string(),
        keys,
        comment,
        line: line_number,
    })
}

fn parse_relationship(
    diagram: &mut ErDiagram,
    line: &str,
    line_number: usize,
) -> Result<(), ParseError> {
    let Some((head, raw_label)) = line.split_once(':') else {
        return Err(error(
            line_number,
            "expected `:` followed by a relationship label",
        ));
    };
    let raw_label = raw_label.trim();
    let label = if let Some(label) = raw_label.strip_prefix('"') {
        let Some(label) = label.strip_suffix('"') else {
            return Err(error(
                line_number,
                "expected a closing `\"` for the relationship label",
            ));
        };
        label
    } else {
        raw_label
    };
    if label.is_empty() {
        return Err(error(
            line_number,
            "expected a relationship label after `:`",
        ));
    }
    validate_terminal_text(label, line_number)?;
    let mut tokens = head.split_whitespace();
    let from_id = tokens.next().unwrap_or("");
    let operator = tokens.next().unwrap_or("");
    let to_id = tokens.next().unwrap_or("");
    if tokens.next().is_some() || !valid_id(from_id) || !valid_id(to_id) {
        return Err(error(
            line_number,
            "expected `ENTITY cardinality--cardinality ENTITY : label`",
        ));
    }
    let (left, kind, right) = parse_relationship_operator(operator, line_number)?;
    let from = diagram.entity(from_id, None, line_number);
    let to = diagram.entity(to_id, None, line_number);
    diagram.relationships.push(Relationship {
        from,
        to,
        left_cardinality: left.to_string(),
        right_cardinality: right.to_string(),
        kind,
        label: label.to_string(),
        line: line_number,
    });
    Ok(())
}

fn relationship_operator(line: &str) -> Option<&str> {
    let head = line.split_once(':').map_or(line, |(head, _)| head);
    let mut tokens = head.split_whitespace();
    let left = tokens.next()?;
    let operator = tokens.next()?;
    let right = tokens.next()?;
    if tokens.next().is_none()
        && valid_id(left)
        && valid_id(right)
        && (operator.contains("--") || operator.contains(".."))
    {
        Some(operator)
    } else {
        None
    }
}

fn parse_relationship_operator(
    operator: &str,
    line: usize,
) -> Result<(&str, RelationshipKind, &str), ParseError> {
    let (left, kind, right) = if let Some((left, right)) = operator.split_once("--") {
        (left, RelationshipKind::Identifying, right)
    } else if let Some((left, right)) = operator.split_once("..") {
        (left, RelationshipKind::NonIdentifying, right)
    } else {
        return Err(relationship_error(line));
    };
    if !matches!(left, "||" | "|o" | "}o" | "}|") || !matches!(right, "||" | "o|" | "o{" | "|{") {
        return Err(relationship_error(line));
    }
    Ok((left, kind, right))
}

fn relationship_error(line: usize) -> ParseError {
    error(
        line,
        "expected valid zero/one/many cardinalities around `--` or `..`",
    )
}

fn is_ignored_directive(line: &str) -> bool {
    matches!(
        line.split_whitespace().next().unwrap_or(""),
        "classDef" | "class" | "style" | "cssClass"
    )
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|character| character.is_alphanumeric() || matches!(character, '_' | '-'))
}

fn valid_attribute_word(word: &str) -> bool {
    word.chars().all(|character| {
        character.is_alphanumeric() || matches!(character, '_' | '-' | '(' | ')' | '[' | ']')
    })
}

fn unquote(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .unwrap_or(text)
}

fn error(line: usize, msg: impl Into<String>) -> ParseError {
    ParseError {
        line,
        msg: msg.into(),
    }
}

pub fn dump(diagram: &ErDiagram) -> String {
    let direction = diagram.direction.unwrap_or(Dir::TB);
    let mut out = format!("type: er\ndirection: {}\nentities:\n", direction.name());
    for (index, entity) in diagram.entities.iter().enumerate() {
        out.push_str(&format!("  {index}: {} [{}]\n", entity.id, entity.label));
        for attribute in &entity.attributes {
            out.push_str(&format!(
                "    attribute: {} {}",
                attribute.data_type, attribute.name
            ));
            for key in &attribute.keys {
                out.push_str(&format!(" {}", key.token()));
            }
            if let Some(comment) = &attribute.comment {
                out.push_str(&format!(" \"{comment}\""));
            }
            out.push('\n');
        }
    }
    out.push_str("relationships:\n");
    for relationship in &diagram.relationships {
        out.push_str(&format!(
            "  {} {}{}{} {} : \"{}\"\n",
            diagram.entities[relationship.from].id,
            relationship.left_cardinality,
            relationship.kind.connector(),
            relationship.right_cardinality,
            diagram.entities[relationship.to].id,
            relationship.label
        ));
    }
    out
}

/// Lower ER semantics into attribute tables and endpoint cardinality glyphs.
pub fn scene(diagram: &ErDiagram, width: usize) -> Scene {
    let mut boxed = BoxDiagram::new(diagram.direction.unwrap_or(Dir::TB));
    let tables: Vec<SceneTable> = diagram
        .entities
        .iter()
        .map(|entity| {
            SceneTable::new(
                entity.label.clone(),
                entity
                    .attributes
                    .iter()
                    .map(|attribute| {
                        vec![
                            attribute.data_type.clone(),
                            attribute.name.clone(),
                            attribute
                                .keys
                                .iter()
                                .map(|key| key.token())
                                .collect::<Vec<_>>()
                                .join(" "),
                            attribute.comment.clone().unwrap_or_default(),
                        ]
                    })
                    .collect(),
            )
            .with_row_dividers()
        })
        .collect();
    let nodes: Vec<NodeId> = diagram
        .entities
        .iter()
        .zip(&tables)
        .map(|(entity, table)| {
            boxed.add_node(BoxNode::new(
                &entity.id,
                table.layout_label(),
                Shape::Rounded,
            ))
        })
        .collect();
    for relationship in &diagram.relationships {
        let mut edge = boxed.add_edge(nodes[relationship.from], nodes[relationship.to]);
        edge.label(relationship.label.clone());
        edge.without_arrow();
        edge.endpoint_spacing(4);
        if relationship.kind == RelationshipKind::NonIdentifying {
            edge.kind(EdgeKind::Dotted);
        }
    }

    // Attribute columns remain intact under narrow budgets. Relationship
    // channels still compact before the documented over-width fallback rather
    // than corrupting table structure with plain-label wrapping.
    let mut scene = boxed.scene_preserving_labels(width);
    for (box_, table) in scene.boxes.iter_mut().zip(tables) {
        box_.lines.clear();
        box_.table = Some(table);
    }
    for (edge_index, relationship) in diagram.relationships.iter().enumerate() {
        decorate_endpoint(
            &mut scene,
            edge_index,
            EdgeEnd::Source,
            cardinality_decoration(&relationship.left_cardinality),
        );
        decorate_endpoint(
            &mut scene,
            edge_index,
            EdgeEnd::Target,
            cardinality_decoration(&relationship.right_cardinality),
        );
    }
    scene.normalize();
    scene
}

fn cardinality_decoration(token: &str) -> EndpointDecorationKind {
    let (minimum, maximum) = match token {
        "||" => (CardinalityMinimum::One, CardinalityMaximum::One),
        "|o" | "o|" => (CardinalityMinimum::Zero, CardinalityMaximum::One),
        "}o" | "o{" => (CardinalityMinimum::Zero, CardinalityMaximum::Many),
        "}|" | "|{" => (CardinalityMinimum::One, CardinalityMaximum::Many),
        _ => unreachable!("parser validates ER cardinalities"),
    };
    EndpointDecorationKind::Cardinality { minimum, maximum }
}
