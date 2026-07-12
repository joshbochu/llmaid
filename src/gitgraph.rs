//! Core Mermaid `gitGraph` subset: ordered commits on stable branch lanes.

use crate::parse::{ParseError, Warning};
use crate::scene::{
    EdgeKind, Point, Rect, RoutedEdge, Scene, SceneBox, ScenePath, SceneText, Shape,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const LABEL_GAP: i32 = 3;
const COLUMN_GAP: i32 = 4;
const ROW_GAP: i32 = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommitType {
    #[default]
    Normal,
    Reverse,
    Highlight,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub id: String,
    pub tag: Option<String>,
    pub kind: CommitType,
    pub branch: usize,
    pub parents: Vec<usize>,
    pub line: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    pub head: Option<usize>,
    pub line: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    Commit(usize),
    Branch(usize),
    Checkout(usize),
    Merge { branch: usize, commit: usize },
}

#[derive(Debug)]
pub struct GitGraph {
    pub branches: Vec<Branch>,
    pub commits: Vec<Commit>,
    pub operations: Vec<Operation>,
    pub current_branch: usize,
    pub warnings: Vec<Warning>,
}

impl Default for GitGraph {
    fn default() -> Self {
        Self {
            branches: vec![Branch {
                name: "main".to_string(),
                head: None,
                line: 1,
            }],
            commits: Vec::new(),
            operations: Vec::new(),
            current_branch: 0,
            warnings: Vec::new(),
        }
    }
}

impl GitGraph {
    pub fn is_empty(&self) -> bool {
        self.commits.is_empty()
    }

    pub fn edge_count(&self) -> usize {
        self.commits.iter().map(|commit| commit.parents.len()).sum()
    }

    pub fn labels(&self) -> Vec<&str> {
        self.branches
            .iter()
            .map(|branch| branch.name.as_str())
            .chain(self.commits.iter().flat_map(|commit| {
                std::iter::once(commit.id.as_str()).chain(commit.tag.iter().map(String::as_str))
            }))
            .collect()
    }
}

pub fn parse(src: &str) -> Result<GitGraph, ParseError> {
    let mut graph = GitGraph::default();
    let mut seen_header = false;
    let mut next_generated_id = 0usize;

    for (line_index, raw) in src.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if !seen_header {
            if line == "gitGraph" {
                seen_header = true;
                graph.branches[0].line = line_number;
                continue;
            }
            if line.starts_with("gitGraph ") {
                return Err(error(
                    line_number,
                    "gitGraph direction is deferred in the core slice; use the exact `gitGraph` header",
                ));
            }
            return Err(error(line_number, "expected `gitGraph` header"));
        }
        if line == "gitGraph" || line.starts_with("gitGraph ") {
            return Err(error(line_number, "duplicate `gitGraph` header"));
        }
        if line.starts_with("cherry-pick") {
            return Err(error(
                line_number,
                "gitGraph cherry-pick is deferred in the core slice",
            ));
        }
        if line == "commit" || line.starts_with("commit ") {
            let attrs = parse_attrs(line.strip_prefix("commit").unwrap(), line_number)?;
            let id = if let Some(id) = attrs.id {
                id
            } else {
                while graph
                    .commits
                    .iter()
                    .any(|commit| commit.id == next_generated_id.to_string())
                {
                    next_generated_id += 1;
                }
                let id = next_generated_id.to_string();
                next_generated_id += 1;
                id
            };
            ensure_new_commit_id(&graph, &id, line_number)?;
            let branch = graph.current_branch;
            let parents = graph.branches[branch].head.into_iter().collect();
            let commit = graph.commits.len();
            graph.commits.push(Commit {
                id,
                tag: attrs.tag,
                kind: attrs.kind,
                branch,
                parents,
                line: line_number,
            });
            graph.branches[branch].head = Some(commit);
            graph.operations.push(Operation::Commit(commit));
            continue;
        }
        if line == "branch" || line.starts_with("branch ") {
            let rest = line.strip_prefix("branch").unwrap().trim();
            let name = parse_name(rest, line_number, "branch name")?;
            if graph.branches.iter().any(|branch| branch.name == name) {
                return Err(error(
                    line_number,
                    format!("branch `{name}` already exists; choose a new branch name"),
                ));
            }
            let branch = graph.branches.len();
            graph.branches.push(Branch {
                name,
                head: graph.branches[graph.current_branch].head,
                line: line_number,
            });
            graph.current_branch = branch;
            graph.operations.push(Operation::Branch(branch));
            continue;
        }
        if line == "checkout"
            || line.starts_with("checkout ")
            || line == "switch"
            || line.starts_with("switch ")
        {
            let keyword = if line.starts_with("checkout") {
                "checkout"
            } else {
                "switch"
            };
            let rest = line.strip_prefix(keyword).unwrap().trim();
            let name = parse_name(rest, line_number, "branch name after checkout or switch")?;
            let branch = find_branch(&graph, &name, line_number)?;
            graph.current_branch = branch;
            graph.operations.push(Operation::Checkout(branch));
            continue;
        }
        if line == "merge" || line.starts_with("merge ") {
            let rest = line.strip_prefix("merge").unwrap().trim();
            let (name, attrs) = parse_name_and_attrs(rest, line_number)?;
            let source = find_branch(&graph, &name, line_number)?;
            if source == graph.current_branch {
                return Err(error(
                    line_number,
                    "cannot merge the current branch into itself; checkout the target branch first",
                ));
            }
            let Some(source_head) = graph.branches[source].head else {
                return Err(error(
                    line_number,
                    format!("branch `{name}` has no commits to merge"),
                ));
            };
            let target = graph.current_branch;
            let Some(target_head) = graph.branches[target].head else {
                return Err(error(
                    line_number,
                    format!(
                        "current branch `{}` has no commits; commit before merging",
                        graph.branches[target].name
                    ),
                ));
            };
            let id = if let Some(id) = attrs.id {
                id
            } else {
                while graph
                    .commits
                    .iter()
                    .any(|commit| commit.id == next_generated_id.to_string())
                {
                    next_generated_id += 1;
                }
                let id = next_generated_id.to_string();
                next_generated_id += 1;
                id
            };
            ensure_new_commit_id(&graph, &id, line_number)?;
            let commit = graph.commits.len();
            graph.commits.push(Commit {
                id,
                tag: attrs.tag,
                kind: attrs.kind,
                branch: target,
                parents: vec![target_head, source_head],
                line: line_number,
            });
            graph.branches[target].head = Some(commit);
            graph.operations.push(Operation::Merge {
                branch: source,
                commit,
            });
            continue;
        }
        return Err(error(
            line_number,
            "expected `commit`, `branch`, `checkout`, `switch`, or `merge` gitGraph syntax",
        ));
    }

    if !seen_header {
        return Err(error(1, "expected `gitGraph` header"));
    }
    Ok(graph)
}

#[derive(Default)]
struct Attributes {
    id: Option<String>,
    tag: Option<String>,
    kind: CommitType,
    saw_type: bool,
}

fn parse_attrs(raw: &str, line: usize) -> Result<Attributes, ParseError> {
    let mut attrs = Attributes::default();
    let mut rest = raw.trim();
    while !rest.is_empty() {
        let Some(colon) = rest.find(':') else {
            return Err(error(
                line,
                "expected commit attribute `id:`, `tag:`, or `type:`",
            ));
        };
        let key = rest[..colon].trim();
        if key.contains(char::is_whitespace) || !matches!(key, "id" | "tag" | "type") {
            return Err(error(
                line,
                "expected commit attribute `id:`, `tag:`, or `type:`",
            ));
        }
        rest = rest[colon + 1..].trim_start();
        if key == "type" {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let value = &rest[..end];
            if value.is_empty() {
                return Err(error(
                    line,
                    "expected NORMAL, REVERSE, or HIGHLIGHT after `type:`",
                ));
            }
            if attrs.saw_type {
                return Err(error(line, "commit `type` may be specified only once"));
            }
            attrs.kind = match value {
                "NORMAL" => CommitType::Normal,
                "REVERSE" => CommitType::Reverse,
                "HIGHLIGHT" => CommitType::Highlight,
                _ => {
                    return Err(error(
                        line,
                        "expected commit type NORMAL, REVERSE, or HIGHLIGHT",
                    ));
                }
            };
            attrs.saw_type = true;
            rest = rest[end..].trim_start();
        } else {
            let (value, tail) = parse_quoted(rest, line)?;
            validate_label(&value, line, "commit attribute")?;
            let slot = if key == "id" {
                &mut attrs.id
            } else {
                &mut attrs.tag
            };
            if slot.is_some() {
                return Err(error(
                    line,
                    format!("commit `{key}` may be specified only once"),
                ));
            }
            *slot = Some(value);
            rest = tail.trim_start();
        }
    }
    Ok(attrs)
}

fn parse_name_and_attrs(raw: &str, line: usize) -> Result<(String, Attributes), ParseError> {
    if raw.is_empty() {
        return Err(error(line, "expected a branch name after `merge`"));
    }
    let (name, rest) = if raw.starts_with('"') {
        parse_quoted(raw, line)?
    } else {
        let end = raw.find(char::is_whitespace).unwrap_or(raw.len());
        (raw[..end].to_string(), &raw[end..])
    };
    validate_label(&name, line, "branch name")?;
    Ok((name, parse_attrs(rest, line)?))
}

fn parse_name(raw: &str, line: usize, expected: &str) -> Result<String, ParseError> {
    if raw.is_empty() {
        return Err(error(line, format!("expected a non-empty {expected}")));
    }
    let name = if raw.starts_with('"') {
        let (name, rest) = parse_quoted(raw, line)?;
        if !rest.trim().is_empty() {
            return Err(error(line, format!("unexpected text after {expected}")));
        }
        name
    } else {
        if raw.contains(char::is_whitespace) {
            return Err(error(line, format!("quote a {expected} containing spaces")));
        }
        raw.to_string()
    };
    validate_label(&name, line, expected)?;
    Ok(name)
}

fn parse_quoted(raw: &str, line: usize) -> Result<(String, &str), ParseError> {
    let Some(body) = raw.strip_prefix('"') else {
        return Err(error(
            line,
            "expected a quoted string after `id:` or `tag:`",
        ));
    };
    let mut escaped = false;
    let mut value = String::new();
    for (index, ch) in body.char_indices() {
        if escaped {
            value.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return Ok((value, &body[index + ch.len_utf8()..]));
        } else {
            value.push(ch);
        }
    }
    Err(error(
        line,
        "unterminated quoted string; expected closing `\"`",
    ))
}

fn validate_label(value: &str, line: usize, expected: &str) -> Result<(), ParseError> {
    if value.is_empty() {
        return Err(error(line, format!("expected a non-empty {expected}")));
    }
    if value.chars().any(|ch| ch.width() == Some(0)) {
        return Err(error(
            line,
            "unsupported zero-width Unicode sequence in core gitGraph label; use precomposed text or a single-scalar emoji",
        ));
    }
    Ok(())
}

fn find_branch(graph: &GitGraph, name: &str, line: usize) -> Result<usize, ParseError> {
    graph
        .branches
        .iter()
        .position(|branch| branch.name == name)
        .ok_or_else(|| {
            error(
                line,
                format!("unknown branch `{name}`; declare it with `branch {name}` first"),
            )
        })
}

fn ensure_new_commit_id(graph: &GitGraph, id: &str, line: usize) -> Result<(), ParseError> {
    if graph.commits.iter().any(|commit| commit.id == id) {
        return Err(error(
            line,
            format!("duplicate commit id `{id}`; commit ids must be unique"),
        ));
    }
    Ok(())
}

fn error(line: usize, msg: impl Into<String>) -> ParseError {
    ParseError {
        line,
        msg: msg.into(),
    }
}

pub fn dump(graph: &GitGraph) -> String {
    let mut output = String::from("gitGraph\n");
    for (index, branch) in graph.branches.iter().enumerate() {
        let head = branch
            .head
            .map_or_else(|| "-".to_string(), |head| head.to_string());
        output.push_str(&format!(
            "branch {index} head={head} line={} name=\"{}\"\n",
            branch.line,
            escape(&branch.name)
        ));
    }
    for (index, commit) in graph.commits.iter().enumerate() {
        let parents = if commit.parents.is_empty() {
            "-".to_string()
        } else {
            commit
                .parents
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",")
        };
        let tag = commit
            .tag
            .as_ref()
            .map_or_else(|| "-".to_string(), |tag| format!("\"{}\"", escape(tag)));
        output.push_str(&format!(
            "commit {index} branch={} parents={parents} type={} line={} id=\"{}\" tag={tag}\n",
            commit.branch,
            kind_name(commit.kind),
            commit.line,
            escape(&commit.id)
        ));
    }
    output
}

fn kind_name(kind: CommitType) -> &'static str {
    match kind {
        CommitType::Normal => "normal",
        CommitType::Reverse => "reverse",
        CommitType::Highlight => "highlight",
    }
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

pub fn scene(graph: &GitGraph, max_width: usize) -> Scene {
    if graph.commits.is_empty() {
        return Scene::default();
    }
    let branch_width = graph
        .branches
        .iter()
        .map(|branch| branch.name.width())
        .max()
        .unwrap_or(1);
    let available = max_width.saturating_sub(branch_width + LABEL_GAP as usize);
    let cap = if graph.commits.len() <= 1 {
        available.saturating_sub(4).max(1)
    } else {
        available
            .saturating_sub((graph.commits.len() - 1) * COLUMN_GAP as usize)
            .checked_div(graph.commits.len())
            .unwrap_or(1)
            .saturating_sub(4)
            .max(1)
    };
    let lines: Vec<Vec<String>> = graph
        .commits
        .iter()
        .map(|commit| {
            let label = commit.tag.as_ref().map_or_else(
                || commit.id.clone(),
                |tag| format!("{} [{}]", commit.id, tag),
            );
            wrap_label(&label, cap)
        })
        .collect();
    lower(graph, &lines, branch_width as i32 + LABEL_GAP)
}

fn lower(graph: &GitGraph, lines: &[Vec<String>], start_x: i32) -> Scene {
    let max_height = lines
        .iter()
        .map(|lines| lines.len() as i32 + 2)
        .max()
        .unwrap_or(3);
    let row_step = max_height + ROW_GAP;
    let mut rects = Vec::with_capacity(graph.commits.len());
    let mut x = start_x;
    for (commit, content) in graph.commits.iter().zip(lines) {
        let width = content.iter().map(|line| line.width()).max().unwrap_or(1) as i32 + 4;
        let height = content.len() as i32 + 2;
        let y = commit.branch as i32 * row_step + (max_height - height) / 2;
        rects.push(Rect::new(x, y, width, height));
        x += width + COLUMN_GAP;
    }

    let boxes = graph
        .commits
        .iter()
        .enumerate()
        .map(|(node, commit)| SceneBox {
            node,
            rect: rects[node],
            lines: lines[node].clone(),
            shape: match commit.kind {
                CommitType::Normal => Shape::Rounded,
                CommitType::Reverse => Shape::Rect,
                CommitType::Highlight => Shape::Stadium,
            },
            table: None,
        })
        .collect();

    let mut edges = Vec::new();
    for (child, commit) in graph.commits.iter().enumerate() {
        for &parent in &commit.parents {
            let from = rects[parent];
            let to = rects[child];
            let start = Point::new(from.right() - 1, from.center2().y / 2);
            let end = Point::new(to.x, to.center2().y / 2);
            let points = if start.y == end.y {
                vec![start, end]
            } else {
                let turn_x = end.x - 2;
                vec![
                    start,
                    Point::new(turn_x, start.y),
                    Point::new(turn_x, end.y),
                    end,
                ]
            };
            edges.push(RoutedEdge {
                edge: edges.len(),
                points,
                rounded: Vec::new(),
                kind: EdgeKind::Solid,
                label: None,
                arrow: None,
            });
        }
    }

    let texts = graph
        .branches
        .iter()
        .enumerate()
        .map(|(branch, value)| {
            let y = branch as i32 * row_step + max_height / 2;
            SceneText::new(Point::new(0, y), value.name.clone())
        })
        .collect();
    let paths = graph
        .branches
        .iter()
        .enumerate()
        .map(|(branch, _)| {
            let y = branch as i32 * row_step + max_height / 2;
            ScenePath {
                path: branch,
                points: vec![Point::new(start_x - 2, y), Point::new(x - COLUMN_GAP, y)],
                rounded: Vec::new(),
                kind: EdgeKind::Dotted,
            }
        })
        .collect();

    Scene {
        foreground_boxes: boxes,
        paths,
        edges,
        texts,
        ..Scene::default()
    }
}

fn wrap_label(label: &str, cap: usize) -> Vec<String> {
    let forced: Vec<&str> = label.split('\n').collect();
    let mut lines = Vec::new();
    for part in forced {
        let mut current = String::new();
        for word in part.split_whitespace() {
            if current.is_empty() {
                push_chunks(word, cap, &mut current, &mut lines);
            } else if current.width() + 1 + word.width() <= cap {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(std::mem::take(&mut current));
                push_chunks(word, cap, &mut current, &mut lines);
            }
        }
        if !current.is_empty() {
            lines.push(current);
        } else if part.is_empty() {
            lines.push(String::new());
        }
    }
    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}

fn push_chunks(word: &str, cap: usize, current: &mut String, lines: &mut Vec<String>) {
    for ch in word.chars() {
        let width = ch.width().unwrap_or(0);
        if !current.is_empty() && current.width() + width > cap {
            lines.push(std::mem::take(current));
        }
        current.push(ch);
    }
}
