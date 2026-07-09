//! Render placed flowchart geometry onto a character canvas.

use crate::layout::{BoxGeom, ClusterGeom, EDGE_LABEL_PAD, Placed};
use crate::parse::{EdgeKind, Graph, Shape};
use crate::style::{E, N, S, Style, W};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Clone, Copy, Debug)]
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
    paint(g, placed, style).finish(style)
}

/// Render and verify B14 frame invariants (closed borders, labels intact,
/// edge endpoints marked). Returns the diagram plus any invariant failures.
pub fn render_with_checks(
    g: &Graph,
    placed: &Placed,
    style: Style,
) -> (String, Vec<String>) {
    let canvas = paint(g, placed, style);
    let failures = check_invariants(g, placed, &canvas);
    (canvas.finish(style), failures)
}

fn paint(g: &Graph, placed: &Placed, style: Style) -> Canvas {
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
        if let (Some(first), Some(label)) = (segs.first(), g.edges[ei].label.as_deref()) {
            draw_edge_label(&mut canvas, placed, first.channel, first.from.1, label);
        }
    }
    for &ei in &placed.back_edges {
        draw_back_edge(&mut canvas, placed, g, ei, style);
    }
    for &ei in &placed.self_loops {
        draw_self_loop(&mut canvas, placed, g, ei, style);
    }
    // Cluster frames last, only on empty cells so exit edges punch clean gaps
    // instead of merging into ┬┴ on the border.
    for cl in &placed.clusters {
        draw_cluster(&mut canvas, placed, cl, style);
    }

    canvas
}

/// B14 frame invariants on the pre-trim canvas.
fn check_invariants(g: &Graph, placed: &Placed, canvas: &Canvas) -> Vec<String> {
    let mut failures = Vec::new();

    // No truncation glyphs anywhere.
    for y in 0..canvas.h {
        for x in 0..canvas.w {
            if let Cell::Text(ch) = canvas.cells[canvas.idx(x, y)] {
                if ch == '…' || ch == '⋯' {
                    failures.push(format!("truncated glyph {ch:?} at ({x},{y})"));
                }
            }
        }
    }

    // Cluster frames closed (title may overwrite top edge cells — still ink).
    for cl in &placed.clusters {
        let (x, y, w, h) = placed.cluster_rect(cl);
        if w < 2 || h < 2 {
            continue;
        }
        for (cx, cy, name) in [
            (x, y, "TL"),
            (x + w - 1, y, "TR"),
            (x, y + h - 1, "BL"),
            (x + w - 1, y + h - 1, "BR"),
        ] {
            if !canvas.in_bounds(cx, cy)
                || matches!(canvas.cells[canvas.idx(cx, cy)], Cell::Empty)
            {
                failures.push(format!(
                    "cluster `{}` open {name} at ({cx},{cy})",
                    cl.title
                ));
            }
        }
    }

    // Closed box borders + interior labels still Text (not overwritten by lines).
    for (i, b) in placed.boxes.iter().enumerate() {
        let (bx, by, bw, bh) = placed.box_rect(b);
        let id = &g.nodes[i].id;
        let corner = |x, y| canvas.in_bounds(x, y) && !matches!(canvas.cells[canvas.idx(x, y)], Cell::Empty);
        for (cx, cy, name) in [
            (bx, by, "TL"),
            (bx + bw - 1, by, "TR"),
            (bx, by + bh - 1, "BL"),
            (bx + bw - 1, by + bh - 1, "BR"),
        ] {
            if !corner(cx, cy) {
                failures.push(format!("box `{id}` open {name} corner at ({cx},{cy})"));
            }
        }
        for xx in bx + 1..bx + bw - 1 {
            if canvas.in_bounds(xx, by)
                && matches!(canvas.cells[canvas.idx(xx, by)], Cell::Empty)
            {
                failures.push(format!("box `{id}` gap on top edge at ({xx},{by})"));
            }
            if canvas.in_bounds(xx, by + bh - 1)
                && matches!(canvas.cells[canvas.idx(xx, by + bh - 1)], Cell::Empty)
            {
                failures.push(format!("box `{id}` gap on bottom edge at ({xx},{})", by + bh - 1));
            }
        }
        for yy in by + 1..by + bh - 1 {
            if canvas.in_bounds(bx, yy)
                && matches!(canvas.cells[canvas.idx(bx, yy)], Cell::Empty)
            {
                failures.push(format!("box `{id}` gap on left edge at ({bx},{yy})"));
            }
            if canvas.in_bounds(bx + bw - 1, yy)
                && matches!(canvas.cells[canvas.idx(bx + bw - 1, yy)], Cell::Empty)
            {
                failures.push(format!(
                    "box `{id}` gap on right edge at ({},{})",
                    bx + bw - 1,
                    yy
                ));
            }
        }

        // Label lines written during draw_box must remain Text cells.
        let inner_w = bw.saturating_sub(2);
        for (li, line) in b.lines.iter().enumerate() {
            let text_w = line.width();
            let mut x = bx + 1 + inner_w.saturating_sub(text_w) / 2;
            let y = by + 1 + li;
            for ch in line.chars() {
                let cw = ch.width().unwrap_or(1).max(1);
                if !canvas.in_bounds(x, y) {
                    failures.push(format!(
                        "box `{id}` label {ch:?} out of bounds at ({x},{y})"
                    ));
                } else {
                    match canvas.cells[canvas.idx(x, y)] {
                        Cell::Text(got) if got == ch => {}
                        other => failures.push(format!(
                            "box `{id}` label {ch:?} overwritten at ({x},{y}): {other:?}"
                        )),
                    }
                }
                x += cw;
            }
        }
    }

    // Forward edge segments: endpoints carry ink (line or arrow text).
    for (ei, segs) in placed.segs.iter().enumerate() {
        if segs.is_empty() {
            continue;
        }
        let first = segs.first().unwrap();
        let last = segs.last().unwrap();
        for (pt, which) in [(first.from, "start"), (last.to, "end")] {
            let (x, y) = placed.to_screen(pt.0, pt.1);
            if !canvas.in_bounds(x, y) {
                failures.push(format!("edge {ei} {which} out of bounds ({x},{y})"));
                continue;
            }
            if matches!(canvas.cells[canvas.idx(x, y)], Cell::Empty) {
                // Arrow may sit one cell before the target border.
                let ok_near = neighbors(x, y).iter().any(|&(nx, ny)| {
                    canvas.in_bounds(nx, ny)
                        && !matches!(canvas.cells[canvas.idx(nx, ny)], Cell::Empty)
                });
                if !ok_near {
                    failures.push(format!(
                        "edge {ei} {which} does not reach endpoint near ({x},{y})"
                    ));
                }
            }
        }
    }

    failures
}

fn neighbors(x: usize, y: usize) -> [(usize, usize); 4] {
    [
        (x.wrapping_sub(1), y),
        (x + 1, y),
        (x, y.wrapping_sub(1)),
        (x, y + 1),
    ]
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

    // TB/BT forward labels sit to the right of the vertical shaft.
    if !placed.horizontal {
        for (ei, segs) in placed.segs.iter().enumerate() {
            if segs.is_empty() {
                continue;
            }
            if let Some(label) = g.edges[ei].label.as_deref() {
                let w = label.width() + 2; // padded ` label `
                right = right.max(w + 2);
            }
        }
    }

    (right, bottom)
}

trait ScreenMap {
    fn to_screen(&self, f: usize, c: usize) -> (usize, usize);
    fn box_rect(&self, b: &BoxGeom) -> (usize, usize, usize, usize);
    fn cluster_rect(&self, cl: &ClusterGeom) -> (usize, usize, usize, usize);
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

    fn cluster_rect(&self, cl: &ClusterGeom) -> (usize, usize, usize, usize) {
        if self.horizontal {
            let x = if self.flipped {
                self.flow_extent - cl.f - cl.flen
            } else {
                cl.f
            };
            (x, cl.c, cl.flen, cl.clen)
        } else {
            let y = if self.flipped {
                self.flow_extent - cl.f - cl.flen
            } else {
                cl.f
            };
            (cl.c, y, cl.clen, cl.flen)
        }
    }
}

fn draw_cluster(canvas: &mut Canvas, placed: &Placed, cl: &ClusterGeom, style: Style) {
    let (x, y, w, h) = placed.cluster_rect(cl);
    if w < 2 || h < 2 {
        return;
    }
    let kind = EdgeKind::Solid;
    // Only paint empty cells so boxes/edges already drawn punch clean gaps
    // through the frame (no ┬┴ merge on exit shafts).
    let paint = |canvas: &mut Canvas, px: usize, py: usize, bits: u8| {
        if !canvas.in_bounds(px, py) {
            return;
        }
        let ix = canvas.idx(px, py);
        if !matches!(canvas.cells[ix], Cell::Empty) {
            return;
        }
        canvas.add_line_bits(px, py, bits, kind);
    };

    for xx in x + 1..x + w - 1 {
        paint(canvas, xx, y, E | W);
        paint(canvas, xx, y + h - 1, E | W);
    }
    for yy in y + 1..y + h - 1 {
        paint(canvas, x, yy, N | S);
        paint(canvas, x + w - 1, yy, N | S);
    }
    paint(canvas, x, y, E | S);
    paint(canvas, x + w - 1, y, S | W);
    paint(canvas, x, y + h - 1, N | E);
    paint(canvas, x + w - 1, y + h - 1, N | W);
    canvas.mark_rounded(x, y);
    canvas.mark_rounded(x + w - 1, y);
    canvas.mark_rounded(x, y + h - 1);
    canvas.mark_rounded(x + w - 1, y + h - 1);

    // Title on the first *interior* row (not the border stroke), centered.
    // Only write into empty cells so we never clobber edges/boxes.
    let title = format!(" {} ", cl.title);
    let tw = title.width();
    if tw > 0 && h >= 3 && tw <= w.saturating_sub(2) {
        let tx = x + (w.saturating_sub(tw)) / 2;
        let mut ok = true;
        for dx in 0..tw {
            if !canvas.in_bounds(tx + dx, y + 1)
                || !matches!(canvas.cells[canvas.idx(tx + dx, y + 1)], Cell::Empty)
            {
                ok = false;
                break;
            }
        }
        if ok {
            canvas.put_text(tx, y + 1, &title);
        }
    }

    let _ = style;
}

fn draw_box(canvas: &mut Canvas, placed: &Placed, b: &BoxGeom, shape: Shape, style: Style) {
    let (x, y, w, h) = placed.box_rect(b);
    let kind = EdgeKind::Solid;
    // B13 / D13: always a rect frame; shape is conveyed by corner/cap/lid hints
    // so grid alignment never depends on true diamond walls.
    // House style (D7): rounded corners by default — including Mermaid `[rect]`.
    // Only diamond/hex replace corners with facet glyphs; stadium/circle add caps.
    let rounded = !matches!(shape, Shape::Diamond | Shape::Hexagon);

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
        .map(|l| UnicodeWidthStr::width(l) + 2) // spaces around label
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
        let text = padded_edge_label(label);
        let tw = text.width();
        let label_x = source.0 + 2 + EDGE_LABEL_PAD;
        if label_x + tw < loop_x {
            canvas.put_text(label_x, source.1, &text);
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
        .map(|l| UnicodeWidthStr::width(l) + 2) // spaces around label
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
        let text = padded_edge_label(label);
        let tw = text.width();
        let label_x = target.0 + EDGE_LABEL_PAD + if edge.arrow { 2 } else { 1 };
        if label_x + tw < perimeter_x {
            canvas.put_text(label_x, target.1, &text);
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
        let text = padded_edge_label(label);
        let text_w = text.width();
        let left = source.0.min(target.0);
        let right = source.0.max(target.0);
        if right > left + text_w {
            let label_x = left + (right - left - text_w) / 2;
            canvas.put_text(label_x, perimeter_y, &text);
        }
    }
}

/// On-arrow labels get a space on each side (` scan `) so the word doesn't
/// jam into the box-drawing strokes.
fn padded_edge_label(label: &str) -> String {
    format!(" {label} ")
}

fn draw_edge_label(
    canvas: &mut Canvas,
    placed: &Placed,
    channel: usize,
    cross: usize,
    label: &str,
) {
    if label.is_empty() {
        return;
    }
    let text = padded_edge_label(label);
    let label_w = text.width();
    let ch = &placed.channels[channel];
    if placed.horizontal {
        // Label sits on the horizontal shaft inside the channel label zone.
        let f = ch.start + 1 + ch.label_zone.saturating_sub(label_w) / 2;
        let (x, y) = if placed.flipped {
            placed.to_screen(f + label_w.saturating_sub(1), cross)
        } else {
            placed.to_screen(f, cross)
        };
        canvas.put_text(x, y, &text);
    } else {
        // Vertical shaft: one horizontal band in the channel, text to the right
        // of the line so multi-char labels stay readable (Phase 0.3).
        let f = ch.start + ch.label_zone.max(1) / 2;
        let (x, y) = placed.to_screen(f, cross);
        canvas.put_text(x.saturating_add(1), y, &text);
    }
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
