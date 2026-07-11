use llmaid::scene::Rect;
use llmaid::tree::{self, TreeNode};

#[test]
fn reusable_tree_layout_is_deterministic_ordered_and_semantic_free() {
    let input = [
        TreeNode {
            parent: None,
            width: 7,
            height: 3,
        },
        TreeNode {
            parent: Some(0),
            width: 5,
            height: 3,
        },
        TreeNode {
            parent: Some(1),
            width: 9,
            height: 5,
        },
        TreeNode {
            parent: Some(0),
            width: 3,
            height: 3,
        },
    ];
    let first = tree::layout(&input, 4);
    assert_eq!(first, tree::layout(&input, 4));
    assert_eq!(first.depths, [0, 1, 2, 1]);
    assert_eq!(first.boxes[1].x, first.boxes[3].x);
    assert_eq!(first.boxes[1].w, first.boxes[3].w);
    assert!(first.boxes[1].center2().y < first.boxes[3].center2().y);
    assert_eq!(
        2 * first.boxes[0].center2().y,
        first.boxes[1].center2().y + first.boxes[3].center2().y
    );
    assert_eq!(first.connections.len(), 3);
    assert!(first.connections.iter().all(|edge| {
        edge.points
            .windows(2)
            .all(|pair| pair[0].x == pair[1].x || pair[0].y == pair[1].y)
    }));
    assert!(first.boxes.iter().all(|rect| *rect != Rect::default()));
}
