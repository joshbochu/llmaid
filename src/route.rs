//! Turn flow-space layout geometry into complete screen-space scene primitives.

use crate::layout::{BoxGeom, CLUSTER_PAD, CLUSTER_TITLE_BAND, EDGE_LABEL_PAD, Placed};
use crate::parse::{Edge, Endpoint, FlowEndpointDecoration, Graph};
use crate::scene::{
    Arrow, ArrowHead, EndpointDecoration, EndpointDecorationKind, Point, Rect, RoutedEdge, Scene,
    SceneBox, SceneGroup, SceneText,
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
            let arrow = target_terminal_arrow(edge, &mut points);

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

    // A group endpoint is laid out through one of its member nodes, then
    // clipped back to the semantic frame. This keeps the layered engine
    // node-only while ensuring no internal proxy leaks into the Scene.
    for edge in &mut edges {
        clip_group_endpoints(g, &groups, &boxes, edge);
    }
    // A path to a nested group can legitimately traverse its ancestors. Keep
    // that traversal out of each ancestor's dedicated title row by using the
    // existing one-cell side gutter rather than allowing an edge to overwrite
    // frame text.
    for group in &groups {
        for edge in &mut edges {
            if g.edges.get(edge.edge).is_some_and(|semantic| {
                matches!(semantic.source, Endpoint::Subgraph(_))
                    || matches!(semantic.target, Endpoint::Subgraph(_))
            }) {
                detour_group_title(edge, group);
            }
        }
    }
    for edge in &mut edges {
        if g.edges.get(edge.edge).is_some_and(|semantic| {
            matches!(semantic.source, Endpoint::Subgraph(_))
                || matches!(semantic.target, Endpoint::Subgraph(_))
        }) {
            relocate_group_endpoint_label(edge, &groups, &boxes);
        }
    }

    let endpoint_decorations = apply_flow_endpoint_decorations(g, &groups, &mut edges);
    place_group_titles(&mut groups, &boxes, &edges);

    let mut scene = Scene {
        boxes,
        foreground_boxes: Vec::new(),
        groups,
        paths: Vec::new(),
        edges,
        endpoint_decorations,
        texts: Vec::new(),
    };
    scene.normalize();
    scene
}

fn clip_group_endpoints(
    g: &Graph,
    groups: &[SceneGroup],
    boxes: &[SceneBox],
    edge: &mut RoutedEdge,
) {
    let Some(semantic) = g.edges.get(edge.edge) else {
        return;
    };
    if !matches!(semantic.source, Endpoint::Subgraph(_))
        && !matches!(semantic.target, Endpoint::Subgraph(_))
    {
        return;
    }
    let mut full = edge.points.clone();
    if let Some(arrow) = &edge.arrow
        && full.last() != Some(&arrow.toward)
    {
        full.push(arrow.toward);
    }
    if full.len() < 2 {
        return;
    }

    let containment = containment_endpoint_route(semantic, groups, boxes);
    let used_containment_route = containment.is_some();
    if let Some(contained) = containment {
        full = contained;
    } else {
        if let Endpoint::Subgraph(group) = semantic.source
            && let Some(rect) = groups
                .iter()
                .find(|value| value.subgraph == group)
                .map(|value| value.rect)
            && let Some(clipped) = clip_polyline_at_group(&full, rect, true)
        {
            full = clipped;
        }
        if let Endpoint::Subgraph(group) = semantic.target
            && let Some(rect) = groups
                .iter()
                .find(|value| value.subgraph == group)
                .map(|value| value.rect)
            && let Some(clipped) = clip_polyline_at_group(&full, rect, false)
        {
            full = clipped;
        }
    }

    let arrow_head = edge.arrow.as_ref().map(|arrow| arrow.head);
    edge.arrow = arrow_head.and_then(|head| {
        (full.len() >= 2).then(|| {
            let target = *full.last().expect("checked length");
            let from = full[full.len() - 2];
            let at = cell_before(target, from);
            *full.last_mut().expect("checked length") = at;
            Arrow {
                at,
                toward: target,
                head,
            }
        })
    });
    edge.rounded = polyline_bends(&full);
    edge.points = full;
    if used_containment_route {
        edge.label = semantic
            .label
            .as_deref()
            .map(|label| containment_label(edge, label, groups, boxes));
    }
}

/// Reserve the path endpoint cell for each Mermaid terminal mark after all
/// group clipping is complete.  Keeping this final step in screen geometry
/// makes a mark adjacent to the semantic node or frame in every direction,
/// including feedback and nested-group routes.
fn apply_flow_endpoint_decorations(
    g: &Graph,
    groups: &[SceneGroup],
    edges: &mut [RoutedEdge],
) -> Vec<EndpointDecoration> {
    let mut decorations = Vec::new();
    for edge in edges {
        let Some(semantic) = g.edges.get(edge.edge) else {
            continue;
        };
        if edge.points.len() < 2 {
            continue;
        }

        if let Some(kind) = scene_decoration_kind(semantic.source_decoration) {
            let toward = edge.points[0];
            let at = cell_after(toward, edge.points[1]);
            edge.points[0] = at;
            decorations.push(EndpointDecoration {
                edge: edge.edge,
                at,
                toward,
                kind,
            });
        }

        // A frame can begin one row/column after an external node. Two
        // terminal marks would then demand the same route cell. Reattach the
        // group side through its outer gutter before painting, retaining two
        // distinct, adjacent semantic terminals instead of collapsing them.
        if semantic.source_decoration != FlowEndpointDecoration::None
            && semantic.target_decoration != FlowEndpointDecoration::None
            && let Endpoint::Subgraph(group) = semantic.target
            && let Some(rect) = groups
                .iter()
                .find(|value| value.subgraph == group)
                .map(|value| value.rect)
            && let Some(arrow) = edge.arrow.as_ref()
            && edge.points.first() == Some(&arrow.at)
        {
            let source_at = arrow.at;
            let approach_is_horizontal = arrow.at.y == arrow.toward.y;
            let (target_at, target_toward, bend) = if approach_is_horizontal {
                let toward = Point::new(rect.x + rect.w / 2, rect.y);
                let at = Point::new(toward.x, toward.y - 1);
                (at, toward, Point::new(source_at.x, at.y))
            } else {
                let toward = Point::new(rect.x, rect.y + rect.h / 2);
                let at = Point::new(toward.x - 1, toward.y);
                (at, toward, Point::new(at.x, source_at.y))
            };
            let mut points = vec![source_at, bend, target_at];
            points.dedup();
            edge.points = points;
            edge.rounded = polyline_bends(&edge.points);
            edge.arrow = Some(Arrow {
                at: target_at,
                toward: target_toward,
                head: ArrowHead::Filled,
            });
        }

        let target = semantic.target_decoration;
        if matches!(
            target,
            FlowEndpointDecoration::Circle | FlowEndpointDecoration::Cross
        ) {
            let (at, toward) = edge.arrow.take().map_or_else(
                || {
                    let last = edge.points.len() - 1;
                    let toward = edge.points[last];
                    let at = cell_before(toward, edge.points[last - 1]);
                    edge.points[last] = at;
                    (at, toward)
                },
                |arrow| (arrow.at, arrow.toward),
            );
            if let Some(kind) = scene_decoration_kind(target) {
                decorations.push(EndpointDecoration {
                    edge: edge.edge,
                    at,
                    toward,
                    kind,
                });
            }
        }
    }
    decorations
}

fn scene_decoration_kind(decoration: FlowEndpointDecoration) -> Option<EndpointDecorationKind> {
    match decoration {
        FlowEndpointDecoration::None => None,
        FlowEndpointDecoration::Arrow => Some(EndpointDecorationKind::Arrow),
        FlowEndpointDecoration::Circle => Some(EndpointDecorationKind::Circle),
        FlowEndpointDecoration::Cross => Some(EndpointDecorationKind::Cross),
    }
}

/// Construct the short gutter path needed when a group endpoint contains the
/// other endpoint. A proxy route may never leave that requested frame, so it
/// cannot be clipped at a border. The four side candidates are all integer
/// straight paths; the first clear one keeps the existing Scene collision
/// contract without introducing a second compound-layout phase.
fn containment_endpoint_route(
    edge: &Edge,
    groups: &[SceneGroup],
    boxes: &[SceneBox],
) -> Option<Vec<Point>> {
    let source = endpoint_rect(edge.source, groups, boxes)?;
    let target = endpoint_rect(edge.target, groups, boxes)?;
    let source_contains_target =
        matches!(edge.source, Endpoint::Subgraph(_)) && rect_contains_rect(source, target);
    let target_contains_source =
        matches!(edge.target, Endpoint::Subgraph(_)) && rect_contains_rect(target, source);
    if !source_contains_target && !target_contains_source {
        return None;
    }

    // A mark consumes the first or last route cell.  A direct containment
    // segment can therefore leave a circle/cross/arrow with no visible run
    // between its two terminals.  Route through the clear band immediately
    // outside the contained endpoint instead.  This is screen-space rather
    // than direction-specific, so LR/RL/TB/BT and either semantic direction
    // use the same four-side construction.
    if edge.has_endpoint_decoration()
        && let Some(points) =
            decorated_containment_route(edge, source, target, source_contains_target, groups, boxes)
    {
        return Some(points);
    }

    for side in [
        ContainmentSide::Left,
        ContainmentSide::Right,
        ContainmentSide::Bottom,
        ContainmentSide::Top,
    ] {
        let (from, to) = shared_side_anchors(source, target, side);
        let points = (from != to).then_some(vec![from, to])?;
        if containment_path_clear(&points, edge, groups, boxes) {
            return Some(points);
        }
    }
    None
}

fn decorated_containment_route(
    edge: &Edge,
    source: Rect,
    target: Rect,
    source_contains_target: bool,
    groups: &[SceneGroup],
    boxes: &[SceneBox],
) -> Option<Vec<Point>> {
    let (outer, inner) = if source_contains_target {
        (source, target)
    } else {
        (target, source)
    };

    // Enter through one outer-frame side and approach a perpendicular inner
    // side.  The middle segment sits in the group padding, never on an inner
    // frame or node border, leaving one route cell for each terminal mark.
    for (outer_side, inner_side) in [
        (ContainmentSide::Left, ContainmentSide::Top),
        (ContainmentSide::Left, ContainmentSide::Bottom),
        (ContainmentSide::Right, ContainmentSide::Top),
        (ContainmentSide::Right, ContainmentSide::Bottom),
        (ContainmentSide::Top, ContainmentSide::Left),
        (ContainmentSide::Top, ContainmentSide::Right),
        (ContainmentSide::Bottom, ContainmentSide::Left),
        (ContainmentSide::Bottom, ContainmentSide::Right),
    ] {
        let (outer_anchor, inner_approach, inner_anchor) =
            containment_gutter_anchors(outer, inner, outer_side, inner_side);
        let mut points = vec![outer_anchor, inner_approach, inner_anchor];
        points.dedup();
        if points.len() < 3 || crate::scene::path_cells(&points).len() < 4 {
            continue;
        }
        if !source_contains_target {
            points.reverse();
        }
        if decorated_containment_path_clear(&points, edge, groups, boxes) {
            return Some(points);
        }
    }
    None
}

fn containment_gutter_anchors(
    outer: Rect,
    inner: Rect,
    outer_side: ContainmentSide,
    inner_side: ContainmentSide,
) -> (Point, Point, Point) {
    let inner_x = (inner.x + 1).min(inner.right() - 2);
    let inner_y = (inner.y + 1).min(inner.bottom() - 2);
    match (outer_side, inner_side) {
        (ContainmentSide::Left, ContainmentSide::Top) => {
            let approach = Point::new(inner_x, inner.y - 1);
            (
                Point::new(outer.x, approach.y),
                approach,
                Point::new(inner_x, inner.y),
            )
        }
        (ContainmentSide::Left, ContainmentSide::Bottom) => {
            let approach = Point::new(inner_x, inner.bottom());
            (
                Point::new(outer.x, approach.y),
                approach,
                Point::new(inner_x, inner.bottom() - 1),
            )
        }
        (ContainmentSide::Right, ContainmentSide::Top) => {
            let approach = Point::new(inner_x, inner.y - 1);
            (
                Point::new(outer.right() - 1, approach.y),
                approach,
                Point::new(inner_x, inner.y),
            )
        }
        (ContainmentSide::Right, ContainmentSide::Bottom) => {
            let approach = Point::new(inner_x, inner.bottom());
            (
                Point::new(outer.right() - 1, approach.y),
                approach,
                Point::new(inner_x, inner.bottom() - 1),
            )
        }
        (ContainmentSide::Top, ContainmentSide::Left) => {
            let approach = Point::new(inner.x - 1, inner_y);
            (
                Point::new(approach.x, outer.y),
                approach,
                Point::new(inner.x, inner_y),
            )
        }
        (ContainmentSide::Top, ContainmentSide::Right) => {
            let approach = Point::new(inner.right(), inner_y);
            (
                Point::new(approach.x, outer.y),
                approach,
                Point::new(inner.right() - 1, inner_y),
            )
        }
        (ContainmentSide::Bottom, ContainmentSide::Left) => {
            let approach = Point::new(inner.x - 1, inner_y);
            (
                Point::new(approach.x, outer.bottom() - 1),
                approach,
                Point::new(inner.x, inner_y),
            )
        }
        (ContainmentSide::Bottom, ContainmentSide::Right) => {
            let approach = Point::new(inner.right(), inner_y);
            (
                Point::new(approach.x, outer.bottom() - 1),
                approach,
                Point::new(inner.right() - 1, inner_y),
            )
        }
        _ => unreachable!("containment sides must be perpendicular"),
    }
}

fn decorated_containment_path_clear(
    points: &[Point],
    edge: &Edge,
    groups: &[SceneGroup],
    boxes: &[SceneBox],
) -> bool {
    if !containment_path_clear(points, edge, groups, boxes) {
        return false;
    }
    let terminals = [
        points[0],
        *points.last().expect("nonempty containment route"),
    ];
    let cells = crate::scene::path_cells(points);
    cells.iter().all(|&point| {
        let touches_box = boxes.iter().any(|box_| box_.rect.contains(point));
        let touches_frame = groups.iter().any(|group| {
            group.rect.contains(point)
                && (point.x == group.rect.x
                    || point.x == group.rect.right() - 1
                    || point.y == group.rect.y
                    || point.y == group.rect.bottom() - 1)
        });
        (!touches_box && !touches_frame) || terminals.contains(&point)
    })
}

#[derive(Clone, Copy)]
enum ContainmentSide {
    Left,
    Right,
    Bottom,
    Top,
}

fn shared_side_anchors(source: Rect, target: Rect, side: ContainmentSide) -> (Point, Point) {
    match side {
        ContainmentSide::Left | ContainmentSide::Right => {
            let y = shared_coordinate(
                source.y + 1,
                source.bottom() - 2,
                target.y + 1,
                target.bottom() - 2,
            );
            let x = |rect: Rect| match side {
                ContainmentSide::Left => rect.x,
                ContainmentSide::Right => rect.right() - 1,
                _ => unreachable!(),
            };
            (Point::new(x(source), y), Point::new(x(target), y))
        }
        ContainmentSide::Bottom | ContainmentSide::Top => {
            let x = shared_coordinate(
                source.x + 1,
                source.right() - 2,
                target.x + 1,
                target.right() - 2,
            );
            let y = |rect: Rect| match side {
                ContainmentSide::Bottom => rect.bottom() - 1,
                ContainmentSide::Top => rect.y,
                _ => unreachable!(),
            };
            (Point::new(x, y(source)), Point::new(x, y(target)))
        }
    }
}

fn shared_coordinate(a_min: i32, a_max: i32, b_min: i32, b_max: i32) -> i32 {
    let low = a_min.max(b_min);
    let high = a_max.min(b_max);
    (low + high) / 2
}

fn containment_path_clear(
    points: &[Point],
    edge: &Edge,
    groups: &[SceneGroup],
    boxes: &[SceneBox],
) -> bool {
    let cells = crate::scene::path_cells(points);
    let endpoint_nodes = [edge.source, edge.target]
        .into_iter()
        .filter_map(|endpoint| match endpoint {
            Endpoint::Node(node) => Some(node),
            Endpoint::Subgraph(_) => None,
        })
        .collect::<Vec<_>>();
    cells.iter().all(|&point| {
        !boxes
            .iter()
            .filter(|box_| !endpoint_nodes.contains(&box_.node))
            .any(|box_| box_.rect.contains(point))
            && !groups
                .iter()
                .any(|group| group.title.bounds().contains(point))
    })
}

fn endpoint_rect(endpoint: Endpoint, groups: &[SceneGroup], boxes: &[SceneBox]) -> Option<Rect> {
    match endpoint {
        Endpoint::Node(node) => boxes
            .iter()
            .find(|box_| box_.node == node)
            .map(|box_| box_.rect),
        Endpoint::Subgraph(group) => groups
            .iter()
            .find(|value| value.subgraph == group)
            .map(|value| value.rect),
    }
}

/// A containment route replaces the proxy route completely, so its label must
/// be placed from the semantic edge text rather than inheriting a position (or
/// a dropped label) from that discarded geometry. Prefer a cell adjacent to
/// the new segment; if the nested frames leave no room, use the deterministic
/// clear margin outside all routed/group geometry. Labels are never truncated.
fn containment_label(
    edge: &RoutedEdge,
    label: &str,
    groups: &[SceneGroup],
    boxes: &[SceneBox],
) -> SceneText {
    let text = padded_edge_label(label);
    let width = multiline_width(&text) as i32;
    let height = text.split('\n').count() as i32;
    let mut points = edge.points.clone();
    if let Some(arrow) = &edge.arrow
        && points.last() != Some(&arrow.toward)
    {
        points.push(arrow.toward);
    }

    let mut candidates = Vec::new();
    for pair in points.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        if from.y == to.y {
            let left = from.x.min(to.x);
            let right = from.x.max(to.x);
            let centered = left + (right - left - width) / 2;
            candidates.extend([
                Point::new(centered, from.y - height),
                Point::new(centered, from.y + 1),
                Point::new(left - width - 1, from.y),
                Point::new(right + 2, from.y),
            ]);
        } else if from.x == to.x {
            let top = from.y.min(to.y);
            let bottom = from.y.max(to.y);
            let centered = top + (bottom - top - height) / 2;
            candidates.extend([
                Point::new(from.x - width - 1, centered),
                Point::new(from.x + 2, centered),
                Point::new(from.x, top - height - 1),
                Point::new(from.x, bottom + 2),
            ]);
        }
    }
    candidates.dedup();
    if let Some(at) = candidates
        .into_iter()
        .find(|&at| label_rect_clear(at, width, height, groups, boxes))
    {
        return SceneText::new(at, text);
    }

    let min_x = points
        .iter()
        .map(|point| point.x)
        .chain(groups.iter().map(|group| group.rect.x))
        .chain(boxes.iter().map(|box_| box_.rect.x))
        .min()
        .unwrap_or(0);
    let y = points.first().map_or(0, |point| point.y);
    SceneText::new(Point::new(min_x - width - 1, y), text)
}

/// Crop an orthogonal routed polyline at the first group-frame crossing. The
/// caller supplies the semantic direction: sources leave a frame, targets
/// enter one. Returning `None` leaves unusual entirely-internal compositions
/// to their existing route rather than inventing a diagonal path.
fn clip_polyline_at_group(points: &[Point], rect: Rect, source: bool) -> Option<Vec<Point>> {
    let cells = crate::scene::path_cells(points);
    if cells.len() < 2 {
        return None;
    }
    let clipped = if source {
        let outside = cells.iter().position(|point| !rect.contains(*point))?;
        (outside > 0).then(|| cells[outside - 1..].to_vec())?
    } else {
        let outside = cells.iter().rposition(|point| !rect.contains(*point))?;
        (outside + 1 < cells.len()).then(|| cells[..=outside + 1].to_vec())?
    };
    Some(compact_polyline(&clipped))
}

fn compact_polyline(cells: &[Point]) -> Vec<Point> {
    let mut points = vec![cells[0]];
    for triple in cells.windows(3) {
        let before = triple[0];
        let at = triple[1];
        let after = triple[2];
        let first_direction = (at.x - before.x, at.y - before.y);
        let second_direction = (after.x - at.x, after.y - at.y);
        if first_direction != second_direction && points.last() != Some(&at) {
            points.push(at);
        }
    }
    if points.last() != cells.last() {
        points.push(*cells.last().expect("nonempty cells"));
    }
    points
}

fn polyline_bends(points: &[Point]) -> Vec<Point> {
    points
        .windows(3)
        .filter_map(|triple| {
            let first_direction = (triple[1].x - triple[0].x, triple[1].y - triple[0].y);
            let second_direction = (triple[2].x - triple[1].x, triple[2].y - triple[1].y);
            (first_direction != second_direction).then_some(triple[1])
        })
        .collect()
}

fn detour_group_title(edge: &mut RoutedEdge, group: &SceneGroup) {
    let title = group.title.bounds();
    let mut rerouted = Vec::new();
    for pair in edge.points.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        if rerouted.last() != Some(&from) {
            rerouted.push(from);
        }
        if from.x == to.x
            && from.x >= title.x
            && from.x < title.right()
            && from.y.min(to.y) <= title.y
            && title.y <= from.y.max(to.y)
        {
            let downward = to.y > from.y;
            let first = if downward {
                if from.y < group.rect.y {
                    Point::new(from.x, group.rect.y - 1)
                } else {
                    from
                }
            } else if from.y >= group.rect.bottom() {
                Point::new(from.x, group.rect.bottom())
            } else {
                from
            };
            let last = if downward {
                if to.y >= group.rect.bottom() {
                    Point::new(to.x, group.rect.bottom())
                } else {
                    to
                }
            } else if to.y < group.rect.y {
                Point::new(to.x, group.rect.y - 1)
            } else {
                to
            };
            let gutter = Point::new(group.rect.x + 1, first.y);
            for point in [first, gutter, Point::new(gutter.x, last.y), last] {
                if rerouted.last() != Some(&point) {
                    rerouted.push(point);
                }
            }
        }
        if rerouted.last() != Some(&to) {
            rerouted.push(to);
        }
    }
    if rerouted.len() >= 2 {
        edge.rounded = polyline_bends(&rerouted);
        edge.points = rerouted;
    }
}

/// Labels use the ordinary channel reservation before a group endpoint is
/// clipped to its frame. If that reservation would paint through a frame,
/// move the label to the nearest clear, adjacent segment of the same route.
/// This keeps the label semantic while preserving group borders.
fn relocate_group_endpoint_label(edge: &mut RoutedEdge, groups: &[SceneGroup], boxes: &[SceneBox]) {
    let Some(label) = edge.label.as_ref() else {
        return;
    };
    if label_clear_of_groups(label, groups, boxes) {
        return;
    }
    let width = label.width() as i32;
    let height = label.height() as i32;
    let original = label.at;
    let mut candidates = Vec::new();
    for pair in edge.points.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        if from.y == to.y {
            for x in from.x.min(to.x) - width..=from.x.max(to.x) + 1 {
                candidates.push(Point::new(x, from.y));
            }
        } else if from.x == to.x {
            for y in from.y.min(to.y)..=from.y.max(to.y) {
                candidates.push(Point::new(from.x + 1, y));
                candidates.push(Point::new(from.x - width, y));
            }
        }
    }
    candidates.sort_by_key(|point| {
        (
            point.x.abs_diff(original.x) + point.y.abs_diff(original.y),
            point.y,
            point.x,
        )
    });
    candidates.dedup();
    if let Some(at) = candidates
        .into_iter()
        .find(|&at| label_rect_clear(at, width, height, groups, boxes))
    {
        edge.label.as_mut().expect("label checked above").at = at;
    }
}

fn label_clear_of_groups(label: &SceneText, groups: &[SceneGroup], boxes: &[SceneBox]) -> bool {
    label_rect_clear(
        label.at,
        label.width() as i32,
        label.height() as i32,
        groups,
        boxes,
    )
}

fn label_rect_clear(
    at: Point,
    width: i32,
    height: i32,
    groups: &[SceneGroup],
    boxes: &[SceneBox],
) -> bool {
    (at.y..at.y + height).all(|y| {
        (at.x..at.x + width).all(|x| {
            let point = Point::new(x, y);
            !boxes.iter().any(|box_| box_.rect.contains(point))
                && !groups.iter().any(|group| {
                    let border = group.rect.contains(point)
                        && (point.x == group.rect.x
                            || point.x == group.rect.right() - 1
                            || point.y == group.rect.y
                            || point.y == group.rect.bottom() - 1);
                    border || group.title.bounds().contains(point)
                })
        })
    })
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

fn cell_after(from: Point, to: Point) -> Point {
    if to.x > from.x {
        Point::new(from.x + 1, from.y)
    } else if to.x < from.x {
        Point::new(from.x - 1, from.y)
    } else if to.y > from.y {
        Point::new(from.x, from.y + 1)
    } else if to.y < from.y {
        Point::new(from.x, from.y - 1)
    } else {
        from
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
    // A decorated self-loop needs terminals that cannot be claimed by the
    // ordinary centred ports of concurrent edges. Use off-centre top/bottom
    // ports and keep the entire return outside the box's left perimeter. This
    // also prevents an LR/RL successor in the next rank from occupying the
    // old right-side loop channel.
    if edge.distinct_endpoints {
        let source = Point::new(rect.x + 1, rect.y);
        let target = Point::new(rect.right() - 2, rect.bottom() - 1);
        let upper = rect.y - 2;
        let left = rect.x - 2;
        let lower = rect.bottom() + 2;
        let points = vec![
            source,
            Point::new(source.x, upper),
            Point::new(left, upper),
            Point::new(left, lower),
            Point::new(target.x, lower),
            target,
        ];
        let label = edge.label.as_deref().map(|label| {
            let text = padded_edge_label(label);
            let x = rect.x - multiline_width(&text) as i32 - 3;
            SceneText::new(Point::new(x, rect.y), text)
        });
        return routed_screen_path(edge_index, edge, points, label);
    }

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
            target.x + EDGE_LABEL_PAD as i32 + if edge.has_target_arrow() { 2 } else { 1 },
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
    let arrow = target_terminal_arrow(edge, &mut points);
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

/// Build the existing target-arrow geometry before group clipping. Circles
/// and crosses use the same temporary terminal shape so clipping and title
/// detours retain a one-cell endpoint approach; they are converted into their
/// final Scene decoration after that geometry pass.
fn target_terminal_arrow(edge: &Edge, points: &mut [Point]) -> Option<Arrow> {
    (edge.target_decoration != FlowEndpointDecoration::None && points.len() >= 2).then(|| {
        let last = points.len() - 1;
        let target = points[last];
        let at = cell_before(target, points[last - 1]);
        points[last] = at;
        Arrow {
            at,
            toward: target,
            head: ArrowHead::Filled,
        }
    })
}
