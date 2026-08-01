//! Turn flow-space layout geometry into complete screen-space scene primitives.

use crate::layout::{BoxGeom, CLUSTER_PAD, CLUSTER_TITLE_BAND, EDGE_LABEL_PAD, Placed};
use crate::parse::{Edge, Graph};
use crate::scene::{
    Arrow, ArrowHead, Point, Rect, RoutedEdge, Scene, SceneBox, SceneGroup, SceneText,
};
use unicode_width::UnicodeWidthStr;

pub fn route(g: &Graph, placed: &Placed) -> Scene {
    let boxes: Vec<SceneBox> = placed
        .boxes
        .iter()
        .enumerate()
        .map(|(node, b)| SceneBox {
            node,
            rect: box_rect(placed, b),
            lines: if placed.bias_odd_box_labels_right {
                let inner_width = b.clen.saturating_sub(2);
                b.lines
                    .iter()
                    .map(|line| {
                        if inner_width.saturating_sub(line.width()) % 2 == 1 {
                            format!(" {line}")
                        } else {
                            line.clone()
                        }
                    })
                    .collect()
            } else {
                b.lines.clone()
            },
            shape: g.nodes[node].shape,
            table: None,
        })
        .collect();

    let mut groups = route_groups(g, &boxes);

    let mut edges: Vec<RoutedEdge> = placed
        .segs
        .iter()
        .enumerate()
        .filter_map(|(edge_index, segs)| {
            if segs.is_empty() {
                return None;
            }
            let mut points = Vec::new();
            let mut rounded = Vec::new();
            for seg in segs {
                let flow_points = if seg.from.1 == seg.to.1 {
                    vec![seg.from, seg.to]
                } else {
                    let track = placed.channels[seg.channel].track_f(seg.track.unwrap_or(0));
                    rounded.push(to_screen(placed, track, seg.from.1));
                    rounded.push(to_screen(placed, track, seg.to.1));
                    vec![seg.from, (track, seg.from.1), (track, seg.to.1), seg.to]
                };
                for (f, c) in flow_points {
                    let point = to_screen(placed, f, c);
                    if points.last() != Some(&point) {
                        points.push(point);
                    }
                }
            }

            let edge = &g.edges[edge_index];
            let arrow = if edge.arrow && points.len() >= 2 {
                let target = *points.last().unwrap();
                let from = points[points.len() - 2];
                let at = cell_before(target, from);
                *points.last_mut().unwrap() = at;
                Some(Arrow {
                    at,
                    toward: target,
                    head: ArrowHead::Filled,
                })
            } else {
                None
            };

            let first = &segs[0];
            let label = edge.label.as_deref().map(|label| {
                let text = padded_edge_label(label);
                let ch = &placed.channels[first.channel];
                let at = if placed.horizontal {
                    let label_w = multiline_width(&text);
                    let (f, cross) = if first.from.1 == first.to.1 {
                        (
                            ch.start + ch.width.saturating_sub(label_w) / 2,
                            first.from.1,
                        )
                    } else {
                        // The label sits on the horizontal branch after its
                        // jog. Center it in the remaining branch span.
                        let track = ch.track_f(first.track.unwrap_or(0));
                        let branch_start = track + 1;
                        let branch_width = ch
                            .start
                            .saturating_add(ch.width)
                            .saturating_sub(branch_start);
                        (
                            branch_start + branch_width.saturating_sub(label_w) / 2,
                            first.to.1,
                        )
                    };
                    if placed.flipped {
                        to_screen(placed, f + label_w.saturating_sub(1), cross)
                    } else {
                        to_screen(placed, f, cross)
                    }
                } else {
                    let label_height = label.split('\n').count();
                    let lane = placed.segs[..edge_index]
                        .iter()
                        .enumerate()
                        .filter(|(other_ei, other)| {
                            g.edges[*other_ei].label.is_some()
                                && other.first().map(|seg| seg.channel) == Some(first.channel)
                        })
                        .map(|(other_ei, _)| {
                            g.edges[other_ei]
                                .label
                                .as_deref()
                                .map_or(0, |label| label.split('\n').count() + 1)
                        })
                        .sum::<usize>();
                    let label_band_start = ch.start + ch.label_offset;
                    // Flow coordinates reverse under BT, while SceneText rows
                    // always advance downward on screen. Anchor at the final
                    // reserved flow row so a multiline label occupies its
                    // band upward in flow-space without growing into the
                    // source box.
                    let anchor = label_band_start
                        + lane
                        + if placed.flipped {
                            label_height.saturating_sub(1)
                        } else {
                            0
                        };
                    // The label band lies after all bend tracks, alongside the
                    // segment approaching this channel's target. Anchor to
                    // that real shaft rather than the source port, whose leg
                    // may already have ended at an earlier bend.
                    let mut point = to_screen(placed, anchor, first.to.1);
                    point.x += 1;
                    point
                };
                SceneText::new(at, text)
            });

            Some(RoutedEdge {
                edge: edge_index,
                points,
                rounded,
                kind: edge.kind,
                label,
                arrow,
            })
        })
        .collect();

    for &edge_index in &placed.back_edges {
        edges.push(route_back_edge(g, placed, edge_index));
    }
    for &edge_index in &placed.self_loops {
        edges.push(route_self_loop(g, placed, edge_index));
    }

    place_group_titles(&mut groups, &boxes, &edges);

    let mut scene = Scene {
        boxes,
        foreground_boxes: Vec::new(),
        groups,
        paths: Vec::new(),
        edges,
        endpoint_decorations: Vec::new(),
        texts: Vec::new(),
    };
    scene.normalize();
    scene
}

/// Keep group titles in their dedicated interior band while moving them only
/// when routed geometry would overwrite the text. The nearest clear span wins
/// with a stable left bias; if the title band is fully occupied, later clear
/// interior rows are considered before checked rendering rejects the scene.
fn place_group_titles(groups: &mut [SceneGroup], boxes: &[SceneBox], edges: &[RoutedEdge]) {
    let mut routed_cells = Vec::new();
    for edge in edges {
        routed_cells.extend(crate::scene::path_cells(&edge.points));
        if let Some(arrow) = &edge.arrow {
            routed_cells.push(arrow.at);
        }
        if let Some(label) = &edge.label {
            for (line, text) in label.text.split('\n').enumerate() {
                for dx in 0..text.width() as i32 {
                    routed_cells.push(Point::new(label.at.x + dx, label.at.y + line as i32));
                }
            }
        }
    }
    routed_cells.sort_by_key(|point| (point.y, point.x));
    routed_cells.dedup();

    for group_index in 0..groups.len() {
        let rect = groups[group_index].rect;
        let title_width = groups[group_index].title.width() as i32;
        let preferred = groups[group_index].title.at;
        let min_x = rect.x + 1;
        let max_x = rect.right() - 1 - title_width;
        if title_width <= 0 || min_x > max_x {
            continue;
        }

        let mut rows: Vec<i32> = (rect.y + 1..rect.bottom() - 1).collect();
        rows.sort_by_key(|&y| ((y - preferred.y).abs(), y));
        let mut columns: Vec<i32> = (min_x..=max_x).collect();
        columns.sort_by_key(|&x| ((x - preferred.x).abs(), x));

        let placement = rows.into_iter().find_map(|y| {
            columns.iter().copied().find_map(|x| {
                let clear = (x..x + title_width).all(|cell_x| {
                    let point = Point::new(cell_x, y);
                    !point_is_routed(point, &routed_cells)
                        && !boxes.iter().any(|box_| box_.rect.contains(point))
                        && !groups.iter().enumerate().any(|(other_index, other)| {
                            other_index != group_index
                                && rect_contains_rect(rect, other.rect)
                                && other.rect.contains(point)
                        })
                });
                clear.then_some(Point::new(x, y))
            })
        });
        if let Some(at) = placement {
            groups[group_index].title.at = at;
        }
    }
}

fn point_is_routed(point: Point, routed_cells: &[Point]) -> bool {
    routed_cells
        .binary_search_by_key(&(point.y, point.x), |candidate| (candidate.y, candidate.x))
        .is_ok()
}

fn rect_contains_rect(outer: Rect, inner: Rect) -> bool {
    outer.contains(Point::new(inner.x, inner.y))
        && outer.contains(Point::new(inner.right() - 1, inner.bottom() - 1))
}

fn to_screen(placed: &Placed, f: usize, c: usize) -> Point {
    if placed.horizontal {
        let x = if placed.flipped {
            placed.flow_extent - 1 - f
        } else {
            f
        };
        Point::new(x as i32, c as i32)
    } else {
        let y = if placed.flipped {
            placed.flow_extent - 1 - f
        } else {
            f
        };
        Point::new(c as i32, y as i32)
    }
}

fn box_rect(placed: &Placed, b: &BoxGeom) -> Rect {
    if placed.horizontal {
        let x = if placed.flipped {
            placed.flow_extent - b.f - b.flen
        } else {
            b.f
        };
        Rect::new(x as i32, b.c as i32, b.flen as i32, b.clen as i32)
    } else {
        let y = if placed.flipped {
            placed.flow_extent - b.f - b.flen
        } else {
            b.f
        };
        Rect::new(b.c as i32, y as i32, b.clen as i32, b.flen as i32)
    }
}

fn route_groups(g: &Graph, boxes: &[SceneBox]) -> Vec<SceneGroup> {
    let mut rects = vec![None; g.subgraphs.len()];
    let mut deepest_first: Vec<usize> = (0..g.subgraphs.len()).collect();
    deepest_first.sort_by_key(|&index| std::cmp::Reverse(subgraph_depth(g, index)));

    for &index in &deepest_first {
        let group = &g.subgraphs[index];
        let mut content = Rect::default();
        for &node in &group.members {
            content = content.union(boxes[node].rect);
        }
        for (child, child_group) in g.subgraphs.iter().enumerate() {
            if child_group.parent == Some(index)
                && let Some(child_rect) = rects[child]
            {
                content = content.union(child_rect);
            }
        }
        if content.w <= 0 || content.h <= 0 {
            continue;
        }

        let side = CLUSTER_PAD as i32;
        let top = (CLUSTER_PAD + CLUSTER_TITLE_BAND) as i32;
        let mut rect = Rect::new(
            content.x - side,
            content.y - top,
            content.w + 2 * side,
            content.h + top + side,
        );
        let title_width = format!(" {} ", group.title).width() as i32;
        let mut needed = title_width + 4;
        if rect.w < needed {
            // Grow by an even number of cells so the content's doubled center
            // remains exact on the integer grid.
            if (needed - rect.w) % 2 != 0 {
                needed += 1;
            }
            let extra = needed - rect.w;
            rect.x -= extra / 2;
            rect.w = needed;
        }
        rects[index] = Some(rect);
    }

    let mut shallowest_first: Vec<usize> = (0..g.subgraphs.len()).collect();
    shallowest_first.sort_by_key(|&index| subgraph_depth(g, index));
    shallowest_first
        .into_iter()
        .filter_map(|index| {
            let rect = rects[index]?;
            let title = format!(" {} ", g.subgraphs[index].title);
            let title_x = rect.x + (rect.w - title.width() as i32) / 2;
            Some(SceneGroup {
                subgraph: index,
                rect,
                title: SceneText::new(Point::new(title_x, rect.y + 1), title),
                separators: Vec::new(),
            })
        })
        .collect()
}

fn subgraph_depth(g: &Graph, index: usize) -> usize {
    let mut depth = 0;
    let mut parent = g.subgraphs[index].parent;
    while let Some(parent_index) = parent {
        depth += 1;
        parent = g.subgraphs[parent_index].parent;
    }
    depth
}

fn cell_before(to: Point, from: Point) -> Point {
    if from.x < to.x {
        Point::new(to.x - 1, to.y)
    } else if from.x > to.x {
        Point::new(to.x + 1, to.y)
    } else if from.y < to.y {
        Point::new(to.x, to.y - 1)
    } else if from.y > to.y {
        Point::new(to.x, to.y + 1)
    } else {
        to
    }
}

fn route_self_loop(g: &Graph, placed: &Placed, edge_index: usize) -> RoutedEdge {
    let edge = &g.edges[edge_index];
    let rect = box_rect(placed, &placed.boxes[edge.from]);
    let label_w = edge
        .label
        .as_deref()
        .map(|label| multiline_width(label) + 2)
        .unwrap_or(0) as i32;
    let source = Point::new(rect.right() - 1, rect.y + rect.h / 2);
    let target = Point::new(rect.x + rect.w / 2, rect.bottom() - 1);
    let loop_x = rect.right() + label_w + 3 + EDGE_LABEL_PAD as i32;
    let loop_y = rect.bottom() + 2;
    let points = vec![
        source,
        Point::new(loop_x, source.y),
        Point::new(loop_x, loop_y),
        Point::new(target.x, loop_y),
        target,
    ];
    let label = edge.label.as_deref().and_then(|label| {
        let text = padded_edge_label(label);
        let at = Point::new(source.x + 2 + EDGE_LABEL_PAD as i32, source.y);
        (at.x + (multiline_width(&text) as i32) < loop_x).then(|| SceneText::new(at, text))
    });
    routed_screen_path(edge_index, edge, points, label)
}

fn route_back_edge(g: &Graph, placed: &Placed, edge_index: usize) -> RoutedEdge {
    if placed.horizontal {
        route_horizontal_back_edge(g, placed, edge_index)
    } else {
        route_vertical_back_edge(g, placed, edge_index)
    }
}

fn route_vertical_back_edge(g: &Graph, placed: &Placed, edge_index: usize) -> RoutedEdge {
    let edge = &g.edges[edge_index];
    let source_rect = box_rect(placed, &placed.boxes[edge.from]);
    let target_rect = box_rect(placed, &placed.boxes[edge.to]);
    let label_w = edge
        .label
        .as_deref()
        .map(|label| multiline_width(label) + 2)
        .unwrap_or(0) as i32;
    let source = Point::new(source_rect.right() - 1, source_rect.y + source_rect.h / 2);
    let target = Point::new(target_rect.right() - 1, target_rect.y + target_rect.h / 2);
    let track = placed
        .back_edges
        .iter()
        .position(|&ei| ei == edge_index)
        .unwrap_or(0) as i32;
    let minimum = source.x.max(target.x) + label_w + 5 + EDGE_LABEL_PAD as i32 + track * 2;
    let perimeter_x = minimum;
    let points = vec![
        source,
        Point::new(perimeter_x, source.y),
        Point::new(perimeter_x, target.y),
        target,
    ];
    let label = edge.label.as_deref().and_then(|label| {
        let text = padded_edge_label(label);
        let at = Point::new(
            target.x + EDGE_LABEL_PAD as i32 + if edge.arrow { 2 } else { 1 },
            target.y,
        );
        (at.x + (multiline_width(&text) as i32) < perimeter_x).then(|| SceneText::new(at, text))
    });
    routed_screen_path(edge_index, edge, points, label)
}

fn route_horizontal_back_edge(g: &Graph, placed: &Placed, edge_index: usize) -> RoutedEdge {
    let edge = &g.edges[edge_index];
    let source_rect = box_rect(placed, &placed.boxes[edge.from]);
    let target_rect = box_rect(placed, &placed.boxes[edge.to]);
    let source = Point::new(source_rect.x + source_rect.w / 2, source_rect.bottom() - 1);
    let target = Point::new(target_rect.x + target_rect.w / 2, target_rect.bottom() - 1);
    let track = placed
        .back_edges
        .iter()
        .position(|&ei| ei == edge_index)
        .unwrap_or(0);
    let base_height = placed.cross_extent as i32;
    let perimeter_y = base_height + 2 + 2 * track as i32;
    let points = vec![
        source,
        Point::new(source.x, perimeter_y),
        Point::new(target.x, perimeter_y),
        target,
    ];
    let label = edge.label.as_deref().and_then(|label| {
        let text = padded_edge_label(label);
        let left = source.x.min(target.x);
        let right = source.x.max(target.x);
        let text_w = multiline_width(&text) as i32;
        (right > left + text_w).then(|| {
            SceneText::new(
                Point::new(left + (right - left - text_w) / 2, perimeter_y),
                text,
            )
        })
    });
    routed_screen_path(edge_index, edge, points, label)
}

fn multiline_width(text: &str) -> usize {
    text.split('\n')
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0)
}

fn padded_edge_label(label: &str) -> String {
    label
        .split('\n')
        .map(|line| format!(" {line} "))
        .collect::<Vec<_>>()
        .join("\n")
}

fn routed_screen_path(
    edge_index: usize,
    edge: &Edge,
    mut points: Vec<Point>,
    label: Option<SceneText>,
) -> RoutedEdge {
    let arrow = if edge.arrow && points.len() >= 2 {
        let target = *points.last().unwrap();
        let from = points[points.len() - 2];
        let at = cell_before(target, from);
        *points.last_mut().unwrap() = at;
        Some(Arrow {
            at,
            toward: target,
            head: ArrowHead::Filled,
        })
    } else {
        None
    };
    let rounded = points
        .iter()
        .copied()
        .skip(1)
        .take(points.len().saturating_sub(2))
        .collect();
    RoutedEdge {
        edge: edge_index,
        points,
        rounded,
        kind: edge.kind,
        label,
        arrow,
    }
}
