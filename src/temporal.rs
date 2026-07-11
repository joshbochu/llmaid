//! Reusable deterministic integer-grid layout for ordered temporal ranks.
//!
//! The layout is semantic-free: callers provide measured leading/trailing
//! boxes, ordered band membership, and spacing. No dates, durations, labels,
//! or Mermaid concepts cross this boundary.

use crate::scene::{Point, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extent {
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemporalEntry {
    pub leading: Extent,
    pub trailing: Vec<Extent>,
    pub band: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TemporalBand {
    pub first_entry: usize,
    pub entry_count: usize,
    pub title_width: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TemporalGaps {
    pub leading_to_spine: i32,
    pub spine_to_trailing: i32,
    pub entry_gap: i32,
    pub rank_gap: i32,
    pub band_gap: i32,
}

impl TemporalGaps {
    pub const fn normal() -> Self {
        Self {
            leading_to_spine: 3,
            spine_to_trailing: 3,
            entry_gap: 0,
            rank_gap: 1,
            band_gap: 1,
        }
    }

    pub const fn compact() -> Self {
        Self {
            leading_to_spine: 2,
            spine_to_trailing: 2,
            entry_gap: 0,
            rank_gap: 1,
            band_gap: 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemporalConnector {
    pub entry: usize,
    pub trailing: Option<usize>,
    pub points: Vec<Point>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TemporalLayout {
    pub spine_x: i32,
    pub leading_boxes: Vec<Rect>,
    pub trailing_boxes: Vec<Vec<Rect>>,
    pub anchors: Vec<Point>,
    pub connectors: Vec<TemporalConnector>,
    pub spine_segments: Vec<Vec<Point>>,
    pub band_rects: Vec<Rect>,
    pub band_title_points: Vec<Point>,
}

#[derive(Clone, Debug)]
struct LocalEntry {
    leading: Rect,
    trailing: Vec<Rect>,
    anchor_y: i32,
    height: i32,
}

/// Lay out ordered temporal entries on one fixed vertical spine.
///
/// All coordinates are integer terminal cells. Input heights are rounded up
/// to odd values so leading boxes and every trailing attachment have exact
/// cell-center rows.
pub fn layout(
    entries: &[TemporalEntry],
    bands: &[TemporalBand],
    gaps: TemporalGaps,
) -> TemporalLayout {
    if entries.is_empty() {
        return TemporalLayout::default();
    }
    assert!(
        gaps.leading_to_spine >= 2
            && gaps.spine_to_trailing >= 2
            && gaps.entry_gap >= 0
            && gaps.rank_gap >= 1
            && gaps.band_gap >= 1,
        "temporal gaps must leave visible connector and separation cells"
    );
    validate(entries, bands);

    let leading_column = entries
        .iter()
        .map(|entry| checked(entry.leading).width)
        .max()
        .unwrap_or(3);
    let trailing_column = entries
        .iter()
        .flat_map(|entry| entry.trailing.iter())
        .map(|extent| checked(*extent).width)
        .max()
        .unwrap_or(1);
    let half_width =
        (leading_column + gaps.leading_to_spine).max(trailing_column + gaps.spine_to_trailing);
    let spine_x = half_width;
    let trailing_x = spine_x + half_width - trailing_column;

    let locals: Vec<LocalEntry> = entries
        .iter()
        .map(|entry| {
            let leading = checked(entry.leading);
            let trailing: Vec<Extent> = entry.trailing.iter().copied().map(checked).collect();
            let mut y = 0;
            let mut trailing_rects: Vec<Rect> = trailing
                .into_iter()
                .map(|extent| {
                    let rect = Rect::new(trailing_x, y, extent.width, extent.height);
                    y += extent.height + gaps.entry_gap;
                    rect
                })
                .collect();
            let anchor_y = if let (Some(first), Some(last)) =
                (trailing_rects.first(), trailing_rects.last())
            {
                let sum = first.center2().y / 2 + last.center2().y / 2;
                (sum + sum.rem_euclid(2)) / 2
            } else {
                0
            };
            let mut leading_rect = Rect::new(
                leading_column - leading.width,
                anchor_y - leading.height / 2,
                leading.width,
                leading.height,
            );
            let min_y = trailing_rects
                .iter()
                .map(|rect| rect.y)
                .chain(std::iter::once(leading_rect.y))
                .min()
                .unwrap_or(0);
            let max_bottom = trailing_rects
                .iter()
                .map(|rect| rect.bottom())
                .chain(std::iter::once(leading_rect.bottom()))
                .max()
                .unwrap_or(1);
            let shift = -min_y;
            leading_rect = translate(leading_rect, 0, shift);
            for rect in &mut trailing_rects {
                *rect = translate(*rect, 0, shift);
            }
            LocalEntry {
                leading: leading_rect,
                trailing: trailing_rects,
                anchor_y: anchor_y + shift,
                height: max_bottom - min_y,
            }
        })
        .collect();

    let mut leading_boxes = Vec::with_capacity(entries.len());
    let mut trailing_boxes: Vec<Vec<Rect>> = Vec::with_capacity(entries.len());
    let mut anchors = Vec::with_capacity(entries.len());
    let mut cursor = 0i32;
    for (index, local) in locals.iter().enumerate() {
        let starts_band = entries[index].band.is_some()
            && (index == 0 || entries[index - 1].band != entries[index].band);
        if index == 0 {
            if starts_band {
                cursor += 3;
            }
        } else {
            let previous_band = entries[index - 1].band;
            let current_band = entries[index].band;
            if previous_band == current_band {
                cursor += gaps.rank_gap;
            } else {
                if previous_band.is_some() {
                    cursor += 2;
                }
                cursor += gaps.band_gap;
                if current_band.is_some() {
                    cursor += 3;
                }
            }
        }

        leading_boxes.push(translate(local.leading, 0, cursor));
        trailing_boxes.push(
            local
                .trailing
                .iter()
                .copied()
                .map(|rect| translate(rect, 0, cursor))
                .collect(),
        );
        anchors.push(Point::new(spine_x, cursor + local.anchor_y));
        cursor += local.height;
    }

    let mut connectors = Vec::new();
    for entry in 0..entries.len() {
        let leading = leading_boxes[entry];
        let anchor = anchors[entry];
        connectors.push(TemporalConnector {
            entry,
            trailing: None,
            points: vec![Point::new(leading.right() - 1, anchor.y), anchor],
        });
        for (trailing, rect) in trailing_boxes[entry].iter().enumerate() {
            let y = rect.center2().y / 2;
            connectors.push(TemporalConnector {
                entry,
                trailing: Some(trailing),
                points: vec![Point::new(spine_x, y), Point::new(rect.x, y)],
            });
        }
    }

    let mut spine_segments = Vec::new();
    let mut first = 0usize;
    while first < entries.len() {
        let band = entries[first].band;
        let mut end = first + 1;
        while end < entries.len() && entries[end].band == band {
            end += 1;
        }
        let mut top = i32::MAX;
        let mut bottom = i32::MIN;
        for entry in first..end {
            top = top.min(anchors[entry].y);
            bottom = bottom.max(anchors[entry].y);
            for rect in &trailing_boxes[entry] {
                let center = rect.center2().y / 2;
                top = top.min(center);
                bottom = bottom.max(center);
            }
        }
        if top == bottom {
            top -= 1;
            bottom += 1;
        }
        spine_segments.push(vec![Point::new(spine_x, top), Point::new(spine_x, bottom)]);
        first = end;
    }

    let title_radius = bands
        .iter()
        .map(|band| (band.title_width + 4) / 2)
        .max()
        .unwrap_or(0);
    let band_radius = (spine_x + 2).max(title_radius);
    let band_left = spine_x - band_radius;
    let band_width = 2 * band_radius + 1;
    let mut band_rects = Vec::with_capacity(bands.len());
    let mut band_title_points = Vec::with_capacity(bands.len());
    for band in bands {
        let last = band.first_entry + band.entry_count - 1;
        let y = leading_boxes[band.first_entry].y.min(
            trailing_boxes[band.first_entry]
                .first()
                .map_or(i32::MAX, |rect| rect.y),
        ) - 3;
        let last_bottom = leading_boxes[last].bottom().max(
            trailing_boxes[last]
                .last()
                .map_or(i32::MIN, |rect| rect.bottom()),
        );
        band_rects.push(Rect::new(band_left, y, band_width, last_bottom + 2 - y));
        band_title_points.push(Point::new(band_left + 2, y + 1));
    }

    TemporalLayout {
        spine_x,
        leading_boxes,
        trailing_boxes,
        anchors,
        connectors,
        spine_segments,
        band_rects,
        band_title_points,
    }
}

fn checked(mut extent: Extent) -> Extent {
    assert!(extent.width >= 1 && extent.height >= 1);
    if extent.height % 2 == 0 {
        extent.height += 1;
    }
    extent
}

fn validate(entries: &[TemporalEntry], bands: &[TemporalBand]) {
    let mut previous_end = 0usize;
    for (index, band) in bands.iter().enumerate() {
        assert!(band.entry_count > 0, "temporal bands cannot be empty");
        assert!(
            band.first_entry >= previous_end,
            "temporal bands must be ordered"
        );
        let end = band.first_entry + band.entry_count;
        assert!(end <= entries.len(), "temporal band exceeds entry range");
        assert!(
            entries[band.first_entry..end]
                .iter()
                .all(|entry| entry.band == Some(index)),
            "temporal entry band indices must match band ranges"
        );
        previous_end = end;
    }
    for (entry, item) in entries.iter().enumerate() {
        if let Some(band) = item.band {
            assert!(band < bands.len(), "temporal entry names an unknown band");
            let range = bands[band].first_entry..bands[band].first_entry + bands[band].entry_count;
            assert!(
                range.contains(&entry),
                "temporal entry lies outside its band"
            );
        }
    }
}

fn translate(rect: Rect, dx: i32, dy: i32) -> Rect {
    Rect::new(rect.x + dx, rect.y + dy, rect.w, rect.h)
}
