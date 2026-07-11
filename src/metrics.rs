//! Layout symmetry / grid-regularity metrics for diagnostic review.
//!
//! These scores are **not** render invariants (B14). They quantify how
//! grid-aligned and balanced a layout is so we can tune layout and later
//! lock regressions. Floats are allowed here; layout/route/render stay integer.

use crate::layout::Placed;
use crate::parse::Graph;
use crate::route;
use crate::scene::{Point, Rect, Scene};

/// Diagnostic scores for one laid-out diagram.
#[derive(Clone, Debug)]
pub struct Metrics {
    /// Scene width / height after normalize (character cells).
    pub width: usize,
    pub height: usize,
    /// Number of ranks in the layered layout.
    pub ranks: usize,
    /// Node count.
    pub nodes: usize,
    /// Edge count.
    pub edges: usize,
    /// Mean, over ranks with ≥2 real nodes, of the mean absolute deviation
    /// of node cross-axis centers from that rank's median center.
    /// `0` = every multi-node rank is perfectly cross-aligned.
    /// `None` if no multi-node rank exists.
    pub rank_cross_mad: Option<f64>,
    /// Coefficient of variation (stddev/mean) of channel widths between ranks.
    /// `0` = uniform inter-rank gaps. `None` if fewer than two channels or mean 0.
    pub channel_gap_cv: Option<f64>,
    /// Fraction of non-empty routed edges whose polyline is a single straight
    /// H or V segment (`points` length 2, or all points collinear on one axis).
    pub straight_frac: f64,
    /// Mean number of elbows (direction changes) per routed edge.
    pub elbow_mean: f64,
    /// Cross-axis mass balance: `|com − mid| / max(1, half_extent)`.
    /// `0` = box centers' mean sits on the diagram mid-line (cross axis).
    pub balance: f64,
    /// Mirror score in `0..=1` about the cross-axis mid-line using box centers
    /// (and rough size). Higher is more symmetric. Always computed when ≥2 boxes;
    /// interpret only when the graph is a symmetry candidate.
    pub mirror: Option<f64>,
}

/// Layout at default golden width, route, measure.
pub fn measure_graph(g: &Graph, max_width: usize) -> Metrics {
    let placed = crate::layout::layout(g, max_width);
    let mut scene = route::route(g, &placed);
    scene.normalize();
    measure(g, &placed, &scene)
}

/// Measure an already laid-out / routed scene. `scene` should be normalized
/// to `(0,0)` for stable width/height and balance (callers may normalize first).
pub fn measure(g: &Graph, placed: &Placed, scene: &Scene) -> Metrics {
    let bounds = scene.bounds();
    let width = bounds.w.max(0) as usize;
    let height = bounds.h.max(0) as usize;

    Metrics {
        width,
        height,
        ranks: placed.rank_span.len(),
        nodes: g.nodes.len(),
        edges: g.edges.len(),
        rank_cross_mad: rank_cross_mad(placed),
        channel_gap_cv: channel_gap_cv(placed),
        straight_frac: straight_frac(scene),
        elbow_mean: elbow_mean(scene),
        balance: balance(scene, placed.horizontal),
        mirror: mirror_score(scene, placed.horizontal),
    }
}

fn rank_cross_mad(placed: &Placed) -> Option<f64> {
    let mut per_rank: Vec<Vec<i32>> = vec![Vec::new(); placed.rank_span.len()];
    for b in &placed.boxes {
        if b.rank >= per_rank.len() {
            continue;
        }
        // Center on the cross axis in flow space (integer cell coords).
        let center = b.c as i32 + b.clen as i32 / 2;
        per_rank[b.rank].push(center);
    }

    let mut mads = Vec::new();
    for centers in &mut per_rank {
        if centers.len() < 2 {
            continue;
        }
        centers.sort_unstable();
        let med = median_i32(centers);
        let mad = centers
            .iter()
            .map(|&c| (c - med).unsigned_abs() as f64)
            .sum::<f64>()
            / centers.len() as f64;
        mads.push(mad);
    }
    if mads.is_empty() {
        None
    } else {
        Some(mads.iter().sum::<f64>() / mads.len() as f64)
    }
}

fn channel_gap_cv(placed: &Placed) -> Option<f64> {
    if placed.channels.len() < 2 {
        return None;
    }
    let widths: Vec<f64> = placed.channels.iter().map(|c| c.width as f64).collect();
    let mean = widths.iter().sum::<f64>() / widths.len() as f64;
    if mean <= f64::EPSILON {
        return None;
    }
    let var = widths.iter().map(|w| {
        let d = w - mean;
        d * d
    }).sum::<f64>()
        / widths.len() as f64;
    Some(var.sqrt() / mean)
}

fn straight_frac(scene: &Scene) -> f64 {
    if scene.edges.is_empty() {
        return 1.0;
    }
    let straight = scene.edges.iter().filter(|e| is_straight(&e.points)).count();
    straight as f64 / scene.edges.len() as f64
}

fn elbow_mean(scene: &Scene) -> f64 {
    if scene.edges.is_empty() {
        return 0.0;
    }
    let total: usize = scene.edges.iter().map(|e| elbow_count(&e.points)).sum();
    total as f64 / scene.edges.len() as f64
}

fn is_straight(points: &[Point]) -> bool {
    if points.len() < 2 {
        return false;
    }
    if points.len() == 2 {
        return points[0].x == points[1].x || points[0].y == points[1].y;
    }
    // All points share x, or all share y.
    let x0 = points[0].x;
    let y0 = points[0].y;
    points.iter().all(|p| p.x == x0) || points.iter().all(|p| p.y == y0)
}

fn elbow_count(points: &[Point]) -> usize {
    if points.len() < 3 {
        return 0;
    }
    let mut n = 0;
    for w in points.windows(3) {
        let d1 = (w[1].x - w[0].x, w[1].y - w[0].y);
        let d2 = (w[2].x - w[1].x, w[2].y - w[1].y);
        // Direction change (including U-turn).
        let axis1 = (d1.0 != 0, d1.1 != 0);
        let axis2 = (d2.0 != 0, d2.1 != 0);
        if axis1 != axis2 {
            n += 1;
        } else if d1.0.signum() != d2.0.signum() || d1.1.signum() != d2.1.signum() {
            // Same axis but reversed — count as corner-ish kink.
            n += 1;
        }
    }
    n
}

fn balance(scene: &Scene, horizontal: bool) -> f64 {
    if scene.boxes.is_empty() {
        return 0.0;
    }
    let bounds = scene.bounds();
    let (mid, half, com) = if horizontal {
        // Cross axis is Y.
        let mid = bounds.y as f64 + bounds.h as f64 / 2.0;
        let half = (bounds.h as f64 / 2.0).max(1.0);
        let com = scene
            .boxes
            .iter()
            .map(|b| box_center(&b.rect).1 as f64)
            .sum::<f64>()
            / scene.boxes.len() as f64;
        (mid, half, com)
    } else {
        let mid = bounds.x as f64 + bounds.w as f64 / 2.0;
        let half = (bounds.w as f64 / 2.0).max(1.0);
        let com = scene
            .boxes
            .iter()
            .map(|b| box_center(&b.rect).0 as f64)
            .sum::<f64>()
            / scene.boxes.len() as f64;
        (mid, half, com)
    };
    (com - mid).abs() / half
}

/// Pair each box center with the nearest reflected center; map mean distance
/// into `(0, 1]` via `1 / (1 + mean_dist)`.
fn mirror_score(scene: &Scene, horizontal: bool) -> Option<f64> {
    if scene.boxes.len() < 2 {
        return None;
    }
    let bounds = scene.bounds();
    let mid = if horizontal {
        bounds.y as f64 + bounds.h as f64 / 2.0
    } else {
        bounds.x as f64 + bounds.w as f64 / 2.0
    };

    let centers: Vec<(f64, f64, f64, f64)> = scene
        .boxes
        .iter()
        .map(|b| {
            let (cx, cy) = box_center(&b.rect);
            (cx as f64, cy as f64, b.rect.w as f64, b.rect.h as f64)
        })
        .collect();

    let mut total = 0.0;
    for &(cx, cy, w, h) in &centers {
        let (rx, ry) = if horizontal {
            (cx, 2.0 * mid - cy)
        } else {
            (2.0 * mid - cx, cy)
        };
        let mut best = f64::INFINITY;
        for &(ox, oy, ow, oh) in &centers {
            // Distance in center space + mild size penalty so large boxes prefer large partners.
            let d = (rx - ox).abs() + (ry - oy).abs() + (w - ow).abs() * 0.25 + (h - oh).abs() * 0.25;
            if d < best {
                best = d;
            }
        }
        total += best;
    }
    let mean = total / centers.len() as f64;
    Some(1.0 / (1.0 + mean))
}

fn box_center(r: &Rect) -> (i32, i32) {
    (r.x + r.w / 2, r.y + r.h / 2)
}

fn median_i32(sorted: &[i32]) -> i32 {
    let n = sorted.len();
    if n == 0 {
        return 0;
    }
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        // Integer midpoint between the two central values.
        let a = sorted[n / 2 - 1];
        let b = sorted[n / 2];
        a + (b - a) / 2
    }
}

/// Format one metrics row for a fixed-width table (no trailing newline).
pub fn format_row(name: &str, m: &Metrics) -> String {
    format!(
        "{:<20} {:>4}×{:<4} {:>5} {:>8} {:>8} {:>7} {:>7} {:>7} {:>7}",
        trunc(name, 20),
        m.width,
        m.height,
        m.ranks,
        opt_f(m.rank_cross_mad, 2),
        opt_f(m.channel_gap_cv, 2),
        format_f(m.straight_frac, 2),
        format_f(m.elbow_mean, 2),
        format_f(m.balance, 2),
        opt_f(m.mirror, 2),
    )
}

/// Header line matching [`format_row`].
pub fn format_header() -> String {
    format!(
        "{:<20} {:>9} {:>5} {:>8} {:>8} {:>7} {:>7} {:>7} {:>7}",
        "case", "size", "ranks", "crossMad", "gapCv", "straight", "elbows", "balance", "mirror"
    )
}

fn trunc(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}

fn format_f(v: f64, prec: usize) -> String {
    format!("{v:.prec$}")
}

fn opt_f(v: Option<f64>, prec: usize) -> String {
    match v {
        Some(x) => format_f(x, prec),
        None => "-".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn pipeline_is_single_rank_cross_and_mostly_straight() {
        let g = parse::parse(
            "flowchart LR\nA[source] -->|scan| B[tokens] -->|parse| C[ast]\n",
        )
        .unwrap();
        let m = measure_graph(&g, 100);
        assert!(m.width > 0 && m.height > 0);
        // Single node per rank → no multi-node rank MAD.
        assert!(m.rank_cross_mad.is_none());
        assert!(m.straight_frac >= 0.99, "straight={}", m.straight_frac);
        assert!(m.elbow_mean < 0.01, "elbows={}", m.elbow_mean);
    }

    #[test]
    fn fanout_has_multi_node_rank_metric() {
        let g = parse::parse("flowchart LR\nA --> B & C\nB & C --> D\n").unwrap();
        let m = measure_graph(&g, 100);
        // Middle ranks have two nodes; MAD should be defined (0 if aligned).
        assert!(m.rank_cross_mad.is_some());
        assert!(m.mirror.is_some());
        assert!(m.nodes == 4);
    }

    #[test]
    fn elbow_count_detects_corner() {
        let pts = vec![
            Point::new(0, 0),
            Point::new(5, 0),
            Point::new(5, 3),
        ];
        assert_eq!(elbow_count(&pts), 1);
        assert!(!is_straight(&pts));
        assert!(is_straight(&[Point::new(0, 1), Point::new(4, 1)]));
    }
}
