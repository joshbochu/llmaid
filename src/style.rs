//! Glyph sets and box-drawing junction resolution. No layout logic here.

use crate::parse::EdgeKind;

/// Direction bits for line cells. A cell's glyph is resolved from the union
/// of bits drawn into it, so crossings and tees always come out right
/// (`─` meeting `│` becomes `┼`).
pub const N: u8 = 1;
pub const E: u8 = 2;
pub const S: u8 = 4;
pub const W: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    pub ascii: bool,
}

impl Style {
    /// Resolve a line cell to a glyph. `rounded` applies to corner cells only.
    /// `kind` styles straight segments (dotted/thick); junctions stay plain so
    /// the junction table stays small and unambiguous.
    pub fn line(self, bits: u8, rounded: bool, kind: EdgeKind) -> char {
        if self.ascii {
            return match bits {
                b if b == (N | S) || b == N || b == S => '|',
                b if b == (E | W) || b == E || b == W => '-',
                _ => '+',
            };
        }
        match bits {
            b if b == (N | S) || b == N || b == S => match kind {
                EdgeKind::Dotted => '┊',
                EdgeKind::Thick => '┃',
                EdgeKind::Solid => '│',
            },
            b if b == (E | W) || b == E || b == W => match kind {
                EdgeKind::Dotted => '┄',
                EdgeKind::Thick => '━',
                EdgeKind::Solid => '─',
            },
            b if b == (N | E) => {
                if rounded {
                    '╰'
                } else {
                    '└'
                }
            }
            b if b == (N | W) => {
                if rounded {
                    '╯'
                } else {
                    '┘'
                }
            }
            b if b == (S | E) => {
                if rounded {
                    '╭'
                } else {
                    '┌'
                }
            }
            b if b == (S | W) => {
                if rounded {
                    '╮'
                } else {
                    '┐'
                }
            }
            b if b == (N | E | S) => '├',
            b if b == (N | S | W) => '┤',
            b if b == (E | S | W) => '┬',
            b if b == (N | E | W) => '┴',
            _ => '┼',
        }
    }

    pub fn arrow_right(self) -> char {
        if self.ascii { '>' } else { '▶' }
    }
    pub fn arrow_left(self) -> char {
        if self.ascii { '<' } else { '◀' }
    }
    pub fn arrow_up(self) -> char {
        if self.ascii { '^' } else { '▲' }
    }
    pub fn arrow_down(self) -> char {
        if self.ascii { 'v' } else { '▼' }
    }
}
