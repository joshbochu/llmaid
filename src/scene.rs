//! Screen-space geometry shared by diagram engines and the terminal painter.
//!
//! Coordinates are signed so layout and routing may extend left or above the
//! origin. `Scene::normalize` performs the only translation to canvas space.

use unicode_width::UnicodeWidthStr;

/// Terminal box appearance shared by diagram engines and the painter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Rect,
    Rounded,
    Stadium,
    Circle,
    Cylinder,
    Diamond,
    Hexagon,
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

/// Terminal line appearance shared by diagram engines and the painter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    Solid,
    Dotted,
    Thick,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    fn translated(self, dx: i32, dy: i32) -> Self {
        Self::new(self.x + dx, self.y + dy)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub const fn right(self) -> i32 {
        self.x + self.w
    }

    pub const fn bottom(self) -> i32 {
        self.y + self.h
    }

    pub const fn contains(self, point: Point) -> bool {
        point.x >= self.x && point.x < self.right() && point.y >= self.y && point.y < self.bottom()
    }

    pub const fn center2(self) -> Point {
        Point::new(2 * self.x + self.w - 1, 2 * self.y + self.h - 1)
    }

    fn translated(self, dx: i32, dy: i32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w, self.h)
    }

    pub fn union(self, other: Self) -> Self {
        if self.w <= 0 || self.h <= 0 {
            return other;
        }
        if other.w <= 0 || other.h <= 0 {
            return self;
        }
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self::new(
            x,
            y,
            self.right().max(other.right()) - x,
            self.bottom().max(other.bottom()) - y,
        )
    }

    fn containing(point: Point) -> Self {
        Self::new(point.x, point.y, 1, 1)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneText {
    pub at: Point,
    pub text: String,
}

impl SceneText {
    pub fn new(at: Point, text: impl Into<String>) -> Self {
        Self {
            at,
            text: text.into(),
        }
    }

    pub fn width(&self) -> usize {
        self.text
            .split('\n')
            .map(UnicodeWidthStr::width)
            .max()
            .unwrap_or(0)
    }

    pub fn height(&self) -> usize {
        self.text.split('\n').count()
    }

    pub(crate) fn bounds(&self) -> Rect {
        Rect::new(
            self.at.x,
            self.at.y,
            self.width() as i32,
            self.height() as i32,
        )
    }

    fn translate(&mut self, dx: i32, dy: i32) {
        self.at = self.at.translated(dx, dy);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneBox {
    pub node: usize,
    pub rect: Rect,
    pub lines: Vec<String>,
    pub shape: Shape,
    pub table: Option<SceneTable>,
}

/// Structured box content used by class and ER diagrams. The title spans the
/// full box; rows use stable, padded columns beneath a horizontal divider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneTable {
    pub title: String,
    pub rows: Vec<Vec<String>>,
    pub row_dividers: bool,
}

impl SceneTable {
    pub fn new(title: impl Into<String>, rows: Vec<Vec<String>>) -> Self {
        Self {
            title: title.into(),
            rows,
            row_dividers: false,
        }
    }

    pub fn with_row_dividers(mut self) -> Self {
        self.row_dividers = true;
        self
    }

    pub fn layout_label(&self) -> String {
        if self.rows.is_empty() {
            return self.title.clone();
        }
        let width = self.title.width().max(self.grid_width()).max(1);
        let placeholder = " ".repeat(width);
        let dividers = if self.row_dividers {
            self.rows.len().saturating_sub(1)
        } else {
            0
        };
        let mut lines = Vec::with_capacity(self.rows.len() + dividers + 2);
        lines.push(self.title.clone());
        lines.push(String::new());
        for row in 0..self.rows.len() {
            lines.push(placeholder.clone());
            if self.row_dividers && row + 1 < self.rows.len() {
                lines.push(String::new());
            }
        }
        lines.join("\n")
    }

    pub(crate) fn column_widths(&self) -> Vec<usize> {
        let columns = self.rows.iter().map(Vec::len).max().unwrap_or(0);
        (0..columns)
            .map(|column| {
                self.rows
                    .iter()
                    .filter_map(|row| row.get(column))
                    .map(|cell| cell.width())
                    .max()
                    .unwrap_or(0)
                    + 2
            })
            .collect()
    }

    pub(crate) fn grid_width(&self) -> usize {
        let widths = self.column_widths();
        widths.iter().sum::<usize>() + widths.len().saturating_sub(1)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneGroup {
    pub subgraph: usize,
    pub rect: Rect,
    pub title: SceneText,
    /// Labeled horizontal subdivisions contained by this frame. Sequence
    /// `alt` branches use these without teaching the renderer diagram syntax.
    pub separators: Vec<SceneGroupSeparator>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneGroupSeparator {
    /// Absolute row of the horizontal stroke. The label occupies cells on the
    /// same row, leaving the stroke visible on both sides.
    pub y: i32,
    pub label: SceneText,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrowHead {
    Filled,
    Open,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arrow {
    pub at: Point,
    pub toward: Point,
    pub head: ArrowHead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardinalityMinimum {
    Zero,
    One,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardinalityMaximum {
    One,
    Many,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointDecorationKind {
    /// A filled directional flowchart arrowhead at a source-side terminal.
    Arrow,
    /// Mermaid flowchart terminal circle (`--o`).
    Circle,
    /// Mermaid flowchart terminal cross (`--x`).
    Cross,
    OpenArrow,
    OpenTriangle,
    OpenDiamond,
    FilledDiamond,
    Cardinality {
        minimum: CardinalityMinimum,
        maximum: CardinalityMaximum,
    },
}

/// Paint-level relationship adornment anchored one cell outside a box. `toward`
/// points from the anchor back toward that endpoint's box border.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndpointDecoration {
    pub edge: usize,
    pub at: Point,
    pub toward: Point,
    pub kind: EndpointDecorationKind,
}

impl EndpointDecoration {
    fn away(&self) -> Point {
        Point::new(
            self.at.x + (self.at.x - self.toward.x).signum(),
            self.at.y + (self.at.y - self.toward.y).signum(),
        )
    }

    pub(crate) fn paint_cells(&self) -> Vec<Point> {
        if !matches!(self.kind, EndpointDecorationKind::Cardinality { .. }) {
            return vec![self.at];
        }
        let away = self.away();
        vec![
            self.at,
            Point::new(
                away.x + (away.x - self.at.x).signum(),
                away.y + (away.y - self.at.y).signum(),
            ),
        ]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutedEdge {
    pub edge: usize,
    pub points: Vec<Point>,
    pub rounded: Vec<Point>,
    pub kind: EdgeKind,
    pub label: Option<SceneText>,
    pub arrow: Option<Arrow>,
}

/// A non-semantic path such as a sequence-diagram lifeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScenePath {
    pub path: usize,
    pub points: Vec<Point>,
    pub rounded: Vec<Point>,
    pub kind: EdgeKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EdgeBoxIntersection {
    pub edge: usize,
    pub node: usize,
    pub at: Point,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scene {
    pub boxes: Vec<SceneBox>,
    /// Boxes painted after non-semantic paths and before semantic edges.
    pub foreground_boxes: Vec<SceneBox>,
    pub groups: Vec<SceneGroup>,
    pub paths: Vec<ScenePath>,
    pub edges: Vec<RoutedEdge>,
    pub endpoint_decorations: Vec<EndpointDecoration>,
    pub texts: Vec<SceneText>,
}

impl Scene {
    pub fn bounds(&self) -> Rect {
        let mut bounds = Rect::default();
        for b in &self.boxes {
            bounds = bounds.union(b.rect);
        }
        for b in &self.foreground_boxes {
            bounds = bounds.union(b.rect);
        }
        for group in &self.groups {
            bounds = bounds.union(group.rect);
            bounds = bounds.union(group.title.bounds());
            for separator in &group.separators {
                bounds = bounds.union(separator.label.bounds());
            }
        }
        for path in &self.paths {
            for &point in path.points.iter().chain(&path.rounded) {
                bounds = bounds.union(Rect::containing(point));
            }
        }
        for edge in &self.edges {
            for &point in edge.points.iter().chain(&edge.rounded) {
                bounds = bounds.union(Rect::containing(point));
            }
            if let Some(label) = &edge.label {
                bounds = bounds.union(label.bounds());
            }
            if let Some(arrow) = &edge.arrow {
                bounds = bounds.union(Rect::containing(arrow.at));
                bounds = bounds.union(Rect::containing(arrow.toward));
            }
        }
        for decoration in &self.endpoint_decorations {
            for cell in decoration.paint_cells() {
                bounds = bounds.union(Rect::containing(cell));
            }
        }
        for text in &self.texts {
            bounds = bounds.union(text.bounds());
        }
        bounds
    }

    /// Translate all geometry so the scene begins at `(0, 0)`, returning its
    /// exact `(width, height)` after translation.
    pub fn normalize(&mut self) -> (usize, usize) {
        let before = self.bounds();
        let dx = -before.x;
        let dy = -before.y;
        for b in &mut self.boxes {
            b.rect = b.rect.translated(dx, dy);
        }
        for b in &mut self.foreground_boxes {
            b.rect = b.rect.translated(dx, dy);
        }
        for group in &mut self.groups {
            group.rect = group.rect.translated(dx, dy);
            group.title.translate(dx, dy);
            for separator in &mut group.separators {
                separator.y += dy;
                separator.label.translate(dx, dy);
            }
        }
        for path in &mut self.paths {
            for point in path.points.iter_mut().chain(&mut path.rounded) {
                *point = point.translated(dx, dy);
            }
        }
        for edge in &mut self.edges {
            for point in edge.points.iter_mut().chain(&mut edge.rounded) {
                *point = point.translated(dx, dy);
            }
            if let Some(label) = &mut edge.label {
                label.translate(dx, dy);
            }
            if let Some(arrow) = &mut edge.arrow {
                arrow.at = arrow.at.translated(dx, dy);
                arrow.toward = arrow.toward.translated(dx, dy);
            }
        }
        for decoration in &mut self.endpoint_decorations {
            decoration.at = decoration.at.translated(dx, dy);
            decoration.toward = decoration.toward.translated(dx, dy);
        }
        for text in &mut self.texts {
            text.translate(dx, dy);
        }
        let after = self.bounds();
        (after.w.max(0) as usize, after.h.max(0) as usize)
    }

    /// Return the first path cell where each edge intersects a box that is not
    /// one of that edge's geometric endpoints. Touching a non-endpoint border
    /// counts: it is visually indistinguishable from routing through the node.
    pub fn edge_box_intersections(&self) -> Vec<EdgeBoxIntersection> {
        let mut intersections = Vec::new();
        for edge in &self.edges {
            let Some(&source) = edge.points.first() else {
                continue;
            };
            let target = edge
                .arrow
                .as_ref()
                .map(|arrow| arrow.toward)
                .or_else(|| edge.points.last().copied())
                .unwrap_or(source);
            let endpoint_nodes: Vec<usize> = self
                .boxes
                .iter()
                .chain(&self.foreground_boxes)
                .filter(|box_| box_.rect.contains(source) || box_.rect.contains(target))
                .map(|box_| box_.node)
                .collect();
            let cells = path_cells(&edge.points);
            for box_ in self.boxes.iter().chain(&self.foreground_boxes) {
                if endpoint_nodes.contains(&box_.node) {
                    continue;
                }
                if let Some(&at) = cells.iter().find(|&&point| box_.rect.contains(point)) {
                    intersections.push(EdgeBoxIntersection {
                        edge: edge.edge,
                        node: box_.node,
                        at,
                    });
                }
            }
        }
        intersections
    }
}

pub(crate) fn path_cells(points: &[Point]) -> Vec<Point> {
    let mut cells = Vec::new();
    for pair in points.windows(2) {
        let mut current = pair[0];
        if cells.last() != Some(&current) {
            cells.push(current);
        }
        while current != pair[1] {
            current = Point::new(
                current.x + (pair[1].x - current.x).signum(),
                current.y + (pair[1].y - current.y).signum(),
            );
            cells.push(current);
        }
    }
    cells
}
