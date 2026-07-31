//! Core Mermaid `mindmap` subset: one indentation-defined ordered hierarchy.

use crate::parse::{ParseError, Warning, validate_terminal_text};
use crate::scene::{EdgeKind, RoutedEdge, Scene, SceneBox, Shape};
use crate::tree::{self, TreeNode};
use crate::wrapping::{self, MIN_READABLE_COLUMNS};
use unicode_width::UnicodeWidthStr;

const NORMAL_DEPTH_GAP: i32 = 6;
const COMPACT_DEPTH_GAP: i32 = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MindmapNode {
    pub label: String,
    pub parent: Option<usize>,
    pub depth: usize,
    pub line: usize,
}

#[derive(Debug, Default)]
pub struct Mindmap {
    pub nodes: Vec<MindmapNode>,
    pub warnings: Vec<Warning>,
}

impl Mindmap {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn labels(&self) -> Vec<&str> {
        self.nodes.iter().map(|node| node.label.as_str()).collect()
    }

    pub fn levels(&self) -> usize {
        self.nodes
            .iter()
            .map(|node| node.depth + 1)
            .max()
            .unwrap_or(0)
    }
}

pub fn parse(src: &str) -> Result<Mindmap, ParseError> {
    crate::parse::validate_terminal_source(src)?;
    let mut diagram = Mindmap::default();
    let mut seen_header = false;
    let mut stack: Vec<usize> = Vec::new();

    for (line_index, raw) in src.lines().enumerate() {
        let line_number = line_index + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with("%%") {
            continue;
        }
        if !seen_header {
            if trimmed != "mindmap" {
                return Err(error(line_number, "expected `mindmap` header"));
            }
            seen_header = true;
            continue;
        }

        let leading_spaces = raw.bytes().take_while(|byte| *byte == b' ').count();
        let after_spaces = &raw[leading_spaces..];
        if after_spaces.starts_with('\t')
            || after_spaces
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace())
        {
            return Err(error(
                line_number,
                "malformed indentation: use spaces, not tabs or other whitespace",
            ));
        }
        if leading_spaces < 2 || leading_spaces % 2 != 0 {
            return Err(error(
                line_number,
                "malformed indentation: expected increments of two spaces beneath `mindmap`",
            ));
        }
        let depth = leading_spaces / 2 - 1;
        if diagram.nodes.is_empty() && depth != 0 {
            return Err(error(
                line_number,
                "missing root: indent the single root by exactly two spaces",
            ));
        }
        if !diagram.nodes.is_empty() && depth == 0 {
            return Err(error(
                line_number,
                "multiple roots: a core mindmap must contain exactly one root",
            ));
        }
        if depth > stack.len() {
            return Err(error(
                line_number,
                format!(
                    "missing parent at depth {depth}: indent by {} spaces or add the parent first",
                    2 * (stack.len() + 1)
                ),
            ));
        }

        let text = raw[leading_spaces..].trim_end();
        if text.is_empty() {
            return Err(error(line_number, "expected a plain mindmap label"));
        }
        let canonical_root = (depth == 0).then(|| canonical_root_label(text)).flatten();
        if canonical_root.is_none() && unsupported_advanced_syntax(text) {
            return Err(error(
                line_number,
                "unsupported advanced mindmap syntax; expected a plain label",
            ));
        }
        let label = if let Some(label) = canonical_root {
            label.to_string()
        } else {
            text.to_string()
        };
        if label.is_empty() {
            return Err(error(line_number, "expected a non-empty mindmap label"));
        }
        validate_terminal_text(&label, line_number)?;
        let parent = (depth > 0).then(|| stack[depth - 1]);
        let index = diagram.nodes.len();
        diagram.nodes.push(MindmapNode {
            label,
            parent,
            depth,
            line: line_number,
        });
        stack.truncate(depth);
        stack.push(index);
    }

    if !seen_header {
        return Err(error(1, "expected `mindmap` header"));
    }
    Ok(diagram)
}

fn canonical_root_label(text: &str) -> Option<&str> {
    let open = text.find("((")?;
    if open == 0 || !text.ends_with("))") || text[..open].chars().any(char::is_whitespace) {
        return None;
    }
    Some(&text[open + 2..text.len() - 2])
}

fn unsupported_advanced_syntax(text: &str) -> bool {
    if text.contains("::icon(") || text.contains(":::") || text.contains('`') {
        return true;
    }
    let shaped = [("((", "))"), ("{{", "}}"), ("[", "]"), ("(", ")")];
    shaped.iter().any(|&(open, close)| {
        text.find(open).is_some_and(|at| {
            at > 0 && !text[..at].chars().any(char::is_whitespace) && text.ends_with(close)
        })
    })
}

fn error(line: usize, msg: impl Into<String>) -> ParseError {
    ParseError {
        line,
        msg: msg.into(),
    }
}

pub fn dump(diagram: &Mindmap) -> String {
    let mut output = String::from("mindmap\n");
    for (index, node) in diagram.nodes.iter().enumerate() {
        let parent = node
            .parent
            .map(|parent| parent.to_string())
            .unwrap_or_else(|| "-".to_string());
        output.push_str(&format!(
            "node {index} depth={} parent={parent} line={} label=\"{}\"\n",
            node.depth,
            node.line,
            node.label.replace('\\', "\\\\").replace('\"', "\\\"")
        ));
    }
    output
}

pub fn scene(diagram: &Mindmap, max_width: usize) -> Scene {
    if diagram.nodes.is_empty() {
        return Scene::default();
    }

    let unwrapped: Vec<Vec<String>> = diagram
        .nodes
        .iter()
        .map(|node| vec![node.label.clone()])
        .collect();
    let normal = lower(diagram, &unwrapped, NORMAL_DEPTH_GAP);
    if normal.bounds().w.max(0) as usize <= max_width {
        return normal;
    }
    let compact = lower(diagram, &unwrapped, COMPACT_DEPTH_GAP);
    if compact.bounds().w.max(0) as usize <= max_width {
        return compact;
    }

    let caps = wrap_caps(diagram, max_width, COMPACT_DEPTH_GAP as usize);
    let wrapped: Vec<Vec<String>> = diagram
        .nodes
        .iter()
        .map(|node| wrapping::wrap_words(&node.label, caps[node.depth]))
        .collect();
    let wrapped = lower(diagram, &wrapped, COMPACT_DEPTH_GAP);
    if wrapped.bounds().w < compact.bounds().w {
        wrapped
    } else {
        compact
    }
}

fn lower(diagram: &Mindmap, lines: &[Vec<String>], depth_gap: i32) -> Scene {
    let input: Vec<TreeNode> = diagram
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let content_width = lines[index]
                .iter()
                .map(|line| line.width())
                .max()
                .unwrap_or(1)
                .max(1);
            let height = lines[index].len() as i32 + 2;
            TreeNode {
                parent: node.parent,
                // Border + one visible padding cell on each side. Connector
                // paths may replace a border glyph, never this interior pad.
                width: content_width as i32 + 4,
                height: if height % 2 == 0 { height + 1 } else { height },
            }
        })
        .collect();
    let placed = tree::layout(&input, depth_gap);
    let boxes = placed
        .boxes
        .iter()
        .enumerate()
        .map(|(node, &rect)| SceneBox {
            node,
            rect,
            lines: lines[node].clone(),
            shape: Shape::Rounded,
            table: None,
        })
        .collect();
    let edges = placed
        .connections
        .iter()
        .enumerate()
        .map(|(edge, connection)| RoutedEdge {
            edge,
            points: connection.points.clone(),
            rounded: Vec::new(),
            kind: EdgeKind::Solid,
            label: None,
            arrow: None,
        })
        .collect();
    Scene {
        boxes,
        edges,
        ..Scene::default()
    }
}

fn wrap_caps(diagram: &Mindmap, max_width: usize, gap: usize) -> Vec<usize> {
    let levels = diagram.levels().max(1);
    let mut caps = vec![1usize; levels];
    for node in &diagram.nodes {
        caps[node.depth] = caps[node.depth].max(node.label.width().max(1));
    }
    let minimums: Vec<usize> = caps
        .iter()
        .map(|cap| (*cap).min(MIN_READABLE_COLUMNS))
        .collect();
    let overhead = 4usize
        .saturating_mul(levels)
        .saturating_add(gap.saturating_mul(levels.saturating_sub(1)));
    let budget = max_width.saturating_sub(overhead).max(levels);
    while caps.iter().sum::<usize>() > budget {
        let Some((depth, _)) = caps
            .iter()
            .enumerate()
            .filter(|(depth, cap)| **cap > minimums[*depth])
            .max_by_key(|(depth, cap)| (**cap, std::cmp::Reverse(*depth)))
        else {
            break;
        };
        caps[depth] -= 1;
    }
    caps
}
