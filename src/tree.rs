//! Reusable deterministic integer-grid layout for ordered rooted trees.
//!
//! The layout is semantic-free: callers supply declaration-ordered nodes,
//! parent indices, and measured box sizes. Nodes occupy left-to-right depth
//! columns while source-ordered sibling subtrees occupy top-to-bottom rows.

use crate::scene::{Point, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeNode {
    pub parent: Option<usize>,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeConnection {
    pub parent: usize,
    pub child: usize,
    pub points: Vec<Point>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeLayout {
    pub boxes: Vec<Rect>,
    pub depths: Vec<usize>,
    pub connections: Vec<TreeConnection>,
}

/// Lay out a preorder tree in integer terminal cells.
///
/// Every depth receives one stable-width column. Leaf centers are separated by
/// an even stride, so each parent can sit at the exact integer midpoint of its
/// first and last child without float rounding or source-order drift.
pub fn layout(nodes: &[TreeNode], depth_gap: i32) -> TreeLayout {
    if nodes.is_empty() {
        return TreeLayout {
            boxes: Vec::new(),
            depths: Vec::new(),
            connections: Vec::new(),
        };
    }
    assert!(nodes[0].parent.is_none(), "tree node 0 must be the root");
    assert!(depth_gap >= 1, "tree depth gap must be positive");

    let mut depths = vec![0usize; nodes.len()];
    let mut children = vec![Vec::new(); nodes.len()];
    for (index, node) in nodes.iter().enumerate().skip(1) {
        let parent = node
            .parent
            .expect("every non-root tree node needs a parent");
        assert!(parent < index, "tree parents must precede their children");
        depths[index] = depths[parent] + 1;
        children[parent].push(index);
    }

    let levels = depths.iter().copied().max().unwrap_or(0) + 1;
    let mut column_widths = vec![1i32; levels];
    let mut max_height = 1i32;
    for (index, node) in nodes.iter().enumerate() {
        assert!(node.width >= 3 && node.height >= 3);
        column_widths[depths[index]] = column_widths[depths[index]].max(node.width);
        max_height = max_height.max(node.height);
    }

    let leaf_stride = if (max_height + 1) % 2 == 0 {
        max_height + 1
    } else {
        max_height + 2
    };
    let first_center = max_height / 2;
    let mut centers = vec![0i32; nodes.len()];
    let mut leaf = 0i32;
    for index in 0..nodes.len() {
        if children[index].is_empty() {
            centers[index] = first_center + leaf * leaf_stride;
            leaf += 1;
        }
    }
    for index in (0..nodes.len()).rev() {
        if let (Some(first), Some(last)) = (children[index].first(), children[index].last()) {
            centers[index] = (centers[*first] + centers[*last]) / 2;
        }
    }

    let mut column_x = vec![0i32; levels];
    for depth in 1..levels {
        column_x[depth] = column_x[depth - 1] + column_widths[depth - 1] + depth_gap;
    }
    let boxes: Vec<Rect> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let height = if node.height % 2 == 0 {
                node.height + 1
            } else {
                node.height
            };
            Rect::new(
                column_x[depths[index]],
                centers[index] - height / 2,
                column_widths[depths[index]],
                height,
            )
        })
        .collect();

    let mut connections = Vec::with_capacity(nodes.len().saturating_sub(1));
    for child in 1..nodes.len() {
        let parent = nodes[child].parent.unwrap();
        let source = Point::new(boxes[parent].right() - 1, centers[parent]);
        let target = Point::new(boxes[child].x, centers[child]);
        let points = if source.y == target.y {
            vec![source, target]
        } else {
            let trunk_x = source.x + (target.x - source.x) / 2;
            vec![
                source,
                Point::new(trunk_x, source.y),
                Point::new(trunk_x, target.y),
                target,
            ]
        };
        connections.push(TreeConnection {
            parent,
            child,
            points,
        });
    }

    TreeLayout {
        boxes,
        depths,
        connections,
    }
}
