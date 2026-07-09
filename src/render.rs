//! Render placed flowchart geometry onto a character canvas.

use crate::layout::{BoxGeom, EDGE_LABEL_PAD, Placed};
use crate::parse::{EdgeKind, Graph, Shape};
use crate::style::{E, N, S, Style, W};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Clone, Copy)]
enum Cell {
    Empty,
    Line {
        bits: u8,
        rounded: bool,
        kind: EdgeKind,
    },
    Text(char),
    WideCont,
}

struct Canvas {
    w: usize,
    h: usize,
    cells: Vec<Cell>,
}

impl Canvas {
    fn new(w: usize, h: usize) -> Canvas {
        Canvas {
            w,
            h,
            cells: vec![Cell::Empty; w.saturating_mul(h)],
        }
    }

    fn idx(&self, x: usize, y: usize) -> usize {
        y * self.w + x
    }

    fn in_bounds(&self, x: usize, y: usize) -> bool {
        x < self.w && y < self.h
    }

    fn add_line_bits(&mut self, x: usize, y: usize, bits: u8, kind: EdgeKind) {
        if !self.in_bounds(x, y) {
            return;
        }
        let ix = self.idx(x, y);
        self.cells[ix] = match self.cells[ix] {
            Cell::Empty => Cell::Line {
                bits,
                rounded: false,
                kind,
            },
            Cell::Line {
                bits: old,
                rounded,
                kind: old_kind,
            } => Cell::Line {
                bits: old | bits,
                rounded,
                // Junctions render plain in style.rs; keep the first kind for
                // straight single-kind spans.
                kind: old_kind,
            },
            Cell::Text(_) | Cell::WideCont => self.cells[ix],
        };
    }

    fn mark_rounded(&mut self, x: usize, y: usize) {
        if !self.in_bounds(x, y) {
            return;
        }
        let ix = self.idx(x, y);
        if let Cell::Line { bits, kind, .. } = self.cells[ix] {
            self.cells[ix] = Cell::Line {
                bits,
                rounded: true,
                kind,
            };
        }
    }

    fn put_text_char(&mut self, x: usize, y: usize, ch: char) {
        if !self.in_bounds(x, y) {
            return;
        }
        let width = ch.width().unwrap_or(1).max(1);
        let ix = self.idx(x, y);
        self.cells[ix] = Cell::Text(ch);
        for dx in 1..width {
            if self.in_bounds(x + dx, y) {
                let cont_ix = self.idx(x + dx, y);
                self.cells[cont_ix] = Cell::WideCont;
            }
        }
    }

    fn put_text(&mut self, mut x: usize, y: usize, text: &str) {
        for ch in text.chars() {
            self.put_text_char(x, y, ch);
            x += ch.width().unwrap_or(1).max(1);
        }
    }

    fn finish(&self, style: Style) -> String {
        let mut rows = Vec::new();
        for y in 0..self.h {
            let mut row = String::new();
            for x in 0..self.w {
                match self.cells[self.idx(x, y)] {
                    Cell::Empty => row.push(' '),
                    Cell::Line {
                        bits,
                        rounded,
                        kind,
                    } => row.push(style.line(bits, rounded, kind)),
                    Cell::Text(ch) => row.push(ch),
                    Cell::WideCont => {}
                }
            }
            while row.ends_with(' ') {
                row.pop();
            }
            rows.push(row);
        }
        let first = rows.iter().position(|row| !row.is_empty());
        let last = rows.iter().rposition(|row| !row.is_empty());
        let Some(first) = first else {
            return String::new();
        };
        let last = last.unwrap();

        let mut out = String::new();
        for row in &rows[first..=last] {
            out.push_str(row);
            out.push('\n');
        }
        out
    }
}

pub fn render(g: &Graph, placed: &Placed, style: Style) -> String {
    let (w, h) = if placed.horizontal {
        (placed.flow_extent, placed.cross_extent)
    } else {
        (placed.cross_extent, placed.flow_extent)
    };
    let (right_extra, bottom_extra) = canvas_extra(g, placed);
    let mut canvas = Canvas::new(w.max(1) + right_extra, h.max(1) + bottom_extra);

    for (i, b) in placed.boxes.iter().enumerate() {
        draw_box(&mut canvas, placed, b, g.nodes[i].shape, style);
    }
    for (ei, segs) in placed.segs.iter().enumerate() {
        for (si, seg) in segs.iter().enumerate() {
            let last = si + 1 == segs.len();
            let arrow = last && g.edges[ei].arrow;
            let kind = g.edges[ei].kind;
            if seg.from.1 == seg.to.1 {
                draw_flow_segment(&mut canvas, placed, seg.from, seg.to, kind, arrow, style);
            } else {
                let track = placed.channels[seg.channel].track_f(seg.track.unwrap_or(0));
                draw_flow_segment(
                    &mut canvas,
                    placed,
                    seg.from,
                    (track, seg.from.1),
                    kind,
                    false,
                    style,
                );
                draw_flow_segment(
                    &mut canvas,
                    placed,
                    (track, seg.from.1),
                    (track, seg.to.1),
                    kind,
                    false,
                    style,
                );
                draw_flow_segment(
                    &mut canvas,
                    placed,
                    (track, seg.to.1),
                    seg.to,
                    kind,
                    arrow,
                    style,
                );
                let a = placed.to_screen(track, seg.from.1);
                let b = placed.to_screen(track, seg.to.1);
                canvas.mark_rounded(a.0, a.1);
                canvas.mark_rounded(b.0, b.1);
            }
        }
        if let (true, Some(first), Some(label)) = (
            placed.horizontal,
            segs.first(),
            g.edges[ei].label.as_deref(),
        ) {
            draw_edge_label(&mut canvas, placed, first.channel, first.from.1, label);
        }
    }
    for &ei in &placed.back_edges {
        draw_back_edge(&mut canvas, placed, g, ei, style);
    }
    for &ei in &placed.self_loops {
        draw_self_loop(&mut canvas, placed, g, ei, style);
    }

    canvas.finish(style)
}

fn canvas_extra(g: &Graph, placed: &Placed) -> (usize, usize) {
    let mut right = 0usize;
    let mut bottom = 0usize;

    if !placed.back_edges.is_empty() {
        if placed.horizontal {
            bottom = bottom.max(placed.back_edges.len() * 2 + 1);
        } else {
            right = right.max(6 + placed.back_edges.len() * 2);
        }
    }
    if !placed.self_loops.is_empty() {
        right = right.max(6);
        bottom = 2;
    }

    for &ei in placed.back_edges.iter().chain(&placed.self_loops) {
        let label_w = g.edges[ei]
            .label
            .as_deref()
            .map(UnicodeWidthStr::width)
            .unwrap_or(0);
        if label_w > 0 {
            let is_horizontal_back = placed.horizontal && placed.back_edges.contains(&ei);
            if !is_horizontal_back {
                right = right.max(label_w + 8 + 2 * EDGE_LABEL_PAD + placed.back_edges.len() * 2);
            }
        }
    }

    (right, bottom)
}

trait ScreenMap {
    fn to_screen(&self, f: usize, c: usize) -> (usize, usize);
    fn box_rect(&self, b: &BoxGeom) -> (usize, usize, usize, usize);
}

impl ScreenMap for Placed {
    fn to_screen(&self, f: usize, c: usize) -> (usize, usize) {
        if self.horizontal {
            let x = if self.flipped {
                self.flow_extent - 1 - f
            } else {
                f
            };
            (x, c)
        } else {
            let y = if self.flipped {
                self.flow_extent - 1 - f
            } else {
                f
            };
            (c, y)
        }
    }

    fn box_rect(&self, b: &BoxGeom) -> (usize, usize, usize, usize) {
        if self.horizontal {
            let x = if self.flipped {
                self.flow_extent - b.f - b.flen
            } else {
                b.f
            };
            (x, b.c, b.flen, b.clen)
        } else {
            let y = if self.flipped {
                self.flow_extent - b.f - b.flen
            } else {
                b.f
            };
            (b.c, y, b.clen, b.flen)
        }
    }
}

fn draw_box(canvas: &mut Canvas, placed: &Placed, b: &BoxGeom, shape: Shape, style: Style) {
    let (x, y, w, h) = placed.box_rect(b);
    let kind = EdgeKind::Solid;
    // B13 / D13: always a rect frame; shape is conveyed by corner/cap/lid hints
    // so grid alignment never depends on true diamond walls.
    let rounded = matches!(
        shape,
        Shape::Rounded | Shape::Stadium | Shape::Circle | Shape::Cylinder | Shape::Hexagon
    );

    for xx in x + 1..x + w - 1 {
        canvas.add_line_bits(xx, y, E | W, kind);
        canvas.add_line_bits(xx, y + h - 1, E | W, kind);
    }
    for yy in y + 1..y + h - 1 {
        canvas.add_line_bits(x, yy, N | S, kind);
        canvas.add_line_bits(x + w - 1, yy, N | S, kind);
    }

    canvas.add_line_bits(x, y, E | S, kind);
    canvas.add_line_bits(x + w - 1, y, S | W, kind);
    canvas.add_line_bits(x, y + h - 1, N | E, kind);
    canvas.add_line_bits(x + w - 1, y + h - 1, N | W, kind);
    if rounded {
        canvas.mark_rounded(x, y);
        canvas.mark_rounded(x + w - 1, y);
        canvas.mark_rounded(x, y + h - 1);
        canvas.mark_rounded(x + w - 1, y + h - 1);
    }

    apply_shape_hints(canvas, x, y, w, h, shape, style);

    let inner_w = w.saturating_sub(2);
    for (i, line) in b.lines.iter().enumerate() {
        let text_w = line.width();
        let start = x + 1 + inner_w.saturating_sub(text_w) / 2;
        canvas.put_text(start, y + 1 + i, line);
    }
}

/// Overlay shape-hint glyphs on a rect frame (D13). Corners preferred so side
/// ports for edges stay free; stadium/circle caps sit on the vertical mid of
/// the left/right borders (same cells edges may use — still readable).
fn apply_shape_hints(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    shape: Shape,
    style: Style,
) {
    let mid_y = y + h / 2;
    match shape {
        Shape::Rect | Shape::Rounded => {}
        Shape::Stadium | Shape::Circle => {
            let (left, right) = if style.ascii {
                ('(', ')')
            } else {
                ('(', ')')
            };
            canvas.put_text_char(x, mid_y, left);
            canvas.put_text_char(x + w - 1, mid_y, right);
        }
        Shape::Diamond => {
            let ch = if style.ascii { '*' } else { '◇' };
            canvas.put_text_char(x, y, ch);
            canvas.put_text_char(x + w - 1, y, ch);
            canvas.put_text_char(x, y + h - 1, ch);
            canvas.put_text_char(x + w - 1, y + h - 1, ch);
        }
        Shape::Cylinder => {
            // Lid on the top edge (distinct from a plain rect top).
            let lid = if style.ascii { '=' } else { '═' };
            for xx in x + 1..x + w - 1 {
                canvas.put_text_char(xx, y, lid);
            }
        }
        Shape::Hexagon => {
            if style.ascii {
                canvas.put_text_char(x, y, '/');
                canvas.put_text_char(x + w - 1, y, '\\');
                canvas.put_text_char(x, y + h - 1, '\\');
                canvas.put_text_char(x + w - 1, y + h - 1, '/');
            } else {
                // Faceted corners without breaking the rect grid.
                canvas.put_text_char(x, y, '╱');
                canvas.put_text_char(x + w - 1, y, '╲');
                canvas.put_text_char(x, y + h - 1, '╲');
                canvas.put_text_char(x + w - 1, y + h - 1, '╱');
            }
        }
    }
}

fn draw_flow_segment(
    canvas: &mut Canvas,
    placed: &Placed,
    from: (usize, usize),
    to: (usize, usize),
    kind: EdgeKind,
    arrow: bool,
    style: Style,
) {
    let end = if arrow { cell_before(to, from) } else { to };
    draw_flow_line(canvas, placed, from, end, kind);
    if arrow {
        let arrow_at = cell_before(to, from);
        let (x, y) = placed.to_screen(arrow_at.0, arrow_at.1);
        let target = placed.to_screen(to.0, to.1);
        canvas.put_text_char(x, y, arrow_toward((x, y), target, style));
    }
}

fn draw_self_loop(
    canvas: &mut Canvas,
    placed: &Placed,
    g: &Graph,
    edge_index: usize,
    style: Style,
) {
    let edge = &g.edges[edge_index];
    let (x, y, w, h) = placed.box_rect(&placed.boxes[edge.from]);
    let label_w = edge
        .label
        .as_deref()
        .map(UnicodeWidthStr::width)
        .unwrap_or(0);
    let source = (x + w - 1, y + h / 2);
    let target = (x + w / 2, y + h - 1);
    let loop_x = x + w + label_w + 3 + EDGE_LABEL_PAD;
    let loop_y = y + h;
    let points = [
        source,
        (loop_x, source.1),
        (loop_x, loop_y),
        (target.0, loop_y),
        target,
    ];

    draw_screen_path(canvas, &points, edge.kind, edge.arrow, style);
    if let Some(label) = edge.label.as_deref() {
        let label_x = source.0 + 2 + EDGE_LABEL_PAD;
        if label_x + label_w < loop_x {
            canvas.put_text(label_x, source.1, label);
        }
    }
}

fn draw_back_edge(
    canvas: &mut Canvas,
    placed: &Placed,
    g: &Graph,
    edge_index: usize,
    style: Style,
) {
    if placed.horizontal {
        draw_horizontal_back_edge(canvas, placed, g, edge_index, style);
    } else {
        draw_vertical_back_edge(canvas, placed, g, edge_index, style);
    }
}

fn draw_vertical_back_edge(
    canvas: &mut Canvas,
    placed: &Placed,
    g: &Graph,
    edge_index: usize,
    style: Style,
) {
    let edge = &g.edges[edge_index];
    let (sx, sy, sw, sh) = placed.box_rect(&placed.boxes[edge.from]);
    let (tx, ty, tw, th) = placed.box_rect(&placed.boxes[edge.to]);
    let label_w = edge
        .label
        .as_deref()
        .map(UnicodeWidthStr::width)
        .unwrap_or(0);
    let source = (sx + sw - 1, sy + sh / 2);
    let target = (tx + tw - 1, ty + th / 2);
    let track = placed
        .back_edges
        .iter()
        .position(|&ei| ei == edge_index)
        .unwrap_or(0);
    let min_perimeter = source.0.max(target.0) + label_w + 5 + EDGE_LABEL_PAD + track * 2;
    let perimeter_x = min_perimeter.min(canvas.w.saturating_sub(2));
    let points = [
        source,
        (perimeter_x, source.1),
        (perimeter_x, target.1),
        target,
    ];

    draw_screen_path(canvas, &points, edge.kind, edge.arrow, style);
    if let Some(label) = edge.label.as_deref() {
        let label_x = target.0 + EDGE_LABEL_PAD + if edge.arrow { 2 } else { 1 };
        if label_x + label_w < perimeter_x {
            canvas.put_text(label_x, target.1, label);
        }
    }
}

fn draw_horizontal_back_edge(
    canvas: &mut Canvas,
    placed: &Placed,
    g: &Graph,
    edge_index: usize,
    style: Style,
) {
    let edge = &g.edges[edge_index];
    let (sx, sy, sw, sh) = placed.box_rect(&placed.boxes[edge.from]);
    let (tx, ty, tw, th) = placed.box_rect(&placed.boxes[edge.to]);
    let label_w = edge
        .label
        .as_deref()
        .map(UnicodeWidthStr::width)
        .unwrap_or(0);
    let source = (sx + sw / 2, sy + sh - 1);
    let target = (tx + tw / 2, ty + th - 1);
    let track = placed
        .back_edges
        .iter()
        .position(|&ei| ei == edge_index)
        .unwrap_or(0);
    let perimeter_y = canvas
        .h
        .saturating_sub(1 + 2 * (placed.back_edges.len().saturating_sub(1) - track));
    let points = [
        source,
        (source.0, perimeter_y),
        (target.0, perimeter_y),
        target,
    ];

    draw_screen_path(canvas, &points, edge.kind, edge.arrow, style);
    if let Some(label) = edge.label.as_deref() {
        let left = source.0.min(target.0);
        let right = source.0.max(target.0);
        if right > left + label_w {
            let label_x = left + (right - left - label_w) / 2;
            canvas.put_text(label_x, perimeter_y, label);
        }
    }
}

fn draw_edge_label(
    canvas: &mut Canvas,
    placed: &Placed,
    channel: usize,
    cross: usize,
    label: &str,
) {
    let label_w = label.width();
    if label_w == 0 {
        return;
    }
    let zone = placed.channels[channel].label_zone;
    let f = placed.channels[channel].start + 1 + zone.saturating_sub(label_w) / 2;
    let (x, y) = if placed.flipped {
        placed.to_screen(f + label_w.saturating_sub(1), cross)
    } else {
        placed.to_screen(f, cross)
    };
    canvas.put_text(x, y, label);
}

fn draw_flow_line(
    canvas: &mut Canvas,
    placed: &Placed,
    from: (usize, usize),
    to: (usize, usize),
    kind: EdgeKind,
) {
    if from == to {
        return;
    }
    let mut cur = from;
    while cur != to {
        let Some(next) = step_toward(cur, to) else {
            break;
        };
        draw_step(canvas, placed, cur, next, kind);
        cur = next;
    }
}

fn draw_screen_path(
    canvas: &mut Canvas,
    points: &[(usize, usize)],
    kind: EdgeKind,
    arrow: bool,
    style: Style,
) {
    if points.len() < 2 {
        return;
    }

    let mut stops = points.to_vec();
    let target = *stops.last().unwrap();
    let arrow_at = if arrow {
        let from = stops[stops.len() - 2];
        let at = cell_before(target, from);
        *stops.last_mut().unwrap() = at;
        Some(at)
    } else {
        None
    };

    for pair in stops.windows(2) {
        draw_screen_line(canvas, pair[0], pair[1], kind);
    }
    for &point in stops.iter().skip(1).take(stops.len().saturating_sub(2)) {
        canvas.mark_rounded(point.0, point.1);
    }

    if let Some(at) = arrow_at {
        canvas.put_text_char(at.0, at.1, arrow_toward(at, target, style));
    }
}

fn draw_screen_line(canvas: &mut Canvas, from: (usize, usize), to: (usize, usize), kind: EdgeKind) {
    if from == to {
        return;
    }
    let mut cur = from;
    while cur != to {
        let Some(next) = step_toward(cur, to) else {
            break;
        };
        draw_screen_step(canvas, cur, next, kind);
        cur = next;
    }
}

fn step_toward(cur: (usize, usize), to: (usize, usize)) -> Option<(usize, usize)> {
    if cur.0 < to.0 {
        Some((cur.0 + 1, cur.1))
    } else if cur.0 > to.0 {
        Some((cur.0 - 1, cur.1))
    } else if cur.1 < to.1 {
        Some((cur.0, cur.1 + 1))
    } else if cur.1 > to.1 {
        Some((cur.0, cur.1 - 1))
    } else {
        None
    }
}

fn cell_before(to: (usize, usize), from: (usize, usize)) -> (usize, usize) {
    if from.0 < to.0 {
        (to.0.saturating_sub(1), to.1)
    } else if from.0 > to.0 {
        (to.0 + 1, to.1)
    } else if from.1 < to.1 {
        (to.0, to.1.saturating_sub(1))
    } else if from.1 > to.1 {
        (to.0, to.1 + 1)
    } else {
        to
    }
}

fn draw_step(
    canvas: &mut Canvas,
    placed: &Placed,
    a: (usize, usize),
    b: (usize, usize),
    kind: EdgeKind,
) {
    let (ax, ay) = placed.to_screen(a.0, a.1);
    let (bx, by) = placed.to_screen(b.0, b.1);
    let (abit, bbit) = if ax + 1 == bx {
        (E, W)
    } else if bx + 1 == ax {
        (W, E)
    } else if ay + 1 == by {
        (S, N)
    } else if by + 1 == ay {
        (N, S)
    } else {
        return;
    };
    canvas.add_line_bits(ax, ay, abit, kind);
    canvas.add_line_bits(bx, by, bbit, kind);
}

fn draw_screen_step(canvas: &mut Canvas, a: (usize, usize), b: (usize, usize), kind: EdgeKind) {
    let (abit, bbit) = if a.0 + 1 == b.0 {
        (E, W)
    } else if b.0 + 1 == a.0 {
        (W, E)
    } else if a.1 + 1 == b.1 {
        (S, N)
    } else if b.1 + 1 == a.1 {
        (N, S)
    } else {
        return;
    };
    canvas.add_line_bits(a.0, a.1, abit, kind);
    canvas.add_line_bits(b.0, b.1, bbit, kind);
}

fn arrow_toward(from: (usize, usize), to: (usize, usize), style: Style) -> char {
    if to.0 > from.0 {
        style.arrow_right()
    } else if to.0 < from.0 {
        style.arrow_left()
    } else if to.1 > from.1 {
        style.arrow_down()
    } else {
        style.arrow_up()
    }
}
