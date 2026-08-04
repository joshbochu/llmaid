//! Central, deterministic bounds for untrusted Mermaid input and raster work.
//!
//! These are deliberately compile-time constants rather than CLI knobs: an
//! agent should receive the same bounded behaviour everywhere, and a caller
//! cannot accidentally turn a terminal render into an unbounded allocation.

use std::fmt;

/// Largest Mermaid document accepted by the CLI streaming reader and by
/// [`crate::diagram::parse`]. Large diagrams should be split at a semantic
/// boundary instead of relying on an ever-larger terminal canvas.
pub const MAX_SOURCE_BYTES: usize = 256 * 1024;
/// Largest accepted `--width` target. This is a fitting preference, not an
/// output cap; B9 can still honestly render wider labels within raster bounds.
pub const MAX_TARGET_WIDTH: usize = 4_096;
/// Maximum total semantic records in one parsed diagram (nodes, edges,
/// groups, events, and their type-specific equivalents).
pub const MAX_SEMANTIC_ELEMENTS: usize = 4_096;
/// Maximum recursive semantic nesting for subgraphs, sequence fragments, and
/// indentation-defined mindmaps.
pub const MAX_NESTING_DEPTH: usize = 128;
/// Largest accepted normalized raster axis.
pub const MAX_CANVAS_DIMENSION: usize = 16_384;
/// Largest accepted normalized raster area. The lower area cap bounds both
/// memory use and work even when each individual axis is valid.
pub const MAX_CANVAS_CELLS: usize = 1_000_000;

/// A stable, actionable refusal emitted before an allocation or unbounded
/// parser/layout traversal can occur.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceLimit {
    pub resource: &'static str,
    pub observed: usize,
    pub limit: usize,
    pub repair: &'static str,
}

impl ResourceLimit {
    pub const fn new(
        resource: &'static str,
        observed: usize,
        limit: usize,
        repair: &'static str,
    ) -> Self {
        Self {
            resource,
            observed,
            limit,
            repair,
        }
    }

    pub const fn source_bytes(observed: usize) -> Self {
        Self::new(
            "source bytes",
            observed,
            MAX_SOURCE_BYTES,
            "split or reduce the diagram",
        )
    }

    pub const fn target_width(observed: usize) -> Self {
        Self::new(
            "target width",
            observed,
            MAX_TARGET_WIDTH,
            "use a smaller --width value",
        )
    }

    pub const fn semantic_elements(observed: usize) -> Self {
        Self::new(
            "semantic elements",
            observed,
            MAX_SEMANTIC_ELEMENTS,
            "split or reduce the diagram",
        )
    }

    pub const fn nesting_depth(observed: usize) -> Self {
        Self::new(
            "nesting depth",
            observed,
            MAX_NESTING_DEPTH,
            "flatten or split the diagram",
        )
    }

    pub const fn canvas_width(observed: usize) -> Self {
        Self::new(
            "canvas width",
            observed,
            MAX_CANVAS_DIMENSION,
            "shorten labels or split the diagram",
        )
    }

    pub const fn canvas_height(observed: usize) -> Self {
        Self::new(
            "canvas height",
            observed,
            MAX_CANVAS_DIMENSION,
            "split the diagram into smaller stages",
        )
    }

    pub const fn canvas_cells(observed: usize) -> Self {
        Self::new(
            "canvas cells",
            observed,
            MAX_CANVAS_CELLS,
            "shorten labels or split the diagram",
        )
    }

    pub const fn canvas_allocation(observed: usize) -> Self {
        Self::new(
            "canvas allocation",
            observed,
            MAX_CANVAS_CELLS,
            "split the diagram and retry",
        )
    }
}

impl fmt::Display for ResourceLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "resource limit exceeded: {} is {} (limit {}); {}",
            self.resource, self.observed, self.limit, self.repair
        )
    }
}

impl std::error::Error for ResourceLimit {}

pub fn validate_source_bytes(observed: usize) -> Result<(), ResourceLimit> {
    if observed > MAX_SOURCE_BYTES {
        Err(ResourceLimit::source_bytes(observed))
    } else {
        Ok(())
    }
}

pub fn validate_target_width(observed: usize) -> Result<(), ResourceLimit> {
    if observed > MAX_TARGET_WIDTH {
        Err(ResourceLimit::target_width(observed))
    } else {
        Ok(())
    }
}

pub fn validate_semantic_elements(observed: usize) -> Result<(), ResourceLimit> {
    if observed > MAX_SEMANTIC_ELEMENTS {
        Err(ResourceLimit::semantic_elements(observed))
    } else {
        Ok(())
    }
}

pub fn validate_nesting_depth(observed: usize) -> Result<(), ResourceLimit> {
    if observed > MAX_NESTING_DEPTH {
        Err(ResourceLimit::nesting_depth(observed))
    } else {
        Ok(())
    }
}

/// Check dimensions and area before creating a canvas. `checked_mul` keeps a
/// hostile programmatic Scene from wrapping an area into a small allocation.
pub fn validate_canvas(width: usize, height: usize) -> Result<usize, ResourceLimit> {
    // Compute this first so a programmatic Scene with hostile dimensions is
    // rejected as arithmetic overflow rather than relying on a later vector
    // length calculation to wrap.
    let cells = width
        .checked_mul(height)
        .ok_or_else(|| ResourceLimit::canvas_cells(usize::MAX))?;
    if width > MAX_CANVAS_DIMENSION {
        return Err(ResourceLimit::canvas_width(width));
    }
    if height > MAX_CANVAS_DIMENSION {
        return Err(ResourceLimit::canvas_height(height));
    }
    if cells > MAX_CANVAS_CELLS {
        return Err(ResourceLimit::canvas_cells(cells));
    }
    Ok(cells)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_canvas_area_refuses_overflow_and_oversize_without_allocating() {
        assert_eq!(validate_canvas(1_000, 1_000), Ok(1_000_000));
        assert_eq!(
            validate_canvas(1_001, 1_000).unwrap_err(),
            ResourceLimit::canvas_cells(1_001_000)
        );
        assert_eq!(
            validate_canvas(usize::MAX, 2).unwrap_err(),
            ResourceLimit::canvas_cells(usize::MAX)
        );
    }
}
