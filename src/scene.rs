//! Screen-space geometry shared by diagram engines and the terminal painter.
//!
//! Coordinates are signed so layout and routing may extend left or above the
//! origin. `Scene::normalize` performs the only translation to canvas space.

use crate::parse::{EdgeKind, Shape};
use unicode_width::UnicodeWidthStr;

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

    fn bounds(&self) -> Rect {
        Rect::new(self.at.x, self.at.y, self.text.width() as i32, 1)
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneGroup {
    pub subgraph: usize,
    pub rect: Rect,
    pub title: SceneText,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arrow {
    pub at: Point,
    pub toward: Point,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scene {
    pub boxes: Vec<SceneBox>,
    pub groups: Vec<SceneGroup>,
    pub edges: Vec<RoutedEdge>,
}

impl Scene {
    pub fn bounds(&self) -> Rect {
        let mut bounds = Rect::default();
        for b in &self.boxes {
            bounds = bounds.union(b.rect);
        }
        for group in &self.groups {
            bounds = bounds.union(group.rect);
            bounds = bounds.union(group.title.bounds());
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
        for group in &mut self.groups {
            group.rect = group.rect.translated(dx, dy);
            group.title.translate(dx, dy);
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
        let after = self.bounds();
        (after.w.max(0) as usize, after.h.max(0) as usize)
    }
}
