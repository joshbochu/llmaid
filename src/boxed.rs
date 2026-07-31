//! Semantic-free adapter for diagrams made of labeled boxes and connections.
//!
//! Type-specific engines lower their own IR into this builder. The adapter
//! deliberately accepts structured nodes and edges rather than Mermaid text,
//! then reuses the deterministic flowchart layout and routing geometry.

use crate::layout;
use crate::parse::{Dir, Edge, Graph, Node};
use crate::route;
use crate::scene::{
    EdgeKind, EndpointDecoration, EndpointDecorationKind, Point, Scene, SceneText, Shape,
};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeEnd {
    Source,
    Target,
}

/// Stable declaration-order handle returned by [`BoxDiagram::add_node`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(usize);

impl NodeId {
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

/// A semantic-free box. `id` is for stable identity; `label` is displayed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoxNode {
    pub id: String,
    pub label: String,
    pub shape: Shape,
}

impl BoxNode {
    pub fn new(id: impl Into<String>, label: impl Into<String>, shape: Shape) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            shape,
        }
    }

    pub fn rect(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self::new(id, label, Shape::Rect)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BoxEdge {
    from: NodeId,
    to: NodeId,
    kind: EdgeKind,
    arrow: bool,
    label: Option<String>,
    endpoint_reserve: usize,
}

/// Builder returned for an edge. Mutations preserve its declaration position.
pub struct EdgeBuilder<'a> {
    edge: &'a mut BoxEdge,
}

impl EdgeBuilder<'_> {
    pub fn kind(&mut self, kind: EdgeKind) -> &mut Self {
        self.edge.kind = kind;
        self
    }

    pub fn with_arrow(&mut self) -> &mut Self {
        self.edge.arrow = true;
        self
    }

    pub fn without_arrow(&mut self) -> &mut Self {
        self.edge.arrow = false;
        self
    }

    pub fn label(&mut self, label: impl Into<String>) -> &mut Self {
        self.edge.label = Some(label.into());
        self
    }

    pub fn endpoint_spacing(&mut self, cells: usize) -> &mut Self {
        self.edge.endpoint_reserve = cells;
        self
    }
}

/// Ordered boxed diagram input to the shared integer layout/router.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoxDiagram {
    direction: Dir,
    nodes: Vec<BoxNode>,
    edges: Vec<BoxEdge>,
}

impl BoxDiagram {
    pub fn new(direction: Dir) -> Self {
        Self {
            direction,
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub fn direction(&self) -> Dir {
        self.direction
    }

    pub fn add_node(&mut self, node: BoxNode) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(node);
        id
    }

    /// Adds a solid arrow in declaration order and returns a property builder.
    pub fn add_edge(&mut self, from: NodeId, to: NodeId) -> EdgeBuilder<'_> {
        assert!(
            from.0 < self.nodes.len() && to.0 < self.nodes.len(),
            "boxed edge endpoints must belong to this diagram"
        );
        self.edges.push(BoxEdge {
            from,
            to,
            kind: EdgeKind::Solid,
            arrow: true,
            label: None,
            endpoint_reserve: 0,
        });
        EdgeBuilder {
            edge: self.edges.last_mut().expect("edge was just inserted"),
        }
    }

    /// Runs the existing integer layout/router and returns a normalized scene.
    pub fn scene(&self, max_width: usize) -> Scene {
        let graph = self.as_graph();
        let placed = layout::layout(&graph, max_width);
        route::route(&graph, &placed)
    }

    /// Runs compact spacing under pressure while preserving node line widths.
    ///
    /// Structured tables use placeholder labels for geometry and replace them
    /// with aligned cells after routing, so ordinary word wrapping would make
    /// their final paint wider than the box that was measured.
    pub fn scene_preserving_labels(&self, max_width: usize) -> Scene {
        let graph = self.as_graph();
        let placed = layout::layout_preserving_labels(&graph, max_width);
        route::route(&graph, &placed)
    }

    fn as_graph(&self) -> Graph {
        let mut graph = Graph::default();
        graph.dir = Some(self.direction);
        graph.nodes = self
            .nodes
            .iter()
            .map(|node| Node {
                id: node.id.clone(),
                label: node.label.clone(),
                shape: node.shape,
            })
            .collect();
        graph.edges = self
            .edges
            .iter()
            .map(|edge| Edge {
                from: edge.from.0,
                to: edge.to.0,
                kind: edge.kind,
                arrow: edge.arrow,
                label: edge.label.clone(),
                endpoint_reserve: edge.endpoint_reserve,
            })
            .collect();
        graph
    }
}

/// Attach a paint-level relationship glyph one cell outside an edge endpoint.
pub fn decorate_endpoint(
    scene: &mut Scene,
    edge_index: usize,
    end: EdgeEnd,
    kind: EndpointDecorationKind,
) {
    let Some((at, toward)) = endpoint_anchor(scene, edge_index, end) else {
        return;
    };
    scene.endpoint_decorations.push(EndpointDecoration {
        edge: edge_index,
        at,
        toward,
        kind,
    });
}

/// Place endpoint metadata (such as a UML multiplicity) beside, rather than
/// inside, the centered relationship label.
pub fn annotate_endpoint(scene: &mut Scene, edge_index: usize, end: EdgeEnd, text: &str) {
    let Some((at, toward)) = endpoint_anchor(scene, edge_index, end) else {
        return;
    };
    let text_width = text.width() as i32;
    let position = if at.x != toward.x {
        let away = (at.x - toward.x).signum();
        let x = if away > 0 {
            at.x + 1
        } else {
            at.x - text_width - 1
        };
        Point::new(x, at.y - 1)
    } else {
        Point::new(at.x + 2, at.y)
    };
    scene.texts.push(SceneText::new(position, text));
}

fn endpoint_anchor(scene: &Scene, edge_index: usize, end: EdgeEnd) -> Option<(Point, Point)> {
    let edge = scene.edges.iter().find(|edge| edge.edge == edge_index)?;
    let cells = crate::scene::path_cells(&edge.points);
    match end {
        EdgeEnd::Source => {
            let toward = *cells.first()?;
            let at = *cells.get(2).or_else(|| cells.get(1))?;
            Some((at, toward))
        }
        EdgeEnd::Target => {
            let toward = edge
                .arrow
                .as_ref()
                .map(|arrow| arrow.toward)
                .or_else(|| cells.last().copied())?;
            let at = if edge.arrow.is_some() {
                let previous = *edge.points.last()?;
                let before = *edge.points.get(edge.points.len().checked_sub(2)?)?;
                step_toward(previous, before)
            } else {
                *cells
                    .get(cells.len().checked_sub(3)?)
                    .or_else(|| cells.get(cells.len().saturating_sub(2)))?
            };
            Some((at, toward))
        }
    }
}

fn step_toward(from: Point, to: Point) -> Point {
    Point::new(
        from.x + (to.x - from.x).signum(),
        from.y + (to.y - from.y).signum(),
    )
}
