//! Rasterize screen-space `Scene` primitives onto a character canvas.

use crate::layout::Placed;
use crate::parse::Graph;
use crate::route;
use crate::scene::{
    EdgeKind, Point, Rect, RoutedEdge, Scene, SceneBox, SceneGroup, ScenePath, Shape,
};
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

    fn clear_rect(&mut self, rect: Rect) {
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                if x >= 0 && y >= 0 && self.in_bounds(x as usize, y as usize) {
                    let index = self.idx(x as usize, y as usize);
                    self.cells[index] = Cell::Empty;
                }
            }
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
    render_scene(&route::route(g, placed), style)
}

/// Paint a complete screen-space scene. Diagram engines own all geometry;
/// this function only rasterizes primitives onto the character canvas.
pub fn render_scene(scene: &Scene, style: Style) -> String {
    paint_scene(scene, style).finish(style)
}

fn paint_scene(scene: &Scene, style: Style) -> Canvas {
    let mut scene = scene.clone();
    let (w, h) = scene.normalize();
    paint_normalized_scene(&scene, style, w, h)
}

fn paint_normalized_scene(scene: &Scene, style: Style, w: usize, h: usize) -> Canvas {
    let mut canvas = Canvas::new(w.max(1), h.max(1));

    for b in &scene.boxes {
        draw_scene_box(&mut canvas, b, style);
    }
    for path in &scene.paths {
        draw_scene_path(&mut canvas, path);
    }
    for b in &scene.foreground_boxes {
        canvas.clear_rect(b.rect);
        draw_scene_box(&mut canvas, b, style);
    }
    for edge in &scene.edges {
        draw_scene_edge(&mut canvas, edge, style);
    }
    for group in &scene.groups {
        draw_scene_group(&mut canvas, group);
    }

    canvas
}

fn draw_scene_path(canvas: &mut Canvas, path: &ScenePath) {
    for pair in path.points.windows(2) {
        draw_screen_line(canvas, point(pair[0]), point(pair[1]), path.kind);
    }
    for &corner in &path.rounded {
        let (x, y) = point(corner);
        canvas.mark_rounded(x, y);
    }
}

/// Paint and verify a scene without consulting parser or layout internals.
pub fn render_scene_with_checks(scene: &Scene, style: Style) -> (String, Vec<String>) {
    let mut scene = scene.clone();
    let (w, h) = scene.normalize();
    let canvas = paint_normalized_scene(&scene, style, w, h);
    let failures = check_scene_invariants(&scene, &canvas);
    (canvas.finish(style), failures)
}

/// Runtime render gate used by the CLI: invalid geometry never becomes a
/// successful diagram. Tests can exercise the same path without spawning the
/// binary or relying on an intentionally broken parser/layout input.
pub fn render_scene_checked(scene: &Scene, style: Style) -> Result<String, Vec<String>> {
    let (output, failures) = render_scene_with_checks(scene, style);
    if failures.is_empty() {
        Ok(output)
    } else {
        Err(failures)
    }
}

fn draw_scene_edge(canvas: &mut Canvas, edge: &RoutedEdge, style: Style) {
    for pair in edge.points.windows(2) {
        draw_screen_line(canvas, point(pair[0]), point(pair[1]), edge.kind);
    }
    for &corner in &edge.rounded {
        let (x, y) = point(corner);
        canvas.mark_rounded(x, y);
    }
    if let Some(label) = &edge.label {
        canvas.put_text(label.at.x as usize, label.at.y as usize, &label.text);
    }
    if let Some(arrow) = &edge.arrow {
        let at = point(arrow.at);
        canvas.put_text_char(
            at.0,
            at.1,
            arrow_toward(at, point(arrow.toward), arrow.head, style),
        );
    }
}

fn point(p: Point) -> (usize, usize) {
    (p.x as usize, p.y as usize)
}

fn check_scene_invariants(scene: &Scene, canvas: &Canvas) -> Vec<String> {
    let mut failures = Vec::new();

    for intersection in scene.edge_box_intersections() {
        failures.push(format!(
            "edge {} intersects non-endpoint box {} at ({},{})",
            intersection.edge, intersection.node, intersection.at.x, intersection.at.y
        ));
    }

    for y in 0..canvas.h {
        for x in 0..canvas.w {
            if let Cell::Text(ch) = canvas.cells[canvas.idx(x, y)]
                && matches!(ch, '…' | '⋯')
            {
                failures.push(format!("truncated glyph {ch:?} at ({x},{y})"));
            }
        }
    }

    for group in &scene.groups {
        check_rect_corners(
            canvas,
            group.rect,
            &format!("group {}", group.subgraph),
            &mut failures,
        );
        check_text(
            canvas,
            &group.title,
            &format!("group {} title", group.subgraph),
            &mut failures,
        );
    }

    for b in &scene.boxes {
        check_rect_corners(canvas, b.rect, &format!("box {}", b.node), &mut failures);
        let rect = b.rect;
        let inner_w = rect.w.saturating_sub(2);
        let text_y = rect.y + (rect.h - b.lines.len() as i32) / 2;
        for (line_index, line) in b.lines.iter().enumerate() {
            let text_w = line.width() as i32;
            let text = crate::scene::SceneText::new(
                Point::new(
                    rect.x + 1 + (inner_w - text_w).max(0) / 2,
                    text_y + line_index as i32,
                ),
                line.clone(),
            );
            check_text(
                canvas,
                &text,
                &format!("box {} label", b.node),
                &mut failures,
            );
        }
    }

    for b in &scene.foreground_boxes {
        check_rect_corners(
            canvas,
            b.rect,
            &format!("foreground box {}", b.node),
            &mut failures,
        );
        let rect = b.rect;
        let inner_w = rect.w.saturating_sub(2);
        let text_y = rect.y + (rect.h - b.lines.len() as i32) / 2;
        for (line_index, line) in b.lines.iter().enumerate() {
            let text_w = line.width() as i32;
            let text = crate::scene::SceneText::new(
                Point::new(
                    rect.x + 1 + (inner_w - text_w).max(0) / 2,
                    text_y + line_index as i32,
                ),
                line.clone(),
            );
            check_text(
                canvas,
                &text,
                &format!("foreground box {} label", b.node),
                &mut failures,
            );
        }
    }

    for path in &scene.paths {
        check_path(
            canvas,
            path.path,
            &path.points,
            &format!("path {}", path.path),
            &mut failures,
        );
    }

    for edge in &scene.edges {
        if edge.points.len() < 2 {
            failures.push(format!("edge {} has fewer than two path points", edge.edge));
            continue;
        }
        for pair in edge.points.windows(2) {
            if pair[0].x != pair[1].x && pair[0].y != pair[1].y {
                failures.push(format!(
                    "edge {} has diagonal segment {:?} -> {:?}",
                    edge.edge, pair[0], pair[1]
                ));
            }
        }
        let start = point(edge.points[0]);
        if !canvas.in_bounds(start.0, start.1)
            || matches!(canvas.cells[canvas.idx(start.0, start.1)], Cell::Empty)
        {
            failures.push(format!("edge {} start is not painted", edge.edge));
        }
        if let Some(label) = &edge.label {
            check_text(
                canvas,
                label,
                &format!("edge {} label", edge.edge),
                &mut failures,
            );
        }
        if let Some(arrow) = &edge.arrow {
            let at = point(arrow.at);
            if !canvas.in_bounds(at.0, at.1)
                || !matches!(canvas.cells[canvas.idx(at.0, at.1)], Cell::Text(_))
            {
                failures.push(format!("edge {} arrow is not painted", edge.edge));
            }
            let distance =
                (arrow.at.x - arrow.toward.x).abs() + (arrow.at.y - arrow.toward.y).abs();
            if distance != 1 {
                failures.push(format!(
                    "edge {} arrow is not adjacent to its target",
                    edge.edge
                ));
            }
        } else if let Some(&end) = edge.points.last() {
            let end = point(end);
            if !canvas.in_bounds(end.0, end.1)
                || matches!(canvas.cells[canvas.idx(end.0, end.1)], Cell::Empty)
            {
                failures.push(format!("edge {} end is not painted", edge.edge));
            }
        }
    }

    failures
}

fn check_path(
    canvas: &Canvas,
    id: usize,
    points: &[Point],
    name: &str,
    failures: &mut Vec<String>,
) {
    if points.len() < 2 {
        failures.push(format!("{name} has fewer than two path points"));
        return;
    }
    for pair in points.windows(2) {
        if pair[0].x != pair[1].x && pair[0].y != pair[1].y {
            failures.push(format!(
                "path {id} has diagonal segment {:?} -> {:?}",
                pair[0], pair[1]
            ));
        }
    }
    for endpoint in [points[0], *points.last().unwrap()] {
        let endpoint = point(endpoint);
        if !canvas.in_bounds(endpoint.0, endpoint.1)
            || matches!(
                canvas.cells[canvas.idx(endpoint.0, endpoint.1)],
                Cell::Empty
            )
        {
            failures.push(format!("{name} endpoint is not painted"));
        }
    }
}

fn check_rect_corners(canvas: &Canvas, rect: Rect, name: &str, failures: &mut Vec<String>) {
    if rect.w < 2 || rect.h < 2 {
        failures.push(format!("{name} is smaller than 2x2"));
        return;
    }
    let x = rect.x as usize;
    let y = rect.y as usize;
    let right = (rect.right() - 1) as usize;
    let bottom = (rect.bottom() - 1) as usize;
    for (cx, cy, corner) in [
        (x, y, "TL"),
        (right, y, "TR"),
        (x, bottom, "BL"),
        (right, bottom, "BR"),
    ] {
        if !canvas.in_bounds(cx, cy) || matches!(canvas.cells[canvas.idx(cx, cy)], Cell::Empty) {
            failures.push(format!("{name} open {corner} corner at ({cx},{cy})"));
        }
    }
}

fn check_text(
    canvas: &Canvas,
    text: &crate::scene::SceneText,
    name: &str,
    failures: &mut Vec<String>,
) {
    let mut x = text.at.x as usize;
    let y = text.at.y as usize;
    for expected in text.text.chars() {
        if !canvas.in_bounds(x, y) {
            failures.push(format!("{name} {expected:?} is out of bounds at ({x},{y})"));
        } else if !matches!(canvas.cells[canvas.idx(x, y)], Cell::Text(got) if got == expected) {
            failures.push(format!("{name} {expected:?} overwritten at ({x},{y})"));
        }
        x += expected.width().unwrap_or(1).max(1);
    }
}

/// Render and verify B14/B16 invariants (closed borders, labels intact, edge
/// endpoints marked, non-endpoint boxes disjoint from paths). Returns the
/// diagram plus any invariant failures.
pub fn render_with_checks(g: &Graph, placed: &Placed, style: Style) -> (String, Vec<String>) {
    render_scene_with_checks(&route::route(g, placed), style)
}

fn draw_scene_group(canvas: &mut Canvas, group: &SceneGroup) {
    let Rect { x, y, w, h } = group.rect;
    draw_group(
        canvas,
        x as usize,
        y as usize,
        w as usize,
        h as usize,
        group.title.at.x as usize,
        group.title.at.y as usize,
        &group.title.text,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_group(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    title_x: usize,
    title_y: usize,
    title: &str,
) {
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
    let tw = title.width();
    if tw > 0 && h >= 3 && tw <= w.saturating_sub(2) {
        let mut ok = true;
        for dx in 0..tw {
            if !canvas.in_bounds(title_x + dx, title_y)
                || !matches!(canvas.cells[canvas.idx(title_x + dx, title_y)], Cell::Empty)
            {
                ok = false;
                break;
            }
        }
        if ok {
            canvas.put_text(title_x, title_y, title);
        }
    }
}

fn draw_scene_box(canvas: &mut Canvas, b: &SceneBox, style: Style) {
    let Rect { x, y, w, h } = b.rect;
    draw_box_at(
        canvas, x as usize, y as usize, w as usize, h as usize, &b.lines, b.shape, style,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_box_at(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    lines: &[String],
    shape: Shape,
    style: Style,
) {
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
    let text_y = y + h.saturating_sub(lines.len()) / 2;
    for (i, line) in lines.iter().enumerate() {
        let text_w = line.width();
        let start = x + 1 + inner_w.saturating_sub(text_w) / 2;
        canvas.put_text(start, text_y + i, line);
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
            canvas.put_text_char(x, mid_y, '(');
            canvas.put_text_char(x + w - 1, mid_y, ')');
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

fn arrow_toward(
    from: (usize, usize),
    to: (usize, usize),
    head: crate::scene::ArrowHead,
    style: Style,
) -> char {
    if to.0 > from.0 {
        style.arrow_right(head)
    } else if to.0 < from.0 {
        style.arrow_left(head)
    } else if to.1 > from.1 {
        style.arrow_down(head)
    } else {
        style.arrow_up(head)
    }
}
