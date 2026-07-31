//! Readability-preserving text wrapping shared by diagram engines.
//!
//! Width pressure may move complete words onto later lines, but it never
//! slices an identifier or other whitespace-free token into vertical shards.

use unicode_width::UnicodeWidthStr;

/// Narrower terminal text becomes harder to scan than the modest overflow it
/// saves. Intrinsically wider words remain whole and may exceed this floor.
pub(crate) const MIN_READABLE_COLUMNS: usize = 8;

pub(crate) fn max_line_width(text: &str) -> usize {
    text.split('\n')
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0)
}

/// Wrap at whitespace while preserving every non-whitespace token intact.
///
/// Explicit line breaks remain paragraph boundaries. Whitespace between words
/// is normalized to one display cell, matching Mermaid label cleanup.
pub(crate) fn wrap_words(text: &str, max_columns: usize) -> Vec<String> {
    let max_columns = max_columns.max(1);
    let mut output = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            output.push(String::new());
            continue;
        }
        if paragraph.width() <= max_columns {
            output.push(paragraph.to_string());
            continue;
        }

        let mut current = String::new();
        let mut current_width = 0usize;
        for word in paragraph.split_whitespace() {
            let word_width = word.width();
            if current.is_empty() {
                current.push_str(word);
                current_width = word_width;
            } else if current_width + 1 + word_width <= max_columns {
                current.push(' ');
                current.push_str(word);
                current_width += 1 + word_width;
            } else {
                output.push(std::mem::take(&mut current));
                current.push_str(word);
                current_width = word_width;
            }
        }
        if !current.is_empty() {
            output.push(current);
        }
    }
    if output.is_empty() {
        output.push(String::new());
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_words_but_never_slices_identifiers() {
        assert_eq!(
            wrap_words("compile Vec<DeveloperTool>", 8),
            ["compile", "Vec<DeveloperTool>"]
        );
        assert_eq!(wrap_words("one two three", 7), ["one two", "three"]);
    }

    #[test]
    fn explicit_lines_remain_lines() {
        assert_eq!(
            wrap_words("first line\nsecond", 6),
            ["first", "line", "second"]
        );
    }
}
