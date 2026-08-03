//! Mermaid flowchart subset -> IR.
//!
//! Forgiving by design: unknown directives (classDef, style, ...)
//! become warnings, not errors. Errors carry the line number and what was
//! expected, so an agent can self-correct in one retry.

use std::collections::HashMap;

use crate::scanner;
pub use crate::scene::{EdgeKind, Shape};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    LR,
    RL,
    TB,
    BT,
}

impl Dir {
    fn from_token(s: &str) -> Option<Dir> {
        match s {
            "LR" => Some(Dir::LR),
            "RL" => Some(Dir::RL),
            "TB" | "TD" => Some(Dir::TB),
            "BT" => Some(Dir::BT),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Dir::LR => "LR",
            Dir::RL => "RL",
            Dir::TB => "TB",
            Dir::BT => "BT",
        }
    }
}

#[derive(Debug)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub shape: Shape,
}

/// A semantic endpoint of a flowchart edge. Layout keeps a real member node
/// as an internal proxy for a subgraph endpoint, but the endpoint identity is
/// retained so routing and inspection attach to the visible frame instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endpoint {
    Node(usize),
    Subgraph(usize),
}

#[derive(Debug)]
pub struct Edge {
    /// Real node used by the layered layout. For a subgraph endpoint this is
    /// a deterministic non-rendered proxy choice from that subgraph.
    pub from: usize,
    pub to: usize,
    /// Declared Mermaid source endpoint, retained independently of the
    /// layout proxy so a subgraph never becomes a phantom node box.
    pub source: Endpoint,
    pub target: Endpoint,
    pub kind: EdgeKind,
    pub arrow: bool,
    pub label: Option<String>,
    /// Extra flow-axis cells reserved for paint-level endpoint adornments.
    pub endpoint_reserve: usize,
    /// Keep this edge on its own endpoint ports instead of sharing a
    /// fork/merge trunk with distinct peers.
    pub distinct_endpoints: bool,
    pub(crate) line: usize,
}

impl Edge {
    /// Canonical textual form of the edge operator, e.g. `-.->`.
    pub fn op(&self) -> &'static str {
        match (self.kind, self.arrow) {
            (EdgeKind::Solid, true) => "-->",
            (EdgeKind::Solid, false) => "---",
            (EdgeKind::Dotted, true) => "-.->",
            (EdgeKind::Dotted, false) => "-.-",
            (EdgeKind::Thick, true) => "==>",
            (EdgeKind::Thick, false) => "===",
        }
    }
}

#[derive(Debug)]
pub struct Warning {
    pub line: usize,
    pub msg: String,
}

/// A Mermaid `subgraph` … `end` group. Members are node indices in declaration
/// order. Nested groups set `parent` to the enclosing subgraph index.
#[derive(Debug)]
pub struct Subgraph {
    pub id: String,
    pub title: String,
    pub parent: Option<usize>,
    pub members: Vec<usize>,
}

#[derive(Debug)]
pub struct ParseError {
    pub line: usize,
    pub msg: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.msg)
    }
}

/// Parsed graph. `nodes` and `edges` are in declaration order — layout and
/// rendering must iterate these (never a HashMap) to stay deterministic.
#[derive(Debug, Default)]
pub struct Graph {
    pub dir: Option<Dir>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub subgraphs: Vec<Subgraph>,
    pub warnings: Vec<Warning>,
    index: HashMap<String, usize>,
    /// Declared subgraph IDs collected before statement parsing. This permits
    /// Mermaid's common forward references such as `A --> workers` before
    /// `subgraph workers` without temporarily creating a node named workers.
    subgraph_index: HashMap<String, usize>,
    /// Parse-time stack of open subgraph indices (not part of the public IR).
    sg_stack: Vec<usize>,
}

impl Graph {
    pub fn direction(&self) -> Dir {
        self.dir.unwrap_or(Dir::TB)
    }

    fn add_node(&mut self, id: &str, shaped: Option<(Shape, String)>, line: usize) -> usize {
        if let Some(&ix) = self.index.get(id) {
            if let Some((shape, label)) = shaped {
                let node = &mut self.nodes[ix];
                // B2: redeclaration is probably an agent mistake — last wins, but warn.
                if node.shape != shape || node.label != label {
                    self.warnings.push(Warning {
                        line,
                        msg: format!(
                            "node `{id}` redeclared; last definition wins (was {} \"{}\")",
                            node.shape.name(),
                            node.label.replace('\n', "\\n"),
                        ),
                    });
                }
                node.shape = shape;
                node.label = label;
            }
            return ix;
        }
        let (shape, label) = shaped.unwrap_or((Shape::Rect, id.to_string()));
        self.nodes.push(Node {
            id: id.to_string(),
            label,
            shape,
        });
        let ix = self.nodes.len() - 1;
        self.index.insert(id.to_string(), ix);
        let sg = self.sg_stack.last().copied();
        if let Some(sgi) = sg {
            self.subgraphs[sgi].members.push(ix);
        }
        ix
    }

    fn open_subgraph(&mut self, id: String, title: String) {
        let parent = self.sg_stack.last().copied();
        let sgi = self.subgraphs.len();
        self.subgraph_index.entry(id.clone()).or_insert(sgi);
        self.subgraphs.push(Subgraph {
            id,
            title,
            parent,
            members: Vec::new(),
        });
        self.sg_stack.push(sgi);
    }

    fn close_subgraph(&mut self, line: usize) {
        if self.sg_stack.pop().is_none() {
            self.warnings.push(Warning {
                line,
                msg: "`end` without an open subgraph".to_string(),
            });
        }
    }

    fn resolve_edge_proxies(&mut self) -> Result<(), ParseError> {
        let proxies: Result<Vec<(usize, usize)>, ParseError> = self
            .edges
            .iter()
            .map(|edge| {
                Ok((
                    self.endpoint_proxy(edge.source, true, edge.line)?,
                    self.endpoint_proxy(edge.target, false, edge.line)?,
                ))
            })
            .collect();
        for (edge, (from, to)) in self.edges.iter_mut().zip(proxies?) {
            edge.from = from;
            edge.to = to;
        }
        Ok(())
    }

    fn endpoint_proxy(
        &self,
        endpoint: Endpoint,
        source: bool,
        line: usize,
    ) -> Result<usize, ParseError> {
        match endpoint {
            Endpoint::Node(node) => Ok(node),
            Endpoint::Subgraph(group) => self.group_proxy(group, source).ok_or_else(|| ParseError {
                line,
                msg: format!(
                    "subgraph `{}` used as an edge endpoint has no member node; add a member before connecting it",
                    self.subgraphs
                        .get(group)
                        .map(|value| value.id.as_str())
                        .unwrap_or("<unknown>")
                ),
            }),
        }
    }

    /// Choose a deterministic layout-only representative for a group endpoint.
    /// An incoming group endpoint uses the first internal entry node; an
    /// outgoing endpoint uses the last internal exit node. The semantic
    /// endpoint itself remains the frame and is never rendered as this node.
    fn group_proxy(&self, group: usize, source: bool) -> Option<usize> {
        let members: Vec<usize> = (0..self.nodes.len())
            .filter(|&node| self.group_contains_node(group, node))
            .collect();
        let candidates: Vec<usize> = members
            .iter()
            .copied()
            .filter(|&node| {
                !self.edges.iter().any(|edge| {
                    let from = matches!(edge.source, Endpoint::Node(value) if value == node);
                    let to = matches!(edge.target, Endpoint::Node(value) if value == node);
                    if source {
                        from
                            && matches!(edge.target, Endpoint::Node(other) if self.group_contains_node(group, other))
                    } else {
                        to && matches!(edge.source, Endpoint::Node(other) if self.group_contains_node(group, other))
                    }
                })
            })
            .collect();
        if source {
            candidates
                .last()
                .copied()
                .or_else(|| members.last().copied())
        } else {
            candidates
                .first()
                .copied()
                .or_else(|| members.first().copied())
        }
    }

    fn group_contains_node(&self, group: usize, node: usize) -> bool {
        self.subgraphs[group].members.contains(&node)
            || self.subgraphs.iter().enumerate().any(|(child, value)| {
                value.parent == Some(group) && self.group_contains_node(child, node)
            })
    }
}

pub fn parse(src: &str) -> Result<Graph, ParseError> {
    validate_terminal_source(src)?;
    let mut subgraph_index = HashMap::new();
    let mut subgraph_ordinal = 0usize;
    for raw_line in src.lines() {
        for stmt in scanner::statements(raw_line) {
            let stmt = stmt.trim();
            let keyword = stmt.split_whitespace().next().unwrap_or("");
            if matches!(
                scanner::classify_non_edge_keyword(keyword),
                Some(scanner::NonEdgeKeyword::Subgraph)
            ) {
                let (id, _) = parse_subgraph_header(stmt["subgraph".len()..].trim());
                subgraph_index.entry(id).or_insert(subgraph_ordinal);
                subgraph_ordinal += 1;
            }
        }
    }
    let mut g = Graph {
        subgraph_index,
        ..Graph::default()
    };
    let mut seen_header = false;
    let mut warned_no_header = false;

    for (ix, raw_line) in src.lines().enumerate() {
        let line_no = ix + 1;
        for stmt in scanner::statements(raw_line) {
            let stmt = stmt.trim();
            if stmt.is_empty() {
                continue;
            }
            let keyword = stmt.split_whitespace().next().unwrap_or("");

            if matches!(
                scanner::classify_non_edge_keyword(keyword),
                Some(scanner::NonEdgeKeyword::Header)
            ) {
                if seen_header {
                    g.warnings.push(Warning {
                        line: line_no,
                        msg: format!("duplicate `{keyword}` header ignored"),
                    });
                    continue;
                }
                seen_header = true;
                let dir_token = stmt[keyword.len()..].trim();
                if dir_token.is_empty() {
                    g.warnings.push(Warning {
                        line: line_no,
                        msg: "no direction after header; assuming TB".to_string(),
                    });
                } else {
                    match Dir::from_token(dir_token) {
                        Some(d) => g.dir = Some(d),
                        None => {
                            g.warnings.push(Warning {
                                line: line_no,
                                msg: format!(
                                    "unknown direction `{dir_token}` (expected LR, RL, TB or BT); assuming TB"
                                ),
                            });
                        }
                    }
                }
                continue;
            }

            if matches!(
                scanner::classify_non_edge_keyword(keyword),
                Some(scanner::NonEdgeKeyword::Subgraph)
            ) {
                if !seen_header && !warned_no_header {
                    warned_no_header = true;
                    g.warnings.push(Warning {
                        line: line_no,
                        msg: "missing `flowchart <DIR>` header; assuming `flowchart TB`"
                            .to_string(),
                    });
                }
                let rest = stmt["subgraph".len()..].trim();
                let (id, raw_title) = parse_subgraph_header(rest);
                let title = clean_flowchart_label(&raw_title, line_no)?;
                g.open_subgraph(id, title);
                continue;
            }

            if matches!(
                scanner::classify_non_edge_keyword(keyword),
                Some(scanner::NonEdgeKeyword::End)
            ) {
                g.close_subgraph(line_no);
                continue;
            }

            if matches!(
                scanner::classify_non_edge_keyword(keyword),
                Some(scanner::NonEdgeKeyword::IgnoredDirective)
            ) {
                g.warnings.push(Warning {
                    line: line_no,
                    msg: format!("`{keyword}`: directive ignored"),
                });
                continue;
            }

            if !seen_header && !warned_no_header {
                warned_no_header = true;
                g.warnings.push(Warning {
                    line: line_no,
                    msg: "missing `flowchart <DIR>` header; assuming `flowchart TB`".to_string(),
                });
            }

            parse_statement(&mut g, stmt, line_no)?;
        }
    }

    if !g.sg_stack.is_empty() {
        g.warnings.push(Warning {
            line: src.lines().count().max(1),
            msg: format!("{} unclosed subgraph(s) at end of input", g.sg_stack.len()),
        });
        g.sg_stack.clear();
    }

    g.resolve_edge_proxies()?;

    Ok(g)
}

/// Reject source controls before any parser can copy them into terminal text.
///
/// Newlines and conventional CRLF line endings are syntax. Every other
/// control scalar is rejected with its original source line so audit mode,
/// normal rendering, and parser diagnostics share the same safety boundary.
/// In particular, a tab must be replaced with spaces instead of reaching a
/// label, while a bare carriage return cannot move the terminal cursor.
pub(crate) fn validate_terminal_source(src: &str) -> Result<(), ParseError> {
    let mut line = 1;
    let mut chars = src.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\n' => line += 1,
            '\r' if chars.peek() == Some(&'\n') => {}
            '\t' => {
                return Err(ParseError {
                    line,
                    msg: "tab control is not supported; use spaces, not tabs".to_string(),
                });
            }
            ch if ch.is_control() => {
                return Err(ParseError {
                    line,
                    msg: format!(
                        "terminal control U+{:04X} is not supported; remove it or use visible text",
                        ch as u32
                    ),
                });
            }
            _ => {}
        }
    }

    Ok(())
}

/// Validate text after Mermaid syntax delimiters have been removed.
///
/// Combining marks and joiners are valid when they belong to a visible
/// extended grapheme. A standalone grapheme that occupies no terminal column
/// cannot be represented or checked on the canvas, so report it as a
/// source-level input error instead of allowing a later render invariant
/// failure. Doing this on parsed labels avoids mistaking visible punctuation
/// such as `.` plus a combining mark for Mermaid syntax.
pub(crate) fn validate_terminal_text(text: &str, line: usize) -> Result<(), ParseError> {
    if text
        .split('\n')
        .flat_map(|row| row.graphemes(true))
        .any(|grapheme| grapheme.width() == 0)
    {
        return Err(ParseError {
            line,
            msg: "zero-column Unicode grapheme is not supported; attach combining marks to a visible base character or remove invisible formatting".to_string(),
        });
    }
    Ok(())
}

/// Parse `subgraph` header rest into `(id, title)`.
/// Forms: `id`, `id [Title]`, `"Title"`, `id "Title"`.
fn parse_subgraph_header(rest: &str) -> (String, String) {
    let rest = rest.trim();
    if rest.is_empty() {
        return ("sg".into(), "sg".into());
    }
    // Quoted title only: subgraph "My Group"
    if let Some(quoted) = rest.strip_prefix('"')
        && let Some(end) = first_unescaped_quote(quoted)
    {
        let title = quoted[..end].to_string();
        let id = title
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect::<String>();
        let id = if id.is_empty() { "sg".into() } else { id };
        return (id, title);
    }
    let mut parts = rest.splitn(2, char::is_whitespace);
    let id = parts.next().unwrap_or("sg").to_string();
    let rem = parts.next().unwrap_or("").trim();
    if rem.is_empty() {
        return (id.clone(), id);
    }
    // [Title] or "Title"
    if rem.starts_with('[') && rem.ends_with(']') && rem.len() >= 2 {
        return (id, rem[1..rem.len() - 1].to_string());
    }
    if rem.starts_with('"') && rem.ends_with('"') && rem.len() >= 2 {
        return (id, rem[1..rem.len() - 1].to_string());
    }
    (id, rem.to_string())
}

fn first_unescaped_quote(text: &str) -> Option<usize> {
    text.char_indices()
        .find(|&(index, ch)| ch == '"' && !is_escaped_quote(text, index))
        .map(|(index, _)| index)
}

fn is_escaped_quote(text: &str, quote: usize) -> bool {
    text.as_bytes()[..quote]
        .iter()
        .rev()
        .take_while(|&&byte| byte == b'\\')
        .count()
        % 2
        == 1
}

/// One statement: `group (edge group)*` where `group = node (& node)*`.
fn parse_statement(g: &mut Graph, stmt: &str, line: usize) -> Result<(), ParseError> {
    let mut cur = Cur::new(stmt);
    let mut prev = parse_group(g, &mut cur, line)?;

    loop {
        cur.skip_ws();
        if cur.eof() {
            return Ok(());
        }
        let (kind, arrow, mut label) = parse_edge_op(&mut cur, line)?;
        cur.skip_ws();
        if cur.eat('|') {
            let text = cur.take_until_str("|").ok_or_else(|| ParseError {
                line,
                msg: "unterminated edge label: expected closing `|`".to_string(),
            })?;
            label = Some(clean_flowchart_label(&text, line)?);
        }
        cur.skip_ws();
        let next = parse_group(g, &mut cur, line)?;
        for &source in &prev {
            for &target in &next {
                g.edges.push(Edge {
                    // Resolve group endpoints after all subgraph members are
                    // known. These placeholders never leave parsing.
                    from: 0,
                    to: 0,
                    source,
                    target,
                    kind,
                    arrow,
                    label: label.clone(),
                    endpoint_reserve: 0,
                    distinct_endpoints: false,
                    line,
                });
            }
        }
        prev = next;
    }
}

fn parse_group(g: &mut Graph, cur: &mut Cur, line: usize) -> Result<Vec<Endpoint>, ParseError> {
    let mut ids = vec![parse_endpoint(g, cur, line)?];
    loop {
        cur.skip_ws();
        if cur.eat('&') {
            cur.skip_ws();
            ids.push(parse_endpoint(g, cur, line)?);
        } else {
            return Ok(ids);
        }
    }
}

const SHAPES: &[(&str, &str, Shape)] = &[
    ("([", "])", Shape::Stadium),
    ("[(", ")]", Shape::Cylinder),
    ("((", "))", Shape::Circle),
    ("{{", "}}", Shape::Hexagon),
    ("[", "]", Shape::Rect),
    ("(", ")", Shape::Rounded),
    ("{", "}", Shape::Diamond),
];

fn parse_endpoint(g: &mut Graph, cur: &mut Cur, line: usize) -> Result<Endpoint, ParseError> {
    let id = cur.take_while(|c| c.is_alphanumeric() || c == '_');
    if id.is_empty() {
        return Err(ParseError {
            line,
            msg: format!("expected a node id, found `{}`", cur.rest_snippet()),
        });
    }
    for &(open, close, shape) in SHAPES {
        if cur.starts_with(open) {
            cur.advance(open.len());
            let raw = cur.take_until_str(close).ok_or_else(|| ParseError {
                line,
                msg: format!("node `{id}`: unterminated `{open}` — expected closing `{close}`"),
            })?;
            return Ok(Endpoint::Node(g.add_node(
                &id,
                Some((shape, clean_flowchart_label(&raw, line)?)),
                line,
            )));
        }
    }
    if let Some(&subgraph) = g.subgraph_index.get(&id) {
        return Ok(Endpoint::Subgraph(subgraph));
    }
    Ok(Endpoint::Node(g.add_node(&id, None, line)))
}

/// Edge operators, including inline-text forms:
///   solid   -->  ---          -- text -->   -- text ---
///   dotted  -.-> -.-          -. text .->   -. text .-
///   thick   ==>  ===          == text ==>   == text ===
fn parse_edge_op(
    cur: &mut Cur,
    line: usize,
) -> Result<(EdgeKind, bool, Option<String>), ParseError> {
    if cur.starts_with("-.") {
        cur.advance(2);
        if cur.peek() == Some('-') || cur.peek() == Some('>') {
            cur.take_while(|c| c == '-' || c == '.');
            let arrow = cur.eat('>');
            return Ok((EdgeKind::Dotted, arrow, None));
        }
        let text = cur
            .take_until_str(".-")
            .ok_or_else(|| unterminated_edge(line, "-.", ".->"))?;
        cur.take_while(|c| c == '-' || c == '.');
        let arrow = cur.eat('>');
        return Ok((
            EdgeKind::Dotted,
            arrow,
            Some(clean_flowchart_label(&text, line)?),
        ));
    }

    if cur.starts_with("==") {
        let eqs = cur.take_while(|c| c == '=');
        if cur.eat('>') {
            return Ok((EdgeKind::Thick, true, None));
        }
        if eqs.len() >= 3 {
            return Ok((EdgeKind::Thick, false, None));
        }
        let text = cur
            .take_until_str("==")
            .ok_or_else(|| unterminated_edge(line, "==", "==>"))?;
        cur.take_while(|c| c == '=');
        let arrow = cur.eat('>');
        return Ok((
            EdgeKind::Thick,
            arrow,
            Some(clean_flowchart_label(&text, line)?),
        ));
    }

    if cur.starts_with("--") {
        let dashes = cur.take_while(|c| c == '-');
        if cur.eat('>') {
            return Ok((EdgeKind::Solid, true, None));
        }
        if dashes.len() >= 3 {
            return Ok((EdgeKind::Solid, false, None));
        }
        let text = cur
            .take_until_str("--")
            .ok_or_else(|| unterminated_edge(line, "--", "-->"))?;
        cur.take_while(|c| c == '-');
        let arrow = cur.eat('>');
        return Ok((
            EdgeKind::Solid,
            arrow,
            Some(clean_flowchart_label(&text, line)?),
        ));
    }

    Err(ParseError {
        line,
        msg: format!(
            "expected an edge such as `-->`, `---`, `-.->` or `==>`, found `{}`",
            cur.rest_snippet()
        ),
    })
}

fn unterminated_edge(line: usize, open: &str, example: &str) -> ParseError {
    ParseError {
        line,
        msg: format!("unterminated `{open} text ...` edge — expected a closing `{example}`"),
    }
}

/// Normalize terminal label text shared by the engines. Literal br tags are
/// the only formatting tags interpreted; Markdown and other HTML-like tags
/// remain visible text.
fn clean_label(raw: &str) -> String {
    let s = raw.trim();
    let s = s
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(s);
    // B1: <br/> (and <br>, <br />, any case) is a line break inside the label.
    let ch: Vec<char> = s.trim().chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < ch.len() {
        if let Some(end) = br_tag_end(&ch, i) {
            out.push('\n');
            i = end;
        } else {
            out.push(ch[i]);
            i += 1;
        }
    }
    out.lines().map(str::trim).collect::<Vec<_>>().join("\n")
}

pub(crate) fn clean_terminal_label(raw: &str, line: usize) -> Result<String, ParseError> {
    let label = clean_label(raw);
    validate_terminal_text(&label, line)?;
    Ok(label)
}

fn clean_flowchart_label(raw: &str, line: usize) -> Result<String, ParseError> {
    let normalized = normalize_flowchart_label(raw);
    if let Some(control) = normalized.decoded_control {
        return Err(ParseError {
            line,
            msg: format!(
                "decoded terminal control U+{:04X} is not supported; remove the entity or use visible text",
                control as u32
            ),
        });
    }
    validate_terminal_text(&normalized.text, line)?;
    Ok(normalized.text)
}

/// Flowchart-only character-reference compatibility runs after shared text
/// cleanup. An entity-escaped br tag therefore remains literal text rather
/// than becoming formatting syntax.
fn normalize_flowchart_label(raw: &str) -> NormalizedLabel {
    decode_entities(&clean_label(raw))
}

struct NormalizedLabel {
    text: String,
    decoded_control: Option<char>,
}

/// Decode the small, deterministic entity subset accepted in flowchart labels.
///
/// The XML named references plus nbsp, Tab, and NewLine cover common Mermaid
/// text. Valid decimal and hexadecimal scalar references also decode. Unknown
/// names, malformed numeric values, surrogates, and out-of-range scalars are
/// retained byte-for-byte; a typo must never turn the rest of a label into
/// parser syntax.
fn decode_entities(text: &str) -> NormalizedLabel {
    let mut output = String::with_capacity(text.len());
    let mut decoded_control = None;
    let mut index = 0;

    while index < text.len() {
        let rest = &text[index..];
        if matches!(rest.chars().next(), Some('&' | '#'))
            && let Some(length) = scanner::entity_reference_len(rest)
        {
            let entity = &rest[..length];
            if let Some(decoded) = decode_entity(entity) {
                if decoded.is_control() {
                    decoded_control.get_or_insert(decoded);
                }
                output.push(decoded);
            } else {
                output.push_str(entity);
            }
            index += length;
            continue;
        }

        let ch = rest
            .chars()
            .next()
            .expect("index always advances across valid UTF-8");
        output.push(ch);
        index += ch.len_utf8();
    }

    NormalizedLabel {
        text: output,
        decoded_control,
    }
}

fn decode_entity(entity: &str) -> Option<char> {
    let body = entity.strip_suffix(';')?;
    let spelling = body.strip_prefix('&').or_else(|| body.strip_prefix('#'))?;
    let named = match spelling {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{a0}'),
        "Tab" => Some('\t'),
        "NewLine" => Some('\n'),
        _ => None,
    };
    let numeric = body.strip_prefix('&').unwrap_or(body);
    named.or_else(|| decode_numeric_entity(numeric))
}

fn decode_numeric_entity(body: &str) -> Option<char> {
    let digits = body
        .strip_prefix("#x")
        .or_else(|| body.strip_prefix("#X"))
        .map(|digits| (digits, 16))
        .or_else(|| body.strip_prefix('#').map(|digits| (digits, 10)))?;
    (!digits.0.is_empty())
        .then(|| u32::from_str_radix(digits.0, digits.1).ok())
        .flatten()
        .and_then(char::from_u32)
}

/// If a `<br>` / `<br/>` / `<br />` tag starts at `i`, return the index past it.
fn br_tag_end(ch: &[char], i: usize) -> Option<usize> {
    if ch.get(i) != Some(&'<') {
        return None;
    }
    let mut j = i + 1;
    if !matches!(ch.get(j), Some('b' | 'B')) {
        return None;
    }
    j += 1;
    if !matches!(ch.get(j), Some('r' | 'R')) {
        return None;
    }
    j += 1;
    while ch.get(j) == Some(&' ') {
        j += 1;
    }
    if ch.get(j) == Some(&'/') {
        j += 1;
    }
    (ch.get(j) == Some(&'>')).then_some(j + 1)
}

/// Stable, human-readable dump of the IR including warnings — the
/// golden-snapshot format for parser tests.
pub fn dump(g: &Graph) -> String {
    let mut out = String::new();
    out.push_str(&format!("direction: {}\n", g.direction().name()));
    out.push_str("nodes:\n");
    for n in &g.nodes {
        out.push_str(&format!(
            "  {} {} \"{}\"\n",
            n.id,
            n.shape.name(),
            dump_text(&n.label)
        ));
    }
    if !g.subgraphs.is_empty() {
        out.push_str("subgraphs:\n");
        for (i, sg) in g.subgraphs.iter().enumerate() {
            let members: Vec<&str> = sg
                .members
                .iter()
                .map(|&ni| g.nodes[ni].id.as_str())
                .collect();
            let parent = sg.parent.map(|p| g.subgraphs[p].id.as_str()).unwrap_or("-");
            out.push_str(&format!(
                "  {} id={} title=\"{}\" parent={} members=[{}]\n",
                i,
                sg.id,
                dump_text(&sg.title),
                parent,
                members.join(",")
            ));
        }
    }
    out.push_str("edges:\n");
    for e in &g.edges {
        let label = e
            .label
            .as_ref()
            .map(|label| format!("|{}|", dump_text(label)))
            .unwrap_or_default();
        out.push_str(&format!(
            "  {} {}{} {}\n",
            endpoint_id(g, e.source),
            e.op(),
            label,
            endpoint_id(g, e.target)
        ));
    }
    if !g.warnings.is_empty() {
        out.push_str("warnings:\n");
        for w in &g.warnings {
            out.push_str(&format!("  line {}: {}\n", w.line, w.msg));
        }
    }
    out
}

fn endpoint_id(g: &Graph, endpoint: Endpoint) -> &str {
    match endpoint {
        Endpoint::Node(node) => g.nodes[node].id.as_str(),
        Endpoint::Subgraph(group) => g.subgraphs[group].id.as_str(),
    }
}

fn dump_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// Character cursor over a single statement.
struct Cur {
    ch: Vec<char>,
    i: usize,
}

impl Cur {
    fn new(s: &str) -> Cur {
        Cur {
            ch: s.chars().collect(),
            i: 0,
        }
    }

    fn eof(&self) -> bool {
        self.i >= self.ch.len()
    }

    fn peek(&self) -> Option<char> {
        self.ch.get(self.i).copied()
    }

    fn advance(&mut self, n: usize) {
        self.i = (self.i + n).min(self.ch.len());
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(|c| c.is_whitespace()) {
            self.i += 1;
        }
    }

    fn starts_with(&self, s: &str) -> bool {
        s.chars()
            .enumerate()
            .all(|(k, c)| self.ch.get(self.i + k) == Some(&c))
    }

    fn take_while(&mut self, pred: impl Fn(char) -> bool) -> String {
        let start = self.i;
        while self.peek().is_some_and(&pred) {
            self.i += 1;
        }
        self.ch[start..self.i].iter().collect()
    }

    /// Consume up to (and including) `close`; return the text before it.
    fn take_until_str(&mut self, close: &str) -> Option<String> {
        let mut k = self.i;
        let mut quoted = false;
        while k < self.ch.len() {
            if self.ch[k] == '"' && !self.quote_is_escaped(k) {
                quoted = !quoted;
                k += 1;
                continue;
            }
            let matches = !quoted
                && close
                    .chars()
                    .enumerate()
                    .all(|(j, c)| self.ch.get(k + j) == Some(&c));
            if matches {
                let text: String = self.ch[self.i..k].iter().collect();
                self.i = k + close.chars().count();
                return Some(text);
            }
            k += 1;
        }
        None
    }

    fn quote_is_escaped(&self, quote: usize) -> bool {
        self.ch[..quote]
            .iter()
            .rev()
            .take_while(|&&ch| ch == '\\')
            .count()
            % 2
            == 1
    }

    fn rest_snippet(&self) -> String {
        let rest: String = self.ch[self.i..].iter().collect();
        if rest.is_empty() {
            "end of line".to_string()
        } else if rest.chars().count() > 20 {
            let cut: String = rest.chars().take(20).collect();
            format!("{cut}…")
        } else {
            rest
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_decode_core_entities_once_and_preserve_unknown_references() {
        assert_eq!(
            clean_flowchart_label(
                r#"AT&amp;T #quot; #65; #x41; &#66; &lt;br&gt; &unknown-name; #xzz; &nbsp;"#,
                1
            )
            .unwrap(),
            "AT&T \" A A B <br> &unknown-name; #xzz; \u{a0}"
        );
    }

    #[test]
    fn dump_escapes_flowchart_node_group_and_edge_text() {
        let graph = Graph {
            dir: Some(Dir::LR),
            nodes: vec![
                Node {
                    id: "A".into(),
                    label: "node\\\"\n".into(),
                    shape: Shape::Rect,
                },
                Node {
                    id: "B".into(),
                    label: "B".into(),
                    shape: Shape::Rect,
                },
            ],
            edges: vec![Edge {
                from: 0,
                to: 1,
                source: Endpoint::Node(0),
                target: Endpoint::Node(1),
                kind: EdgeKind::Solid,
                arrow: true,
                label: Some("edge\\\"\n".into()),
                endpoint_reserve: 0,
                distinct_endpoints: false,
                line: 1,
            }],
            subgraphs: vec![Subgraph {
                id: "Group".into(),
                title: "group\\\"\n".into(),
                parent: None,
                members: vec![0],
            }],
            warnings: Vec::new(),
            index: HashMap::new(),
            subgraph_index: HashMap::new(),
            sg_stack: Vec::new(),
        };

        let dumped = dump(&graph);
        assert!(
            dumped.contains(&format!("  A rect \"{}\"\n", r#"node\\\"\n"#)),
            "{dumped}"
        );
        assert!(
            dumped.contains(&format!(r#"title="{}""#, r#"group\\\"\n"#)),
            "{dumped}"
        );
        assert!(
            dumped.contains(&format!("-->|{}|", r#"edge\\\"\n"#)),
            "{dumped}"
        );
    }

    #[test]
    fn duplicate_subgraph_ids_do_not_shift_later_forward_endpoint_ids() {
        let graph = parse(
            "flowchart LR\n\
             S --> b\n\
             subgraph a\nA\nend\n\
             subgraph a\nA2\nend\n\
             subgraph b\nB\nend\n",
        )
        .unwrap();

        assert_eq!(graph.subgraphs[2].id, "b");
        assert!(matches!(graph.edges[0].target, Endpoint::Subgraph(2)));
        assert!(graph.nodes.iter().all(|node| node.id != "b"));
    }

    #[test]
    fn decoded_entity_controls_are_rejected_before_rendering() {
        for (entity, control) in [
            ("#0;", "U+0000"),
            ("#x1b;", "U+001B"),
            ("&Tab;", "U+0009"),
            ("&NewLine;", "U+000A"),
        ] {
            let error = clean_flowchart_label(entity, 7).unwrap_err();
            assert_eq!(error.line, 7, "{entity}: {error}");
            assert!(error.msg.contains(control), "{entity}: {error}");
        }
    }

    #[test]
    fn decoded_zero_column_scalars_are_rejected_before_rendering() {
        let error = clean_flowchart_label("#x301;", 9).unwrap_err();
        assert_eq!(error.line, 9);
        assert!(
            error.msg.contains("zero-column Unicode grapheme"),
            "{error}"
        );
    }

    #[test]
    fn quoted_shape_and_edge_closers_are_not_terminators() {
        let graph = parse(r#"flowchart LR; A["cache[key]"] --> B("call(foo)")"#).unwrap();
        assert_eq!(graph.nodes[0].label, "cache[key]");
        assert_eq!(graph.nodes[1].label, "call(foo)");

        let graph = parse(r#"flowchart LR; A -->|"left | right"| B"#).unwrap();
        assert_eq!(graph.edges[0].label.as_deref(), Some("left | right"));
    }

    #[test]
    fn inline_edge_labels_decode_entities_for_every_supported_style() {
        for (source, label) in [
            ("flowchart LR; A -- AT&amp;T --> B", "AT&T"),
            ("flowchart LR; A -. AT&amp;T .-> B", "AT&T"),
            ("flowchart LR; A == AT&amp;T ==> B", "AT&T"),
        ] {
            let graph = parse(source).unwrap();
            assert_eq!(graph.edges[0].label.as_deref(), Some(label), "{source}");
        }
    }

    #[test]
    fn inline_edge_labels_keep_plain_boundaries_until_their_closers() {
        for source in [
            "flowchart LR; A -- plain; %% literal --> B; C --> D",
            "flowchart LR; A -. plain; %% literal .-> B; C --> D",
            "flowchart LR; A == plain; %% literal ==> B; C --> D",
        ] {
            let graph = parse(source).unwrap();
            assert_eq!(graph.edges.len(), 2, "{source}");
            assert_eq!(
                graph.edges[0].label.as_deref(),
                Some("plain; %% literal"),
                "{source}"
            );
            assert_eq!(graph.nodes[2].id, "C", "{source}");
            assert_eq!(graph.nodes[3].id, "D", "{source}");
        }
    }

    #[test]
    fn directives_and_subgraphs_do_not_open_inline_labels() {
        let graph = parse(
            "flowchart LR; title [release]--plan; A --> B; title A --> B -- plan; C --> D; \
             click A href -- plan; E --> F; style A fill:#f00 -- plan; G --> H",
        )
        .unwrap();
        let ids: Vec<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
        assert_eq!(ids, ["A", "B", "C", "D", "E", "F", "G", "H"]);
        assert_eq!(graph.edges.len(), 4);
        assert_eq!(graph.warnings.len(), 4);

        let graph = parse("flowchart LR; subgraph G [release]--plan; A; end; C").unwrap();
        assert_eq!(graph.subgraphs.len(), 1);
        assert_eq!(graph.subgraphs[0].title, "[release]--plan");
        assert_eq!(graph.subgraphs[0].members.len(), 1);
        assert_eq!(graph.nodes[0].id, "A");
        assert_eq!(graph.nodes[1].id, "C");

        let graph = parse("flowchart LR; titleNode[release] -- plan; %% text --> B; C").unwrap();
        assert_eq!(graph.nodes[0].id, "titleNode");
        assert_eq!(graph.edges[0].label.as_deref(), Some("plan; %% text"));
        assert_eq!(graph.nodes[2].id, "C");
    }

    #[test]
    fn dotted_and_other_predecessor_edges_enable_following_inline_labels() {
        for (source, kind, arrow) in [
            (
                "flowchart LR; A -.-> B -- label; %% text --> C; C --> D",
                EdgeKind::Dotted,
                true,
            ),
            (
                "flowchart LR; A -.- B -- label; %% text --> C; C --> D",
                EdgeKind::Dotted,
                false,
            ),
            (
                "flowchart LR; A --> B -- label; %% text --> C; C --> D",
                EdgeKind::Solid,
                true,
            ),
            (
                "flowchart LR; A ==> B -- label; %% text --> C; C --> D",
                EdgeKind::Thick,
                true,
            ),
            (
                "flowchart LR; A -->|edge| B -- label; %% text --> C; C --> D",
                EdgeKind::Solid,
                true,
            ),
        ] {
            let graph = parse(source).unwrap();
            assert_eq!(graph.edges.len(), 3, "{source}");
            assert_eq!(graph.edges[0].kind, kind, "{source}");
            assert_eq!(graph.edges[0].arrow, arrow, "{source}");
            assert_eq!(
                graph.edges[1].label.as_deref(),
                Some("label; %% text"),
                "{source}"
            );
        }
    }

    #[test]
    fn unicode_node_ids_support_grouped_and_chained_inline_labels() {
        let graph = parse("flowchart LR; 源 --> 目 -- chained; %% text --> 終; 終 --> 次").unwrap();
        assert_eq!(graph.nodes[0].id, "源");
        assert_eq!(graph.nodes[1].id, "目");
        assert_eq!(graph.nodes[2].id, "終");
        assert_eq!(graph.nodes[3].id, "次");
        assert_eq!(graph.edges[1].label.as_deref(), Some("chained; %% text"));

        let graph = parse("flowchart LR; α & β -- grouped; %% text --> γ; γ --> δ").unwrap();
        assert_eq!(graph.edges.len(), 3);
        assert_eq!(graph.edges[0].label.as_deref(), Some("grouped; %% text"));
        assert_eq!(graph.edges[1].label.as_deref(), Some("grouped; %% text"));
        assert_eq!(graph.nodes[2].id, "γ");
        assert_eq!(graph.nodes[3].id, "δ");
    }

    #[test]
    fn unterminated_inline_edges_keep_a_parse_diagnostic() {
        let error = parse("flowchart LR; A -- plain; %% literal; C").unwrap_err();
        assert_eq!(error.line, 1);
        assert!(error.msg.contains("unterminated"), "{error}");
    }

    #[test]
    fn arrowless_inline_closers_and_long_runs_keep_following_statements() {
        for (source, kind) in [
            (
                "flowchart LR; A -- AT&amp;T --- B; C --> D",
                EdgeKind::Solid,
            ),
            (
                "flowchart LR; A -. AT&amp;T .- B; C --> D",
                EdgeKind::Dotted,
            ),
            (
                "flowchart LR; A == AT&amp;T === B; C --> D",
                EdgeKind::Thick,
            ),
            (
                "flowchart LR; A -- AT&amp;T ----> B; C --> D",
                EdgeKind::Solid,
            ),
            (
                "flowchart LR; A == AT&amp;T =====> B; C --> D",
                EdgeKind::Thick,
            ),
        ] {
            let graph = parse(source).unwrap();
            assert_eq!(graph.edges.len(), 2, "{source}");
            assert_eq!(graph.edges[0].kind, kind, "{source}");
            assert_eq!(graph.edges[0].label.as_deref(), Some("AT&T"), "{source}");
            assert_eq!(graph.nodes[2].id, "C", "{source}");
            assert_eq!(graph.nodes[3].id, "D", "{source}");
        }
    }

    #[test]
    fn flowchart_subgraph_titles_decode_entities() {
        let graph = parse("flowchart LR; subgraph Group [AT&amp;T #35;]; A; end").unwrap();
        assert_eq!(graph.subgraphs[0].title, "AT&T #");

        let graph = parse("flowchart LR; subgraph AT&amp;T; A; end").unwrap();
        assert_eq!(graph.subgraphs[0].title, "AT&T");
        assert_eq!(graph.subgraphs[0].members.len(), 1);

        for entity in [
            "&unknown_name;",
            "&unknown.name;",
            "&unknown:name;",
            "&unknown+name;",
        ] {
            let graph = parse(&format!("flowchart LR; subgraph G AT{entity}X; A; end")).unwrap();
            assert_eq!(graph.subgraphs[0].title, format!("AT{entity}X"));
            assert_eq!(graph.subgraphs[0].members.len(), 1);
        }
    }

    #[test]
    fn malformed_ascii_entity_tokens_stay_literal_in_inline_labels() {
        for entity in [
            "&unknown_name;",
            "&unknown.name;",
            "&unknown:name;",
            "&unknown+name;",
        ] {
            let graph = parse(&format!("flowchart LR; A -- {entity} --> B; C")).unwrap();
            assert_eq!(graph.edges[0].label.as_deref(), Some(entity));
            assert_eq!(graph.nodes[2].id, "C");
        }
    }

    #[test]
    fn quote_only_subgraph_headers_honor_escaped_quotes() {
        let graph = parse(r#"flowchart LR; subgraph "release \"plan\""; A; end"#).unwrap();
        assert_eq!(graph.subgraphs[0].title, r#"release \"plan\""#);

        let graph = parse(r#"flowchart LR; subgraph "release \\"; A; end"#).unwrap();
        assert_eq!(graph.subgraphs[0].title, r#"release \\"#);
    }

    #[test]
    fn semicolon_scanning_keeps_unquoted_fanout_as_flowchart_syntax() {
        let graph = parse("flowchart LR; A&B; C").unwrap();
        let ids: Vec<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
        assert_eq!(ids, ["A", "B", "C"]);
    }
}
