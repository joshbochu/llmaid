//! Mermaid flowchart subset -> IR.
//!
//! Forgiving by design: unknown directives (classDef, style, subgraph, ...)
//! become warnings, not errors. Errors carry the line number and what was
//! expected, so an agent can self-correct in one retry.

use std::collections::HashMap;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Rect,     // [x]
    Rounded,  // (x)
    Stadium,  // ([x])
    Circle,   // ((x))
    Cylinder, // [(x)]
    Diamond,  // {x}
    Hexagon,  // {{x}}
}

impl Shape {
    pub fn name(self) -> &'static str {
        match self {
            Shape::Rect => "rect",
            Shape::Rounded => "rounded",
            Shape::Stadium => "stadium",
            Shape::Circle => "circle",
            Shape::Cylinder => "cylinder",
            Shape::Diamond => "diamond",
            Shape::Hexagon => "hexagon",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    Solid,  // --> / ---
    Dotted, // -.-> / -.-
    Thick,  // ==> / ===
}

#[derive(Debug)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub shape: Shape,
}

#[derive(Debug)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub kind: EdgeKind,
    pub arrow: bool,
    pub label: Option<String>,
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
    pub warnings: Vec<Warning>,
    index: HashMap<String, usize>,
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
        self.index.insert(id.to_string(), self.nodes.len() - 1);
        self.nodes.len() - 1
    }
}

/// Directive keywords we recognize and deliberately ignore (with a warning).
const IGNORED_DIRECTIVES: &[&str] = &[
    "classDef",
    "class",
    "style",
    "linkStyle",
    "click",
    "subgraph",
    "direction",
    "accTitle",
    "accDescr",
    "title",
];

pub fn parse(src: &str) -> Result<Graph, ParseError> {
    let mut g = Graph::default();
    let mut seen_header = false;
    let mut warned_no_header = false;

    for (ix, raw_line) in src.lines().enumerate() {
        let line_no = ix + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        for stmt in line.split(';') {
            let stmt = stmt.trim();
            if stmt.is_empty() {
                continue;
            }
            let keyword = stmt.split_whitespace().next().unwrap_or("");

            if keyword == "flowchart" || keyword == "graph" {
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

            if keyword == "end" {
                // Closes a subgraph we already warned about; nothing to do.
                continue;
            }

            if IGNORED_DIRECTIVES.contains(&keyword) {
                let detail = if keyword == "subgraph" {
                    "subgraph ignored; its contents are parsed at top level"
                } else {
                    "directive ignored"
                };
                g.warnings.push(Warning {
                    line: line_no,
                    msg: format!("`{keyword}`: {detail}"),
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

    Ok(g)
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
            label = Some(clean_label(&text));
        }
        cur.skip_ws();
        let next = parse_group(g, &mut cur, line)?;
        for &from in &prev {
            for &to in &next {
                g.edges.push(Edge {
                    from,
                    to,
                    kind,
                    arrow,
                    label: label.clone(),
                });
            }
        }
        prev = next;
    }
}

fn parse_group(g: &mut Graph, cur: &mut Cur, line: usize) -> Result<Vec<usize>, ParseError> {
    let mut ids = vec![parse_node(g, cur, line)?];
    loop {
        cur.skip_ws();
        if cur.eat('&') {
            cur.skip_ws();
            ids.push(parse_node(g, cur, line)?);
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

fn parse_node(g: &mut Graph, cur: &mut Cur, line: usize) -> Result<usize, ParseError> {
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
            return Ok(g.add_node(&id, Some((shape, clean_label(&raw))), line));
        }
    }
    Ok(g.add_node(&id, None, line))
}

/// Edge operators, including inline-text forms:
///   solid   -->  ---          -- text -->   -- text ---
///   dotted  -.-> -.-          -. text .->   -. text .-
///   thick   ==>  ===          == text ==>   == text ===
fn parse_edge_op(cur: &mut Cur, line: usize) -> Result<(EdgeKind, bool, Option<String>), ParseError> {
    if cur.starts_with("-.") {
        cur.advance(2);
        if cur.peek() == Some('-') || cur.peek() == Some('>') {
            cur.take_while(|c| c == '-' || c == '.');
            let arrow = cur.eat('>');
            return Ok((EdgeKind::Dotted, arrow, None));
        }
        let text = cur.take_until_str(".-").ok_or_else(|| unterminated_edge(line, "-.", ".->"))?;
        cur.take_while(|c| c == '-' || c == '.');
        let arrow = cur.eat('>');
        return Ok((EdgeKind::Dotted, arrow, Some(clean_label(&text))));
    }

    if cur.starts_with("==") {
        let eqs = cur.take_while(|c| c == '=');
        if cur.eat('>') {
            return Ok((EdgeKind::Thick, true, None));
        }
        if eqs.len() >= 3 {
            return Ok((EdgeKind::Thick, false, None));
        }
        let text = cur.take_until_str("==").ok_or_else(|| unterminated_edge(line, "==", "==>"))?;
        cur.take_while(|c| c == '=');
        let arrow = cur.eat('>');
        return Ok((EdgeKind::Thick, arrow, Some(clean_label(&text))));
    }

    if cur.starts_with("--") {
        let dashes = cur.take_while(|c| c == '-');
        if cur.eat('>') {
            return Ok((EdgeKind::Solid, true, None));
        }
        if dashes.len() >= 3 {
            return Ok((EdgeKind::Solid, false, None));
        }
        let text = cur.take_until_str("--").ok_or_else(|| unterminated_edge(line, "--", "-->"))?;
        cur.take_while(|c| c == '-');
        let arrow = cur.eat('>');
        return Ok((EdgeKind::Solid, arrow, Some(clean_label(&text))));
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
    out.lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n")
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
    dump_impl(g, true)
}

/// IR dump without the warnings section — the placeholder CLI stdout until
/// rendering lands (M2). Warnings belong on stderr only (B6).
pub fn dump_diagram(g: &Graph) -> String {
    dump_impl(g, false)
}

fn dump_impl(g: &Graph, include_warnings: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!("direction: {}\n", g.direction().name()));
    out.push_str("nodes:\n");
    for n in &g.nodes {
        out.push_str(&format!(
            "  {} {} \"{}\"\n",
            n.id,
            n.shape.name(),
            n.label.replace('\n', "\\n")
        ));
    }
    out.push_str("edges:\n");
    for e in &g.edges {
        let label = e
            .label
            .as_ref()
            .map(|l| format!("|{l}|"))
            .unwrap_or_default();
        out.push_str(&format!(
            "  {} {}{} {}\n",
            g.nodes[e.from].id,
            e.op(),
            label,
            g.nodes[e.to].id
        ));
    }
    if include_warnings && !g.warnings.is_empty() {
        out.push_str("warnings:\n");
        for w in &g.warnings {
            out.push_str(&format!("  line {}: {}\n", w.line, w.msg));
        }
    }
    out
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
        while k < self.ch.len() {
            let matches = close
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
