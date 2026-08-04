//! Deterministic single-line statement scanning for the flowchart parser.
//!
//! Mermaid permits semicolons between statements, but semicolons also finish
//! HTML character references.  Splitting a raw line therefore loses labels
//! such as `AT&amp;T`.  This intentionally small scanner recognizes only the
//! syntax needed to find statement boundaries: outer label spans (quoted,
//! shaped, pipe-delimited, and inline-edge text), safely-contained
//! entity-shaped references, and `%%` comments outside those spans. The parser
//! remains responsible for validating statement syntax and for decoding the
//! recognized entities in labels.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NonEdgeKeyword {
    Header,
    Subgraph,
    End,
    IgnoredDirective,
}

/// Classify the exact first token for both statement scanning and parsing.
///
/// Node IDs that merely begin with one of these words stay edge candidates.
pub(crate) fn classify_non_edge_keyword(keyword: &str) -> Option<NonEdgeKeyword> {
    match keyword {
        "flowchart" | "graph" => Some(NonEdgeKeyword::Header),
        "subgraph" => Some(NonEdgeKeyword::Subgraph),
        "end" => Some(NonEdgeKeyword::End),
        _ if IGNORED_DIRECTIVES.contains(&keyword) => Some(NonEdgeKeyword::IgnoredDirective),
        _ => None,
    }
}

const IGNORED_DIRECTIVES: &[&str] = &[
    "classDef",
    "class",
    "style",
    "linkStyle",
    "click",
    "direction",
    "accTitle",
    "accDescr",
    "title",
];

/// Split one Mermaid source line into statements, stopping at a trailing
/// `%%` comment when it is outside an outer flowchart label span.
///
/// Returned slices retain their original whitespace so callers can apply their
/// own grammar-specific trimming. A semicolon in a safely-contained entity
/// reference, including named HTML or Mermaid `#name;` text, numeric text
/// beginning with `#`, and malformed ASCII tokens made of letters, digits,
/// `#`, `-`, `_`, `.`, `:`, and `+`, stays in the current label.
/// Unknown entities are deliberately not decoded here: preserving the raw
/// token is safer than treating its remainder as a new Mermaid statement.
pub(crate) fn statements(line: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut protected_entity_semicolon = None;
    let mut suppress_until = 0;
    let mut label_closers = Vec::new();
    let mut edge_label = false;
    let mut inline_edge: Option<InlineEdge> = None;

    for (index, ch) in line.char_indices() {
        if index < suppress_until {
            continue;
        }
        if protected_entity_semicolon == Some(index) {
            protected_entity_semicolon = None;
            continue;
        }

        if ch == '"' && !is_escaped_quote(line, index) {
            quoted = !quoted;
            continue;
        }
        if quoted {
            continue;
        }

        if let Some(edge) = inline_edge {
            if let Some(length) = edge.closing_len(&line[index..]) {
                inline_edge = None;
                suppress_until = index + length;
            } else {
                if matches!(ch, '&' | '#') {
                    protected_entity_semicolon =
                        entity_reference_len(&line[index..]).map(|length| index + length - 1);
                }
                continue;
            }
            continue;
        }

        if !edge_label {
            match ch {
                '[' => label_closers.push(']'),
                '(' => label_closers.push(')'),
                '{' => label_closers.push('}'),
                _ if label_closers.last() == Some(&ch) => {
                    label_closers.pop();
                }
                _ => {}
            }
        }
        if ch == '|' && label_closers.is_empty() {
            edge_label = !edge_label;
            continue;
        }
        if !label_closers.is_empty() || edge_label {
            if matches!(ch, '&' | '#') {
                protected_entity_semicolon =
                    entity_reference_len(&line[index..]).map(|length| index + length - 1);
            }
            continue;
        }
        if let Some(edge) = InlineEdge::opens(&line[index..])
            && has_inline_left_operand(line, start, index)
        {
            inline_edge = Some(edge);
            continue;
        }
        if matches!(ch, '&' | '#') && bare_subgraph_title_prefix(line, start, index) {
            protected_entity_semicolon =
                entity_reference_len(&line[index..]).map(|length| index + length - 1);
        }
        if line[index..].starts_with("%%") {
            result.push(&line[start..index]);
            return result;
        }
        if ch == ';' {
            result.push(&line[start..index]);
            start = index + ch.len_utf8();
        }
    }

    result.push(&line[start..]);
    result
}

/// An unquoted inline label may only follow a syntactically plausible
/// flowchart group. This stops prose such as title release--plan from
/// swallowing later statement boundaries without making label content itself
/// impose artificial semicolon or comment boundaries.
fn has_inline_left_operand(line: &str, statement_start: usize, operator_start: usize) -> bool {
    if classify_non_edge_keyword(statement_keyword(&line[statement_start..])).is_some() {
        return false;
    }

    let prefix = line[statement_start..operator_start].trim_end();
    // `o-- label -->` / `x-- label -->` and `<-- label -->` carry a
    // source-side terminal mark immediately before the ordinary inline edge
    // opener. It is syntax, not part of the previous node ID.
    let prefix = prefix
        .strip_suffix(['o', 'x', '<'])
        // `o` and `x` are also valid one-character node IDs.  A terminal
        // marker exists only after a real source operand, so never consume
        // the entire operand while deciding whether a semicolon is inline
        // label content.
        .filter(|stripped| !stripped.trim_end().is_empty())
        .unwrap_or(prefix)
        .trim_end();
    if prefix.ends_with([']', ')', '}']) {
        return true;
    }

    let node_start = prefix
        .char_indices()
        .rev()
        .find(|&(_, ch)| !is_node_id_char(ch))
        .map(|(index, ch)| index + ch.len_utf8())
        .unwrap_or(0);
    if node_start == prefix.len() {
        return false;
    }

    let before_node = prefix[..node_start].trim_end();
    before_node.is_empty()
        || before_node.ends_with('&')
        || ends_after_previous_edge_closer(before_node)
}

fn statement_keyword(statement: &str) -> &str {
    statement.split_whitespace().next().unwrap_or("")
}

fn is_node_id_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn ends_after_previous_edge_closer(prefix: &str) -> bool {
    let prefix = prefix.trim_end();
    // A circle or cross can terminate the preceding edge (`--o` / `--x`).
    // Strip only that terminal marker before recognizing the ordinary run.
    let (prefix, terminal_mark) = prefix
        .strip_suffix(['o', 'x'])
        .map(|value| (value.trim_end(), true))
        .unwrap_or((prefix, false));
    if prefix.ends_with('|') {
        return true;
    }

    let (without_arrow, arrow) = prefix
        .strip_suffix('>')
        .map(|text| (text, true))
        .unwrap_or((prefix, false));
    let run_start = without_arrow
        .char_indices()
        .rev()
        .find(|&(_, ch)| !matches!(ch, '-' | '.' | '='))
        .map(|(index, ch)| index + ch.len_utf8())
        .unwrap_or(0);
    let run = &without_arrow[run_start..];
    let solid = run.bytes().all(|byte| byte == b'-')
        && run.len() >= if arrow || terminal_mark { 2 } else { 3 };
    let dotted = (run.starts_with("-.") || run.starts_with(".-"))
        && run.bytes().all(|byte| matches!(byte, b'.' | b'-'));
    let thick = run.bytes().all(|byte| byte == b'=')
        && run.len() >= if arrow || terminal_mark { 2 } else { 3 };
    solid || dotted || thick
}

fn bare_subgraph_title_prefix(line: &str, statement_start: usize, entity_start: usize) -> bool {
    let prefix = line[statement_start..entity_start].trim();
    let Some(rest) = prefix.strip_prefix("subgraph") else {
        return false;
    };
    if !rest.chars().next().is_some_and(char::is_whitespace) {
        return false;
    }
    let rest = rest.trim_start();
    !rest.is_empty() && !rest.contains(['[', '"'])
}

#[derive(Clone, Copy)]
enum InlineEdge {
    Solid,
    Dotted,
    Thick,
}

impl InlineEdge {
    fn opens(text: &str) -> Option<Self> {
        if let Some(rest) = text.strip_prefix("--") {
            return (!rest.starts_with(['-', '>', 'o', 'x'])).then_some(Self::Solid);
        }
        if let Some(rest) = text.strip_prefix("-.") {
            return (!rest.starts_with(['-', '>', 'o', 'x'])).then_some(Self::Dotted);
        }
        if let Some(rest) = text.strip_prefix("==") {
            return (!rest.starts_with(['=', '>', 'o', 'x'])).then_some(Self::Thick);
        }
        None
    }

    fn closing_len(self, text: &str) -> Option<usize> {
        match self {
            Self::Solid if text.starts_with("--") => {
                Some(consume_operator_run(text, |ch| ch == '-'))
            }
            Self::Dotted if text.starts_with(".-") => {
                Some(consume_operator_run(text, |ch| ch == '.' || ch == '-'))
            }
            Self::Thick if text.starts_with("==") => {
                Some(consume_operator_run(text, |ch| ch == '='))
            }
            _ => None,
        }
    }
}

fn consume_operator_run(text: &str, accepts: impl Fn(char) -> bool) -> usize {
    let mut length = text
        .chars()
        .take_while(|&ch| accepts(ch))
        .map(char::len_utf8)
        .sum();
    if text[length..].starts_with(['>', 'o', 'x']) {
        length += 1;
    }
    length
}

/// Return the byte length of an entity-shaped reference beginning at & or #.
///
/// This intentionally recognizes unknown and malformed references too, as
/// long as they are safely bounded by a semicolon with no whitespace or label
/// delimiter. The label normalizer later decodes only its small, documented
/// recognized set and leaves all other references literal.
pub(crate) fn entity_reference_len(text: &str) -> Option<usize> {
    let rest = text.strip_prefix('&').or_else(|| text.strip_prefix('#'))?;
    let mut saw_token = false;
    for (offset, ch) in rest.char_indices() {
        if ch == ';' {
            let prefix = text.len() - rest.len();
            return saw_token.then_some(offset + prefix + 1);
        }
        if rest[offset..].starts_with("--")
            || rest[offset..].starts_with("-.")
            || rest[offset..].starts_with("==")
            || matches!(ch, '>' | '"' | '[' | ']' | '(' | ')' | '{' | '}' | '|')
            || ch.is_whitespace()
        {
            return None;
        }
        if !ch.is_ascii_alphanumeric() && !matches!(ch, '#' | '-' | '_' | '.' | ':' | '+') {
            return None;
        }
        saw_token = true;
    }
    None
}

fn is_escaped_quote(text: &str, quote: usize) -> bool {
    let preceding_backslashes = text.as_bytes()[..quote]
        .iter()
        .rev()
        .take_while(|&&byte| byte == b'\\')
        .count();
    preceding_backslashes % 2 == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_only_at_real_statement_separators() {
        assert_eq!(
            statements(r#"A[AT&amp;T]; B[#35;]; C[&unknown-name;]; D[#xzz;]"#),
            ["A[AT&amp;T]", " B[#35;]", " C[&unknown-name;]", " D[#xzz;]"]
        );
    }

    #[test]
    fn unquoted_fanout_ampersands_remain_flowchart_syntax() {
        assert_eq!(statements("A&B; C"), ["A&B", " C"]);
    }

    #[test]
    fn quoted_semicolons_and_comments_stay_in_the_label() {
        assert_eq!(
            statements(r#"A["literal; %% text"]; B %% trailing; C"#),
            [r#"A["literal; %% text"]"#, " B "]
        );
    }

    #[test]
    fn unquoted_shape_and_pipe_labels_keep_their_inner_boundaries() {
        assert_eq!(
            statements("A[alpha; beta %% literal]; B"),
            ["A[alpha; beta %% literal]", " B"]
        );
        assert_eq!(
            statements("A -->|alpha; beta %% literal| B; C"),
            ["A -->|alpha; beta %% literal| B", " C"]
        );
    }

    #[test]
    fn inline_edge_text_protects_entity_semicolons_for_every_style() {
        assert_eq!(
            statements("A -- AT&amp;T --> B; C"),
            ["A -- AT&amp;T --> B", " C"]
        );
        assert_eq!(
            statements("A -. AT&amp;T .-> B; C"),
            ["A -. AT&amp;T .-> B", " C"]
        );
        assert_eq!(
            statements("A == AT&amp;T ==> B; C"),
            ["A == AT&amp;T ==> B", " C"]
        );
    }

    #[test]
    fn inline_edge_text_owns_plain_semicolons_and_comments_for_every_style() {
        for (source, first) in [
            (
                "A -- plain; %% literal --> B; C --> D",
                "A -- plain; %% literal --> B",
            ),
            (
                "A -. plain; %% literal .-> B; C --> D",
                "A -. plain; %% literal .-> B",
            ),
            (
                "A == plain; %% literal ==> B; C --> D",
                "A == plain; %% literal ==> B",
            ),
        ] {
            assert_eq!(statements(source), [first, " C --> D"], "{source}");
        }
    }

    #[test]
    fn source_terminal_marks_do_not_break_inline_or_chained_edge_scanning() {
        assert_eq!(
            statements("A o-- plain; %% literal --> B; C"),
            ["A o-- plain; %% literal --> B", " C"]
        );
        assert_eq!(
            statements("A --o B -- chained; %% literal --> C; D"),
            ["A --o B -- chained; %% literal --> C", " D"]
        );
        assert_eq!(
            statements("A <== note; %% literal ==> B; C"),
            ["A <== note; %% literal ==> B", " C"]
        );
        for (source, first) in [
            (
                "o -- plain; %% literal --> B; C",
                "o -- plain; %% literal --> B",
            ),
            (
                "x -. plain; %% literal .-> B; C",
                "x -. plain; %% literal .-> B",
            ),
            (
                "x == plain; %% literal ==> B; C",
                "x == plain; %% literal ==> B",
            ),
        ] {
            assert_eq!(statements(source), [first, " C"], "{source}");
        }
    }

    #[test]
    fn inline_closers_are_consumed_as_full_operator_runs() {
        assert_eq!(
            statements("A -- AT&amp;T --- B; C --> D"),
            ["A -- AT&amp;T --- B", " C --> D"]
        );
        assert_eq!(
            statements("A == AT&amp;T === B %% trailing; C"),
            ["A == AT&amp;T === B "]
        );
        assert_eq!(
            statements("A -. text .- B; C --> D"),
            ["A -. text .- B", " C --> D"]
        );
        assert_eq!(
            statements("A -- text ----> B; C"),
            ["A -- text ----> B", " C"]
        );
        assert_eq!(
            statements("A == text =====> B; C"),
            ["A == text =====> B", " C"]
        );
    }

    #[test]
    fn malformed_entity_text_cannot_cross_an_inline_closer() {
        for (source, first) in [
            ("A--R&D-->B; C", "A--R&D-->B"),
            ("A--&bad-->B; C", "A--&bad-->B"),
            ("A--#xzz-->B; C", "A--#xzz-->B"),
            ("A--&unknown_name;-->B; C", "A--&unknown_name;-->B"),
            ("A--&unknown.name;-->B; C", "A--&unknown.name;-->B"),
            ("A--&unknown:name;-->B; C", "A--&unknown:name;-->B"),
            ("A--&unknown+name;-->B; C", "A--&unknown+name;-->B"),
        ] {
            assert_eq!(statements(source), [first, " C"], "{source}");
        }
    }

    #[test]
    fn title_text_is_not_an_inline_edge() {
        assert_eq!(
            statements("title release--plan; A --> B"),
            ["title release--plan", " A --> B"]
        );
    }

    #[test]
    fn non_edge_statement_keywords_never_open_inline_labels() {
        for (source, expected) in [
            (
                "title [release]--plan; A-->B",
                vec!["title [release]--plan", " A-->B"],
            ),
            (
                "title A --> B -- plan; C-->D",
                vec!["title A --> B -- plan", " C-->D"],
            ),
            (
                "subgraph G [release]--plan; A; end; C",
                vec!["subgraph G [release]--plan", " A", " end", " C"],
            ),
            (
                "click A href -- plan; C-->D",
                vec!["click A href -- plan", " C-->D"],
            ),
            (
                "style A fill:#f00 -- plan; C-->D",
                vec!["style A fill:#f00 -- plan", " C-->D"],
            ),
            (
                "flowchart LR -- plan; A-->B",
                vec!["flowchart LR -- plan", " A-->B"],
            ),
            ("end -- plan; C", vec!["end -- plan", " C"]),
        ] {
            assert_eq!(statements(source), expected, "{source}");
        }
        assert_eq!(classify_non_edge_keyword("titleNode"), None);
        assert_eq!(
            statements("titleNode[release] -- plan; %% text --> B; C"),
            ["titleNode[release] -- plan; %% text --> B", " C"]
        );
    }

    #[test]
    fn predecessor_edge_closers_enable_following_inline_labels() {
        for (source, first) in [
            (
                "A -.-> B -- label; %% text --> C; D",
                "A -.-> B -- label; %% text --> C",
            ),
            (
                "A -.- B -- label; %% text --> C; D",
                "A -.- B -- label; %% text --> C",
            ),
            (
                "A -...-> B -- label; %% text --> C; D",
                "A -...-> B -- label; %% text --> C",
            ),
            (
                "A --> B -- label; %% text --> C; D",
                "A --> B -- label; %% text --> C",
            ),
            (
                "A ==> B -- label; %% text --> C; D",
                "A ==> B -- label; %% text --> C",
            ),
            (
                "A -->|edge| B -- label; %% text --> C; D",
                "A -->|edge| B -- label; %% text --> C",
            ),
        ] {
            assert_eq!(statements(source), [first, " D"], "{source}");
        }
    }

    #[test]
    fn unicode_node_ids_open_and_chain_inline_labels() {
        assert_eq!(
            statements("源 -- note; %% text --> 目; C"),
            ["源 -- note; %% text --> 目", " C"]
        );
        assert_eq!(
            statements("α & β -- note; %% text --> γ; C"),
            ["α & β -- note; %% text --> γ", " C"]
        );
        assert_eq!(
            statements("源 --> 目 -- note; %% text --> 終; C"),
            ["源 --> 目 -- note; %% text --> 終", " C"]
        );
    }

    #[test]
    fn bare_subgraph_titles_protect_their_entity_semicolons() {
        assert_eq!(
            statements("subgraph AT&amp;T; A; end"),
            ["subgraph AT&amp;T", " A", " end"]
        );
        assert_eq!(
            statements("subgraph G AT&unknown_name;X; A; end"),
            ["subgraph G AT&unknown_name;X", " A", " end"]
        );
        for (source, first) in [
            (
                "subgraph G AT&unknown.name;X; A; end",
                "subgraph G AT&unknown.name;X",
            ),
            (
                "subgraph G AT&unknown:name;X; A; end",
                "subgraph G AT&unknown:name;X",
            ),
            (
                "subgraph G AT&unknown+name;X; A; end",
                "subgraph G AT&unknown+name;X",
            ),
        ] {
            assert_eq!(statements(source), [first, " A", " end"], "{source}");
        }
    }

    #[test]
    fn odd_backslashes_escape_quotes_but_even_backslashes_do_not() {
        assert_eq!(
            statements("A[\"say \\\"; still text\"]; B"),
            [r#"A["say \"; still text"]"#, " B"]
        );
        assert_eq!(statements("A[\"two \\\\\"]; B"), [r#"A["two \\"]"#, " B"]);
    }
}
