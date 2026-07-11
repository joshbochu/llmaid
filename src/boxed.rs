//! Semantic-free adapter for diagrams made of labeled boxes and connections.
//!
//! Type-specific engines lower their own IR into this builder. The adapter
//! deliberately accepts structured nodes and edges rather than Mermaid text,
//! then reuses the deterministic flowchart layout and routing geometry.

use crate::layout;
use crate::parse::{Dir, Edge, Graph, Node};
use crate::route;
use crate::scene::{EdgeKind, Scene, Shape};

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
            })
            .collect();
        graph
    }
}
