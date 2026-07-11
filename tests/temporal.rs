use llmaid::temporal::{self, Extent, TemporalBand, TemporalEntry, TemporalGaps};

fn extent(width: i32, height: i32) -> Extent {
    Extent { width, height }
}

#[test]
fn reusable_temporal_layout_is_deterministic_ordered_aligned_and_semantic_free() {
    let entries = [
        TemporalEntry {
            leading: extent(7, 3),
            trailing: vec![extent(9, 3), extent(5, 3)],
            band: Some(0),
        },
        TemporalEntry {
            leading: extent(5, 3),
            trailing: vec![extent(11, 5)],
            band: Some(0),
        },
        TemporalEntry {
            leading: extent(9, 5),
            trailing: vec![extent(7, 3)],
            band: Some(1),
        },
    ];
    let bands = [
        TemporalBand {
            first_entry: 0,
            entry_count: 2,
            title_width: 10,
        },
        TemporalBand {
            first_entry: 2,
            entry_count: 1,
            title_width: 8,
        },
    ];
    let first = temporal::layout(&entries, &bands, TemporalGaps::normal());
    assert_eq!(
        first,
        temporal::layout(&entries, &bands, TemporalGaps::normal())
    );

    assert!(first.anchors.windows(2).all(|pair| pair[0].y < pair[1].y));
    assert!(first.anchors.iter().all(|anchor| anchor.x == first.spine_x));
    assert!(
        first
            .leading_boxes
            .iter()
            .zip(&first.anchors)
            .all(|(box_, anchor)| {
                box_.right() - 1 < anchor.x && box_.center2().y == 2 * anchor.y
            })
    );
    for (entry, boxes) in first.trailing_boxes.iter().enumerate() {
        for box_ in boxes {
            assert!(box_.x > first.anchors[entry].x);
        }
    }
    assert_eq!(first.band_rects.len(), 2);
    assert!(first.band_rects[0].bottom() < first.band_rects[1].y);
    assert!(first.band_rects[0].contains(first.anchors[0]));
    assert!(first.band_rects[0].contains(first.anchors[1]));
    assert!(first.band_rects[1].contains(first.anchors[2]));
    assert!(first.connectors.iter().all(|connector| {
        connector
            .points
            .windows(2)
            .all(|pair| pair[0].x == pair[1].x || pair[0].y == pair[1].y)
    }));
    assert_eq!(first.spine_segments[0].first().unwrap().x, first.spine_x);
    assert_eq!(first.spine_segments[0].last().unwrap().x, first.spine_x);
    assert!(first.spine_segments[0].first().unwrap().y <= first.anchors[0].y);
    assert!(first.spine_segments[0].last().unwrap().y >= first.anchors[1].y);
}
