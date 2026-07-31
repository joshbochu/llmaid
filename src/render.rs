//! Rasterize screen-space `Scene` primitives onto a character canvas.

use crate::layout::Placed;
use crate::parse::Graph;
use crate::route;
use crate::scene::{
    CardinalityMaximum, CardinalityMinimum, EdgeKind, EndpointDecoration, EndpointDecorationKind,
    Point, Rect, RoutedEdge, Scene, SceneBox, SceneGroup, ScenePath, SceneText, Shape,
};
use crate::style::{E, N, S, Style, W};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug)]
enum Cell {
    Empty,
    Line {
        bits: u8,
        rounded: bool,
        kind: EdgeKind,
    },
    Text {
        run: usize,
    },
    WideCont {
        run: usize,
    },
}

#[derive(Debug)]
struct TextRun {
    text: String,
    x: usize,
    y: usize,
    width: usize,
}

struct Canvas {
    w: usize,
    h: usize,
    cells: Vec<Cell>,
    text_runs: Vec<TextRun>,
}

impl Canvas {
    fn new(w: usize, h: usize) -> Canvas {
        Canvas {
            w,
            h,
            cells: vec![Cell::Empty; w.saturating_mul(h)],
            text_runs: Vec::new(),
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
            Cell::Text { .. } | Cell::WideCont { .. } => self.cells[ix],
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
        let mut encoded = [0; 4];
        self.put_text(x, y, ch.encode_utf8(&mut encoded));
    }

    fn put_text_run(&mut self, x: usize, y: usize, text: &str, width: usize) {
        if width == 0 || !self.in_bounds(x, y) {
            return;
        }

        // Replacing any cell of a wide grapheme removes that entire grapheme.
        // Leaving its start behind would make the terminal print more columns
        // than the canvas owns; leaving a continuation behind would suppress a
        // real canvas cell.
        let mut replaced = Vec::new();
        for dx in 0..width {
            if !self.in_bounds(x + dx, y) {
                continue;
            }
            let owner = match self.cells[self.idx(x + dx, y)] {
                Cell::Text { run } | Cell::WideCont { run } => Some(run),
                _ => None,
            };
            if let Some(run) = owner
                && !replaced.contains(&run)
            {
                replaced.push(run);
            }
        }
        for run in replaced {
            let old = &self.text_runs[run];
            let (old_x, old_y, old_width) = (old.x, old.y, old.width);
            for dx in 0..old_width {
                if self.in_bounds(old_x + dx, old_y) {
                    let old_ix = self.idx(old_x + dx, old_y);
                    if matches!(
                        self.cells[old_ix],
                        Cell::Text { run: owner } | Cell::WideCont { run: owner }
                            if owner == run
                    ) {
                        self.cells[old_ix] = Cell::Empty;
                    }
                }
            }
        }

        let run = self.text_runs.len();
        self.text_runs.push(TextRun {
            text: text.to_string(),
            x,
            y,
            width,
        });
        let ix = self.idx(x, y);
        self.cells[ix] = Cell::Text { run };
        for dx in 1..width {
            if self.in_bounds(x + dx, y) {
                let cont_ix = self.idx(x + dx, y);
                self.cells[cont_ix] = Cell::WideCont { run };
            }
        }
    }

    fn put_text(&mut self, mut x: usize, y: usize, text: &str) {
        for grapheme in text.graphemes(true) {
            // Never forward terminal controls or isolated zero-column runs.
            // Checked rendering reports the precise source failure; unchecked
            // library rendering remains terminal-safe by omitting it.
            if grapheme.chars().any(char::is_control) {
                continue;
            }
            let width = grapheme.width();
            if width == 0 {
                continue;
            }
            self.put_text_run(x, y, grapheme, width);
            x += width;
        }
    }

    fn put_scene_text(&mut self, text: &SceneText) {
        for (line, value) in text.text.split('\n').enumerate() {
            self.put_text(text.at.x as usize, text.at.y as usize + line, value);
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
                    Cell::Text { run } => row.push_str(&self.text_runs[run].text),
                    Cell::WideCont { .. } => {}
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
    for decoration in &scene.endpoint_decorations {
        draw_endpoint_decoration(&mut canvas, decoration, style);
    }
    for text in &scene.texts {
        canvas.put_scene_text(text);
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
        canvas.put_scene_text(label);
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

    for group in &scene.groups {
        check_rect_border(
            canvas,
            group.rect,
            &format!("group {}", group.subgraph),
            |_, _, _| false,
            |_, _| false,
            &mut failures,
        );
        check_single_line_text(
            canvas,
            &group.title,
            &format!("group {} title", group.subgraph),
            &mut failures,
        );
        for (separator_index, separator) in group.separators.iter().enumerate() {
            let name = format!("group {} separator {separator_index}", group.subgraph);
            check_group_separator(canvas, group, separator, &name, &mut failures);
        }
    }

    for b in &scene.boxes {
        check_rect_border(
            canvas,
            b.rect,
            &format!("box {}", b.node),
            |x, y, text| is_shape_hint(b, x, y, text),
            |_, _| false,
            &mut failures,
        );
        if b.table.is_some() {
            for text in table_texts(b) {
                check_single_line_text(
                    canvas,
                    &text,
                    &format!("box {} table", b.node),
                    &mut failures,
                );
            }
        } else {
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
                check_single_line_text(
                    canvas,
                    &text,
                    &format!("box {} label", b.node),
                    &mut failures,
                );
            }
        }
    }

    for (box_index, b) in scene.foreground_boxes.iter().enumerate() {
        check_rect_border(
            canvas,
            b.rect,
            &format!("foreground box {}", b.node),
            |x, y, text| is_shape_hint(b, x, y, text),
            |x, y| {
                let point = Point::new(x as i32, y as i32);
                scene.foreground_boxes[box_index + 1..]
                    .iter()
                    .any(|later| later.rect.contains(point))
            },
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
            check_single_line_text(
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
                || !matches!(canvas.cells[canvas.idx(at.0, at.1)], Cell::Text { .. })
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

    for decoration in &scene.endpoint_decorations {
        for at in decoration_cells(decoration) {
            let (x, y) = point(at);
            if !canvas.in_bounds(x, y)
                || !matches!(canvas.cells[canvas.idx(x, y)], Cell::Text { .. })
            {
                failures.push(format!(
                    "edge {} endpoint decoration is not painted at ({x},{y})",
                    decoration.edge
                ));
            }
        }
    }

    for (index, text) in scene.texts.iter().enumerate() {
        check_text(canvas, text, &format!("scene text {index}"), &mut failures);
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

fn check_rect_border(
    canvas: &Canvas,
    rect: Rect,
    name: &str,
    allowed_text: impl Fn(usize, usize, &str) -> bool,
    occluded: impl Fn(usize, usize) -> bool,
    failures: &mut Vec<String>,
) {
    if rect.w < 2 || rect.h < 2 {
        failures.push(format!("{name} is smaller than 2x2"));
        return;
    }
    let x = rect.x as usize;
    let y = rect.y as usize;
    let right = (rect.right() - 1) as usize;
    let bottom = (rect.bottom() - 1) as usize;

    let mut border = Vec::new();
    for cx in x..=right {
        border.push((
            cx,
            y,
            if cx == x {
                "TL corner"
            } else if cx == right {
                "TR corner"
            } else {
                "top border"
            },
            if cx == x {
                E | S
            } else if cx == right {
                S | W
            } else {
                E | W
            },
        ));
        border.push((
            cx,
            bottom,
            if cx == x {
                "BL corner"
            } else if cx == right {
                "BR corner"
            } else {
                "bottom border"
            },
            if cx == x {
                N | E
            } else if cx == right {
                N | W
            } else {
                E | W
            },
        ));
    }
    for cy in y + 1..bottom {
        border.push((x, cy, "left border", N | S));
        border.push((right, cy, "right border", N | S));
    }

    for (cx, cy, part, required) in border {
        let closed = occluded(cx, cy)
            || canvas.in_bounds(cx, cy)
                && match canvas.cells[canvas.idx(cx, cy)] {
                    Cell::Line { bits, .. } => bits & required == required,
                    Cell::Text { run } => allowed_text(cx, cy, &canvas.text_runs[run].text),
                    Cell::Empty | Cell::WideCont { .. } => false,
                };
        if !closed {
            failures.push(format!(
                "{name} incomplete {part} at ({cx},{cy}); required border directions are missing"
            ));
        }
    }
}

fn cell_has_line_bits(canvas: &Canvas, x: usize, y: usize, required: u8) -> bool {
    canvas.in_bounds(x, y)
        && matches!(
            canvas.cells[canvas.idx(x, y)],
            Cell::Line { bits, .. } if bits & required == required
        )
}

fn is_shape_hint(b: &SceneBox, x: usize, y: usize, text: &str) -> bool {
    let left = b.rect.x as usize;
    let top = b.rect.y as usize;
    let right = (b.rect.right() - 1) as usize;
    let bottom = (b.rect.bottom() - 1) as usize;
    let mid_y = top + b.rect.h as usize / 2;
    match b.shape {
        Shape::Rect | Shape::Rounded => false,
        Shape::Stadium | Shape::Circle => {
            (x, y) == (left, mid_y) && text == "(" || (x, y) == (right, mid_y) && text == ")"
        }
        Shape::Cylinder => y == top && x > left && x < right && matches!(text, "=" | "═"),
        Shape::Diamond => {
            (x == left || x == right) && (y == top || y == bottom) && matches!(text, "*" | "◇")
        }
        Shape::Hexagon => {
            (x == left || x == right)
                && (y == top || y == bottom)
                && matches!(text, "/" | "\\" | "╱" | "╲")
        }
    }
}

fn check_text(
    canvas: &Canvas,
    text: &crate::scene::SceneText,
    name: &str,
    failures: &mut Vec<String>,
) {
    check_text_mode(canvas, text, name, true, failures);
}

fn check_single_line_text(
    canvas: &Canvas,
    text: &SceneText,
    name: &str,
    failures: &mut Vec<String>,
) {
    check_text_mode(canvas, text, name, false, failures);
}

fn check_text_mode(
    canvas: &Canvas,
    text: &SceneText,
    name: &str,
    allow_newlines: bool,
    failures: &mut Vec<String>,
) {
    for ch in text.text.chars() {
        if ch.is_control() && !(allow_newlines && ch == '\n') {
            failures.push(format!(
                "{name} contains terminal control U+{:04X}; remove it or use visible text",
                ch as u32
            ));
            return;
        }
    }

    for (line, value) in text.text.split('\n').enumerate() {
        let mut x = text.at.x as usize;
        let y = text.at.y as usize + line;
        for expected in value.graphemes(true) {
            let width = expected.width();
            if width == 0 {
                failures.push(format!(
                    "{name} contains unsupported zero-column grapheme {expected:?}"
                ));
                return;
            }
            if !canvas.in_bounds(x, y) {
                failures.push(format!("{name} {expected:?} is out of bounds at ({x},{y})"));
            } else {
                let run = match canvas.cells[canvas.idx(x, y)] {
                    Cell::Text { run } if canvas.text_runs[run].text == expected => Some(run),
                    _ => None,
                };
                if let Some(run) = run {
                    for dx in 1..width {
                        if !canvas.in_bounds(x + dx, y)
                            || !matches!(
                                canvas.cells[canvas.idx(x + dx, y)],
                                Cell::WideCont { run: owner } if owner == run
                            )
                        {
                            failures.push(format!(
                                "{name} {expected:?} continuation overwritten at ({},{y})",
                                x + dx
                            ));
                        }
                    }
                } else {
                    failures.push(format!("{name} {expected:?} overwritten at ({x},{y})"));
                }
            }
            x += width;
        }
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
    for separator in &group.separators {
        draw_group_separator(canvas, group.rect, separator);
    }
}

fn draw_group_separator(
    canvas: &mut Canvas,
    rect: Rect,
    separator: &crate::scene::SceneGroupSeparator,
) {
    if separator.y <= rect.y
        || separator.y >= rect.bottom() - 1
        || rect.w < 2
        || separator.y < 0
        || rect.x < 0
    {
        return;
    }
    let left = rect.x as usize;
    let right = (rect.right() - 1) as usize;
    let y = separator.y as usize;
    for x in left..=right {
        let bits = if x == left {
            N | E | S
        } else if x == right {
            N | S | W
        } else {
            E | W
        };
        canvas.add_line_bits(x, y, bits, EdgeKind::Solid);
    }
    canvas.put_scene_text(&separator.label);
}

fn check_group_separator(
    canvas: &Canvas,
    group: &SceneGroup,
    separator: &crate::scene::SceneGroupSeparator,
    name: &str,
    failures: &mut Vec<String>,
) {
    if separator.y <= group.rect.y || separator.y >= group.rect.bottom() - 1 {
        failures.push(format!("{name} is not inside its group frame"));
        return;
    }
    if separator.label.at.y != separator.y || separator.label.height() != 1 {
        failures.push(format!("{name} label is not on its separator row"));
        return;
    }
    let label_left = separator.label.at.x;
    let label_right = label_left + separator.label.width() as i32;
    if label_left <= group.rect.x || label_right >= group.rect.right() - 1 {
        failures.push(format!("{name} label is not padded inside its group frame"));
        return;
    }

    check_single_line_text(canvas, &separator.label, &format!("{name} label"), failures);
    let y = separator.y as usize;
    for x in group.rect.x..group.rect.right() {
        if x >= label_left && x < label_right {
            continue;
        }
        let required = if x == group.rect.x {
            N | E | S
        } else if x == group.rect.right() - 1 {
            N | S | W
        } else {
            E | W
        };
        let (x, y) = (x as usize, y);
        let painted = cell_has_line_bits(canvas, x, y, required);
        if !painted {
            failures.push(format!(
                "{name} is missing its horizontal stroke at ({x},{y})"
            ));
        }
    }
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
    // Merge frame directions into existing line cells. A connector crossing a
    // frame remains one continuous connector *and* one continuous frame,
    // producing the exact tee/crossing glyph from their combined bitmask.
    // Text remains untouched so checked rendering can report real collisions.
    let paint = |canvas: &mut Canvas, px: usize, py: usize, bits: u8| {
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
    let lines: &[String] = if b.table.is_some() { &[] } else { &b.lines };
    draw_box_at(
        canvas, x as usize, y as usize, w as usize, h as usize, lines, b.shape, style,
    );
    if b.table.is_some() {
        draw_scene_table(canvas, b);
    }
}

fn draw_scene_table(canvas: &mut Canvas, b: &SceneBox) {
    let Some(table) = &b.table else {
        return;
    };
    if !table.rows.is_empty() {
        let divider_y = b.rect.y + 2;
        draw_screen_line(
            canvas,
            (b.rect.x as usize, divider_y as usize),
            ((b.rect.right() - 1) as usize, divider_y as usize),
            EdgeKind::Solid,
        );

        let widths = table.column_widths();
        let grid_x = table_grid_x(b);
        let mut x = grid_x;
        for width in widths.iter().take(widths.len().saturating_sub(1)) {
            x += *width as i32;
            draw_screen_line(
                canvas,
                (x as usize, divider_y as usize),
                (x as usize, (b.rect.bottom() - 1) as usize),
                EdgeKind::Solid,
            );
            x += 1;
        }
        if table.row_dividers {
            for row in 1..table.rows.len() {
                let y = b.rect.y + 2 + (2 * row) as i32;
                draw_screen_line(
                    canvas,
                    (b.rect.x as usize, y as usize),
                    ((b.rect.right() - 1) as usize, y as usize),
                    EdgeKind::Solid,
                );
            }
        }
    }
    for text in table_texts(b) {
        canvas.put_text(text.at.x as usize, text.at.y as usize, &text.text);
    }
}

fn table_grid_x(b: &SceneBox) -> i32 {
    let table = b.table.as_ref().expect("table box");
    let inner_width = b.rect.w.saturating_sub(2);
    b.rect.x + 1 + (inner_width - table.grid_width() as i32).max(0) / 2
}

fn table_texts(b: &SceneBox) -> Vec<SceneText> {
    let Some(table) = &b.table else {
        return Vec::new();
    };
    let inner_width = b.rect.w.saturating_sub(2);
    let title_width = table.title.width() as i32;
    let mut texts = vec![SceneText::new(
        Point::new(
            b.rect.x + 1 + (inner_width - title_width).max(0) / 2,
            b.rect.y + 1,
        ),
        table.title.clone(),
    )];
    if table.rows.is_empty() {
        return texts;
    }

    let widths = table.column_widths();
    let grid_x = table_grid_x(b);
    for (row_index, row) in table.rows.iter().enumerate() {
        let mut x = grid_x;
        let row_y = b.rect.y
            + 3
            + if table.row_dividers {
                (2 * row_index) as i32
            } else {
                row_index as i32
            };
        for (column, width) in widths.iter().enumerate() {
            if let Some(cell) = row.get(column)
                && !cell.is_empty()
            {
                texts.push(SceneText::new(Point::new(x + 1, row_y), cell.clone()));
            }
            x += *width as i32 + 1;
        }
    }
    texts
}

fn draw_endpoint_decoration(canvas: &mut Canvas, decoration: &EndpointDecoration, style: Style) {
    match decoration.kind {
        EndpointDecorationKind::Cardinality { minimum, maximum } => {
            let minimum_glyph = match minimum {
                CardinalityMinimum::Zero => {
                    if style.ascii {
                        'o'
                    } else {
                        '○'
                    }
                }
                CardinalityMinimum::One => cardinality_bar(decoration, style),
            };
            let maximum_glyph = cardinality_maximum_glyph(decoration, maximum, style);
            let cells = decoration_cells(decoration);
            if decoration.at.x == decoration.toward.x {
                let minimum_at = point(cells[0]);
                let maximum_at = point(cells[1]);
                canvas.put_text_char(minimum_at.0, minimum_at.1, minimum_glyph);
                canvas.put_text_char(maximum_at.0, maximum_at.1, maximum_glyph);
            } else {
                let maximum_at = point(cells[0]);
                let minimum_at = point(cells[1]);
                canvas.put_text_char(maximum_at.0, maximum_at.1, maximum_glyph);
                canvas.put_text_char(minimum_at.0, minimum_at.1, minimum_glyph);
            }
        }
        kind => {
            let ch = match kind {
                EndpointDecorationKind::OpenArrow => arrow_toward(
                    point(decoration.at),
                    point(decoration.toward),
                    crate::scene::ArrowHead::Filled,
                    style,
                ),
                EndpointDecorationKind::OpenTriangle => {
                    triangle_toward(decoration.at, decoration.toward, style)
                }
                EndpointDecorationKind::OpenDiamond => {
                    if style.ascii {
                        'o'
                    } else {
                        '◇'
                    }
                }
                EndpointDecorationKind::FilledDiamond => {
                    if style.ascii {
                        '*'
                    } else {
                        '◆'
                    }
                }
                EndpointDecorationKind::Cardinality { .. } => unreachable!(),
            };
            let at = point(decoration.at);
            canvas.put_text_char(at.0, at.1, ch);
        }
    }
}

fn decoration_cells(decoration: &EndpointDecoration) -> Vec<Point> {
    decoration.paint_cells()
}

fn cardinality_bar(_decoration: &EndpointDecoration, style: Style) -> char {
    if style.ascii { '|' } else { '│' }
}

fn cardinality_maximum_glyph(
    decoration: &EndpointDecoration,
    maximum: CardinalityMaximum,
    style: Style,
) -> char {
    match maximum {
        CardinalityMaximum::One => cardinality_bar(decoration, style),
        CardinalityMaximum::Many if decoration.toward.x > decoration.at.x => '<',
        CardinalityMaximum::Many if decoration.toward.x < decoration.at.x => '>',
        CardinalityMaximum::Many => '<',
    }
}

fn triangle_toward(from: Point, to: Point, style: Style) -> char {
    if style.ascii {
        return arrow_toward(point(from), point(to), crate::scene::ArrowHead::Open, style);
    }
    if to.x > from.x {
        '▷'
    } else if to.x < from.x {
        '◁'
    } else if to.y > from.y {
        '▽'
    } else {
        '△'
    }
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

#[cfg(test)]
mod border_tests {
    use super::*;

    #[test]
    fn border_check_rejects_wrong_orientation_but_accepts_merged_crossings() {
        let mut canvas = Canvas::new(7, 5);
        draw_group(&mut canvas, 1, 1, 5, 3, 2, 2, "G");

        let top = canvas.idx(3, 1);
        let bottom = canvas.idx(3, 3);
        let left = canvas.idx(1, 2);
        let right = canvas.idx(5, 2);
        canvas.cells[top] = Cell::Line {
            bits: N | S,
            rounded: false,
            kind: EdgeKind::Solid,
        };
        canvas.cells[bottom] = canvas.cells[top];
        canvas.cells[left] = Cell::Line {
            bits: E | W,
            rounded: false,
            kind: EdgeKind::Solid,
        };
        canvas.cells[right] = canvas.cells[left];

        let mut failures = Vec::new();
        check_rect_border(
            &canvas,
            Rect::new(1, 1, 5, 3),
            "test frame",
            |_, _, _| false,
            |_, _| false,
            &mut failures,
        );
        for expected in ["top border", "bottom border", "left border", "right border"] {
            assert!(
                failures.iter().any(|failure| failure.contains(expected)),
                "missing {expected:?}: {failures:#?}"
            );
        }

        for index in [top, bottom, left, right] {
            canvas.cells[index] = Cell::Line {
                bits: N | E | S | W,
                rounded: false,
                kind: EdgeKind::Solid,
            };
        }
        let mut failures = Vec::new();
        check_rect_border(
            &canvas,
            Rect::new(1, 1, 5, 3),
            "test frame",
            |_, _, _| false,
            |_, _| false,
            &mut failures,
        );
        assert!(failures.is_empty(), "{failures:#?}");
    }
}
