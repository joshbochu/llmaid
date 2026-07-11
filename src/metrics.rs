//! Human-readable reporting for [`crate::audit`].
//!
//! The audit vector is intentionally not collapsed into a scalar. Hard
//! violations come first, then exact relational residuals, then descriptive
//! routing costs. A lower value is better for every reported quality column.

use crate::audit::{self, GeometryAudit};
use crate::layout::Placed;
use crate::parse::Graph;
use crate::scene::Scene;

pub type Metrics = GeometryAudit;

pub fn measure_graph(graph: &Graph, max_width: usize) -> Metrics {
    audit::measure_graph(graph, max_width)
}

pub fn measure(graph: &Graph, placed: &Placed, scene: &Scene) -> Metrics {
    audit::measure(graph, placed, scene)
}

pub fn format_row(name: &str, metrics: &Metrics) -> String {
    format!(
        "{:<20} {:>4}×{:<4} {:>4} {:>6} {:>6} {:>6} {:>6} {:>5}/{:<5} {:>5} {:>5} {:>6}",
        trunc(name, 20),
        metrics.width,
        metrics.height,
        metrics.hard_violations.len(),
        metrics.rank_axis_residual2,
        metrics.mono_centerline_residual2,
        metrics.fork_barycenter_residual2,
        metrics.merge_barycenter_residual2,
        metrics.diamond_mirror_residual2,
        metrics.diamond_motifs,
        metrics.crossing_cells,
        metrics.bends,
        metrics.wire_length,
    )
}

pub fn format_header() -> String {
    format!(
        "{:<20} {:>9} {:>4} {:>6} {:>6} {:>6} {:>6} {:>11} {:>5} {:>5} {:>6}",
        "case",
        "size",
        "hard",
        "rank²",
        "mono²",
        "fork²",
        "merge²",
        "mirror²/n",
        "cross",
        "bends",
        "wire",
    )
}

fn trunc(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    value
        .chars()
        .take(max.saturating_sub(1))
        .collect::<String>()
        + "…"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn pipeline_reports_exact_centerline_and_no_bends() {
        let graph = parse::parse("flowchart LR\nA --> B --> C\n").unwrap();
        let metrics = measure_graph(&graph, 100);

        assert!(metrics.hard_violations.is_empty());
        assert_eq!(metrics.mono_centerline_residual2, 0);
        assert_eq!(metrics.bends, 0);
    }

    #[test]
    fn fanout_reports_one_clean_diamond() {
        let graph = parse::parse("flowchart LR\nA --> B & C\nB & C --> D\n").unwrap();
        let metrics = measure_graph(&graph, 100);

        assert_eq!(metrics.diamond_motifs, 1);
        assert_eq!(metrics.diamond_mirror_residual2, 0);
        assert_eq!(metrics.fork_barycenter_residual2, 0);
        assert_eq!(metrics.merge_barycenter_residual2, 0);
    }
}
