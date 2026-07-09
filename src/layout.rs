//! Sugiyama layered layout in integer character-grid coordinates.
//!
//! Works in "flow space": `f` is the rank axis, `c` the cross axis. For LR
//! diagrams flow is screen-x; for TB it is screen-y. `render.rs` maps flow
//! space to the screen per direction, so layout and routing are written once.
//!
//! Determinism: all iteration is over Vecs in declaration order; ties break
//! by declaration index. No HashMap iteration anywhere.

use crate::parse::{Dir, Graph};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Padding inside a box between border and label, in flow-space cross terms
/// this is only horizontal on screen (label lines are padded when rendered).
pub const PAD: usize = 1;
pub const EDGE_LABEL_PAD: usize = 2;
/// Padding between member boxes and subgraph frame (flow / cross).
pub const CLUSTER_PAD: usize = 1;
/// Extra strip along the screen-top of a cluster for the title.
pub const CLUSTER_TITLE_STRIP: usize = 1;

/// Fit mode for the B9 overflow ladder. Labels wrap only when `label_cols` is
/// set (B10: no arbitrary wrap under a comfortable width budget).
#[derive(Clone, Copy, Debug)]
struct Fit {
    compact: bool,
    /// Max display columns per node label line; `None` means no forced wrap.
    label_cols: Option<usize>,
}

const FIT_NORMAL: Fit = Fit {
    compact: false,
    label_cols: None,
};
const FIT_COMPACT: Fit = Fit {
    compact: true,
    label_cols: None,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Real(usize),  // node index
    Dummy(usize), // edge index passing through this rank
}

#[derive(Debug)]
pub struct BoxGeom {
    pub rank: usize,
    pub f: usize,
    pub c: usize,
    pub flen: usize, // size along flow axis
    pub clen: usize, // size along cross axis
    pub lines: Vec<String>,
}

/// One channel segment of an edge, between adjacent ranks.
#[derive(Debug)]
pub struct Seg {
    pub channel: usize,
    /// Border cell where the segment leaves the source (f = border coord).
    pub from: (usize, usize),
    /// Border cell where it enters the target.
    pub to: (usize, usize),
    /// Vertical jog track index within the channel, when from.1 != to.1.
    pub track: Option<usize>,
}

#[derive(Debug)]
pub struct Channel {
    pub start: usize,
    pub width: usize,
    pub label_zone: usize,
}

impl Channel {
    pub fn track_f(&self, t: usize) -> usize {
        self.start + self.label_zone + 1 + 2 * t
    }
}

/// Axis-aligned subgraph frame in flow-space (same coords as `BoxGeom`).
#[derive(Debug)]
pub struct ClusterGeom {
    pub subgraph: usize,
    pub f: usize,
    pub c: usize,
    pub flen: usize,
    pub clen: usize,
    pub title: String,
}

#[derive(Debug)]
pub struct Placed {
    pub horizontal: bool, // LR/RL (flow = screen x)
    pub flipped: bool,    // RL/BT (flow axis mirrored on screen)
    pub boxes: Vec<BoxGeom>,
    pub clusters: Vec<ClusterGeom>,
    /// Per edge: channel segments in flow order. Empty for self-loops/back edges.
    pub segs: Vec<Vec<Seg>>,
    pub channels: Vec<Channel>,
    /// Per intermediate rank of a long edge: pass-through cross position.
    /// Indexed parallel to `segs` boundaries via (edge, rank).
    pub pass_through: Vec<(usize, usize, usize)>, // (edge, rank, cross)
    pub self_loops: Vec<usize>,
    pub back_edges: Vec<usize>,
    pub rank_span: Vec<(usize, usize)>, // (start, width) of each rank column
    pub flow_extent: usize,
    pub cross_extent: usize,
}

/// Lay out `g` aiming for at most `max_width` screen columns (B8/B9).
/// Degradation: normal → compact gaps → wrap labels → over-width (never
/// truncate, never fail).
pub fn layout(g: &Graph, max_width: usize) -> Placed {
    let normal = layout_fit(g, FIT_NORMAL);
    if approx_width(g, &normal) <= max_width {
        return normal;
    }
    let compact = layout_fit(g, FIT_COMPACT);
    if approx_width(g, &compact) <= max_width {
        return compact;
    }
    // Wrap under pressure: target content width from budget and rank count.
    let nranks = compact.rank_span.len().max(1);
    let content = max_width
        .saturating_sub(nranks.saturating_mul(2)) // borders
        .saturating_sub(nranks.saturating_sub(1).saturating_mul(3)) // min channels
        / nranks;
    let label_cols = content.saturating_sub(2 * PAD).max(4);
    layout_fit(
        g,
        Fit {
            compact: true,
            label_cols: Some(label_cols),
        },
    )
}

fn approx_width(g: &Graph, p: &Placed) -> usize {
    let base = if p.horizontal {
        p.flow_extent
    } else {
        p.cross_extent
    };
    let mut right = 0usize;
    if !p.back_edges.is_empty() && !p.horizontal {
        right = right.max(6 + p.back_edges.len() * 2);
    }
    if !p.self_loops.is_empty() {
        right = right.max(6);
    }
    for &ei in p.back_edges.iter().chain(&p.self_loops) {
        let label_w = g.edges[ei]
            .label
            .as_deref()
            .map(UnicodeWidthStr::width)
            .unwrap_or(0);
        if label_w > 0 {
            let is_horizontal_back = p.horizontal && p.back_edges.contains(&ei);
            if !is_horizontal_back {
                right = right.max(label_w + 8 + 2 * EDGE_LABEL_PAD + p.back_edges.len() * 2);
            }
        }
    }
    base + right
}

fn layout_fit(g: &Graph, fit: Fit) -> Placed {
    let dir = g.direction();
    let horizontal = matches!(dir, Dir::LR | Dir::RL);
    let flipped = matches!(dir, Dir::RL | Dir::BT);
    let n = g.nodes.len();

    // --- Classify edges: self-loops, and back edges via DFS on declaration order.
    let mut self_loops = Vec::new();
    let mut reversed = vec![false; g.edges.len()];
    for (ei, e) in g.edges.iter().enumerate() {
        if e.from == e.to {
            self_loops.push(ei);
        }
    }
    mark_back_edges(g, &mut reversed);

    // Ranking adjacency: forward edges, with back edges flipped.
    let ranked_edge = |ei: usize| -> Option<(usize, usize)> {
        let e = &g.edges[ei];
        if e.from == e.to {
            return None;
        }
        Some(if reversed[ei] {
            (e.to, e.from)
        } else {
            (e.from, e.to)
        })
    };

    // --- Rank assignment: longest path (Kahn).
    let mut indeg = vec![0usize; n];
    for ei in 0..g.edges.len() {
        if let Some((_, t)) = ranked_edge(ei) {
            indeg[t] += 1;
        }
    }
    let mut rank = vec![0usize; n];
    let mut queue: Vec<usize> = (0..n).filter(|&v| indeg[v] == 0).collect();
    let mut qi = 0;
    while qi < queue.len() {
        let v = queue[qi];
        qi += 1;
        for ei in 0..g.edges.len() {
            if let Some((s, t)) = ranked_edge(ei) {
                if s == v {
                    rank[t] = rank[t].max(rank[v] + 1);
                    indeg[t] -= 1;
                    if indeg[t] == 0 {
                        queue.push(t);
                    }
                }
            }
        }
    }

    let nranks = rank.iter().copied().max().unwrap_or(0) + 1;

    // --- Box sizes from labels (optional wrap under width pressure — B9/B10).
    let mut boxes: Vec<BoxGeom> = g
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let lines: Vec<String> = match fit.label_cols {
                Some(cols) => wrap_label(&node.label, cols),
                None => node.label.split('\n').map(str::to_string).collect(),
            };
            let text_w = lines.iter().map(|l| l.width()).max().unwrap_or(1).max(1);
            let w = text_w + 2 * PAD + 2;
            let h = lines.len() + 2;
            let (flen, clen) = if horizontal { (w, h) } else { (h, w) };
            BoxGeom {
                rank: rank[i],
                f: 0,
                c: 0,
                flen,
                clen,
                lines,
            }
        })
        .collect();

    // B12: grow the cross-axis so each forward edge can own a distinct port.
    // Single-line boxes only have one interior row; without this, parallel and
    // multi-out edges collapse onto the same path and overwrite labels.
    for ni in 0..n {
        let mut out_d = 0usize;
        let mut in_d = 0usize;
        for (ei, e) in g.edges.iter().enumerate() {
            if e.from == e.to || reversed[ei] {
                continue;
            }
            if e.from == ni {
                out_d += 1;
            }
            if e.to == ni {
                in_d += 1;
            }
        }
        let need = out_d.max(in_d);
        if need > 0 {
            boxes[ni].clen = boxes[ni].clen.max(need + 2);
        }
    }

    // --- Build rank slot lists with dummies for long edges.
    let mut ranks: Vec<Vec<Slot>> = vec![Vec::new(); nranks];
    for (i, b) in boxes.iter().enumerate() {
        ranks[b.rank].push(Slot::Real(i));
    }
    // (edge, rank) -> position recorded later; collect dummies in edge order.
    let mut edge_spans: Vec<Option<(usize, usize)>> = vec![None; g.edges.len()];
    for ei in 0..g.edges.len() {
        if let Some((s, t)) = ranked_edge(ei) {
            edge_spans[ei] = Some((rank[s], rank[t]));
            for r in rank[s] + 1..rank[t] {
                ranks[r].push(Slot::Dummy(ei));
            }
        }
    }

    // --- Crossing reduction: barycenter sweeps.
    order_by_barycenter(g, &mut ranks, &rank, &reversed, &edge_spans);

    // --- Cross sizes/gaps.
    let max_label = g
        .edges
        .iter()
        .map(|e| e.label.as_deref().map(|l| l.width()).unwrap_or(0))
        .max()
        .unwrap_or(0);
    let base_gap = if fit.compact {
        if horizontal {
            0
        } else {
            2.max(max_label.saturating_add(1).min(3))
        }
    } else if horizontal {
        1
    } else {
        // Vertical flow: cross-axis gap between siblings — not edge-label width
        // (labels sit beside the vertical run; see render).
        let _ = max_label;
        2
    };
    let edge_label_pad = if fit.compact {
        1
    } else {
        EDGE_LABEL_PAD
    };
    // Vertical channels only need a short band for beside-shaft labels.
    let channel_min = if fit.compact {
        3
    } else if horizontal {
        5
    } else {
        3
    };
    let has_self_loop = |ni: usize| self_loops.iter().any(|&ei| g.edges[ei].from == ni);

    let slot_clen = |slot: &Slot, boxes: &[BoxGeom]| -> usize {
        match slot {
            Slot::Real(i) => boxes[*i].clen,
            Slot::Dummy(_) => 1,
        }
    };

    // Initial stacking.
    let mut slot_cross: Vec<Vec<usize>> = Vec::new();
    for rslots in &ranks {
        let mut cur = 0usize;
        let mut row = Vec::new();
        for slot in rslots {
            row.push(cur);
            let mut adv = slot_clen(slot, &boxes) + base_gap;
            if let Slot::Real(i) = slot {
                if horizontal && has_self_loop(*i) {
                    adv += 2; // room for the loop return below the box
                }
            }
            cur += adv;
        }
        slot_cross.push(row);
    }

    // Alignment sweeps: pull slots toward the barycenter of their neighbors.
    for sweep in 0..4 {
        let range: Vec<usize> = if sweep % 2 == 0 {
            (0..nranks).collect()
        } else {
            (0..nranks).rev().collect()
        };
        for &r in &range {
            let desired: Vec<Option<usize>> = ranks[r]
                .iter()
                .map(|slot| {
                    neighbor_centers(
                        g,
                        slot,
                        r,
                        &ranks,
                        &slot_cross,
                        &boxes,
                        &reversed,
                        &edge_spans,
                    )
                })
                .collect();
            legalize(
                &ranks[r],
                &mut slot_cross[r],
                &desired,
                &boxes,
                base_gap,
                horizontal,
                &self_loops,
                g,
            );
        }
    }

    for (r, rslots) in ranks.iter().enumerate() {
        for (si, slot) in rslots.iter().enumerate() {
            if let Slot::Real(i) = slot {
                boxes[*i].c = slot_cross[r][si];
            }
        }
    }

    // Phase 0.4: straighten mono-rank chains so simple A→B columns share a
    // centerline (fewer needless elbows when each rank has one real node).
    straighten_mono_chains(g, &ranks, &mut boxes, &reversed);

    // Keep slot_cross in sync for reals after straightening (dummies unchanged).
    for (r, rslots) in ranks.iter().enumerate() {
        for (si, slot) in rslots.iter().enumerate() {
            if let Slot::Real(i) = slot {
                slot_cross[r][si] = boxes[*i].c;
            }
        }
    }

    let mut pass_through = Vec::new();
    for (r, rslots) in ranks.iter().enumerate() {
        for (si, slot) in rslots.iter().enumerate() {
            if let Slot::Dummy(ei) = slot {
                pass_through.push((*ei, r, slot_cross[r][si]));
            }
        }
    }

    let mut cross_extent = 0usize;
    for r in 0..nranks {
        for (si, slot) in ranks[r].iter().enumerate() {
            let end = match slot {
                Slot::Real(i) => boxes[*i].c + boxes[*i].clen,
                Slot::Dummy(_) => slot_cross[r][si] + 1,
            };
            cross_extent = cross_extent.max(end);
        }
    }

    // --- Ports and channel segments.
    let dummy_cross = |ei: usize, r: usize| -> usize {
        pass_through
            .iter()
            .find(|&&(e, pr, _)| e == ei && pr == r)
            .map(|&(_, _, c)| c)
            .expect("dummy exists for spanned rank")
    };

    // For each node, forward out-edges and in-edges in a stable order.
    let mut segs: Vec<Vec<Seg>> = (0..g.edges.len()).map(|_| Vec::new()).collect();
    let mut channel_edges: Vec<Vec<usize>> = vec![Vec::new(); nranks.saturating_sub(1)];

    // First cross positions of each edge's endpoints per channel; ports assigned
    // per node so several edges on one side spread across interior rows.
    let mut out_port = port_map(g, &boxes, &reversed, &edge_spans, &pass_through, true);
    let mut in_port = port_map(g, &boxes, &reversed, &edge_spans, &pass_through, false);
    // Snap single-attachment edges to a shared cross so mono-chains stay straight.
    snap_mono_edge_ports(g, &boxes, &reversed, &mut out_port, &mut in_port);

    for ei in 0..g.edges.len() {
        let Some((rs, rt)) = edge_spans[ei] else {
            continue;
        };
        if reversed[ei] {
            continue; // routed as a perimeter back edge
        }
        for r in rs..rt {
            let from_c = if r == rs {
                out_port[ei]
            } else {
                dummy_cross(ei, r)
            };
            let to_c = if r + 1 == rt {
                in_port[ei]
            } else {
                dummy_cross(ei, r + 1)
            };
            segs[ei].push(Seg {
                channel: r,
                from: (0, from_c), // f filled in after flow assignment
                to: (0, to_c),
                track: None,
            });
            channel_edges[r].push(ei);
        }
    }

    // --- Channel widths: label zone + jog tracks.
    let mut channels = Vec::new();
    for r in 0..nranks.saturating_sub(1) {
        let mut label_zone = 0usize;
        let mut tracks = 0usize;
        for &ei in &channel_edges[r] {
            let seg = segs[ei].iter().find(|s| s.channel == r).unwrap();
            let labeled_here =
                g.edges[ei].label.is_some() && edge_spans[ei].map(|(rs, _)| rs) == Some(r);
            if labeled_here {
                if horizontal {
                    // Label lies along the channel (flow axis): need full text width.
                    // +2 for the spaces drawn around the word (` scan `).
                    label_zone = label_zone.max(
                        g.edges[ei].label.as_deref().unwrap().width() + 2 + 2 * edge_label_pad,
                    );
                } else {
                    // Vertical flow: label is a single horizontal band beside the
                    // shaft — only a few rows of channel height.
                    label_zone = label_zone.max(3);
                }
            }
            if seg.from.1 != seg.to.1 {
                tracks += 1;
            }
        }
        let slack = if fit.compact {
            2
        } else if horizontal {
            3
        } else {
            1
        };
        let width = channel_min.max(label_zone + 2 * tracks + slack);
        channels.push(Channel {
            start: 0,
            width,
            label_zone,
        });
    }

    // Assign track indices deterministically (by source cross, then edge index).
    for r in 0..channels.len() {
        let mut benders: Vec<usize> = channel_edges[r]
            .iter()
            .copied()
            .filter(|&ei| {
                let s = segs[ei].iter().find(|s| s.channel == r).unwrap();
                s.from.1 != s.to.1
            })
            .collect();
        benders.sort_by_key(|&ei| {
            let s = segs[ei].iter().find(|s| s.channel == r).unwrap();
            (s.from.1, ei)
        });
        for (t, &ei) in benders.iter().enumerate() {
            segs[ei].iter_mut().find(|s| s.channel == r).unwrap().track = Some(t);
        }
    }

    // --- Flow positions: rank columns separated by channels.
    let mut rank_span = Vec::new();
    let mut f = 0usize;
    for r in 0..nranks {
        let width = ranks[r]
            .iter()
            .map(|s| match s {
                Slot::Real(i) => boxes[*i].flen,
                Slot::Dummy(_) => 1,
            })
            .max()
            .unwrap_or(1);
        rank_span.push((f, width));
        if r < channels.len() {
            channels[r].start = f + width;
            f += width + channels[r].width;
        } else {
            f += width;
        }
    }
    let flow_extent = f;

    for b in boxes.iter_mut() {
        let (start, width) = rank_span[b.rank];
        b.f = start + (width - b.flen) / 2;
    }

    // Fill in segment endpoint flow coords (border cells).
    for ei in 0..g.edges.len() {
        let Some((rs, rt)) = edge_spans[ei] else {
            continue;
        };
        if reversed[ei] {
            continue;
        }
        for seg in segs[ei].iter_mut() {
            let r = seg.channel;
            seg.from.0 = if r == rs {
                let b = &boxes[g.edges[ei].from];
                b.f + b.flen - 1
            } else {
                let (start, width) = rank_span[r];
                start + width - 1
            };
            seg.to.0 = if r + 1 == rt {
                boxes[g.edges[ei].to].f
            } else {
                rank_span[r + 1].0
            };
        }
    }

    let back_edges: Vec<usize> = (0..g.edges.len())
        .filter(|&ei| reversed[ei] && g.edges[ei].from != g.edges[ei].to)
        .collect();

    // Make room for cluster padding/title when members sit on the origin edge.
    let (shift_f, shift_c) = cluster_origin_shift(g, &boxes, horizontal);
    let mut boxes = boxes;
    let mut segs = segs;
    let mut channels = channels;
    let mut pass_through = pass_through;
    let mut rank_span = rank_span;
    let mut flow_extent = flow_extent;
    let mut cross_extent = cross_extent;
    if shift_f > 0 || shift_c > 0 {
        shift_placed(
            &mut boxes,
            &mut segs,
            &mut channels,
            &mut pass_through,
            &mut rank_span,
            &mut flow_extent,
            &mut cross_extent,
            shift_f,
            shift_c,
        );
    }

    let (clusters, flow_extent, cross_extent) =
        place_clusters(g, &boxes, horizontal, flow_extent, cross_extent);

    Placed {
        horizontal,
        flipped,
        boxes,
        clusters,
        segs,
        channels,
        pass_through,
        self_loops,
        back_edges,
        rank_span,
        flow_extent,
        cross_extent,
    }
}

/// Direct members plus all nodes in descendant subgraphs.
fn subgraph_members_deep(g: &Graph, sgi: usize) -> Vec<usize> {
    let mut out = g.subgraphs[sgi].members.clone();
    for (i, sg) in g.subgraphs.iter().enumerate() {
        if sg.parent == Some(sgi) {
            out.extend(subgraph_members_deep(g, i));
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// How far to shift flow/cross so every subgraph has room for pad + title.
fn cluster_origin_shift(g: &Graph, boxes: &[BoxGeom], horizontal: bool) -> (usize, usize) {
    let mut shift_f = 0usize;
    let mut shift_c = 0usize;
    for si in 0..g.subgraphs.len() {
        let members = subgraph_members_deep(g, si);
        if members.is_empty() {
            continue;
        }
        let min_f = members.iter().map(|&i| boxes[i].f).min().unwrap();
        let min_c = members.iter().map(|&i| boxes[i].c).min().unwrap();
        let has_child = g.subgraphs.iter().any(|s| s.parent == Some(si));
        let top_strip = CLUSTER_TITLE_STRIP + usize::from(has_child);
        if horizontal {
            shift_c = shift_c.max((CLUSTER_PAD + top_strip).saturating_sub(min_c));
            shift_f = shift_f.max(CLUSTER_PAD.saturating_sub(min_f));
        } else {
            shift_f = shift_f.max((CLUSTER_PAD + top_strip).saturating_sub(min_f));
            shift_c = shift_c.max(CLUSTER_PAD.saturating_sub(min_c));
        }
    }
    (shift_f, shift_c)
}

#[allow(clippy::too_many_arguments)]
fn shift_placed(
    boxes: &mut [BoxGeom],
    segs: &mut [Vec<Seg>],
    channels: &mut [Channel],
    pass_through: &mut [(usize, usize, usize)],
    rank_span: &mut [(usize, usize)],
    flow_extent: &mut usize,
    cross_extent: &mut usize,
    df: usize,
    dc: usize,
) {
    for b in boxes.iter_mut() {
        b.f += df;
        b.c += dc;
    }
    for edge_segs in segs.iter_mut() {
        for s in edge_segs.iter_mut() {
            s.from.0 += df;
            s.from.1 += dc;
            s.to.0 += df;
            s.to.1 += dc;
        }
    }
    for ch in channels.iter_mut() {
        ch.start += df;
    }
    for p in pass_through.iter_mut() {
        p.2 += dc;
    }
    for rs in rank_span.iter_mut() {
        rs.0 += df;
    }
    *flow_extent += df;
    *cross_extent += dc;
}

/// Bounding-box clusters around subgraph members with padding + title strip.
fn place_clusters(
    g: &Graph,
    boxes: &[BoxGeom],
    horizontal: bool,
    mut flow_extent: usize,
    mut cross_extent: usize,
) -> (Vec<ClusterGeom>, usize, usize) {
    let mut clusters = Vec::new();
    for si in 0..g.subgraphs.len() {
        let members = subgraph_members_deep(g, si);
        if members.is_empty() {
            continue;
        }
        let mut f0 = usize::MAX;
        let mut f1 = 0usize;
        let mut c0 = usize::MAX;
        let mut c1 = 0usize;
        for &ni in &members {
            let b = &boxes[ni];
            f0 = f0.min(b.f);
            f1 = f1.max(b.f + b.flen);
            c0 = c0.min(b.c);
            c1 = c1.max(b.c + b.clen);
        }
        if f0 == usize::MAX {
            continue;
        }
        // Parents need an extra top strip so nested titles don't share a row.
        let has_child = g.subgraphs.iter().any(|s| s.parent == Some(si));
        let top_strip = CLUSTER_TITLE_STRIP + if has_child { 1 } else { 0 };
        // Screen-top title strip: smaller cross for LR, smaller flow for TB.
        if horizontal {
            c0 = c0.saturating_sub(CLUSTER_PAD + top_strip);
            c1 += CLUSTER_PAD;
            f0 = f0.saturating_sub(CLUSTER_PAD);
            f1 += CLUSTER_PAD;
        } else {
            f0 = f0.saturating_sub(CLUSTER_PAD + top_strip);
            f1 += CLUSTER_PAD;
            c0 = c0.saturating_sub(CLUSTER_PAD);
            c1 += CLUSTER_PAD;
        }
        let title = g.subgraphs[si].title.clone();
        let tw = title.width() + 2; // ` title `
        // Leave corner cells free for box-drawing (+2).
        let need = tw + 2;
        if horizontal {
            if f1.saturating_sub(f0) < need {
                f1 = f0 + need;
            }
        } else if c1.saturating_sub(c0) < need {
            c1 = c0 + need;
        }
        flow_extent = flow_extent.max(f1);
        cross_extent = cross_extent.max(c1);
        clusters.push(ClusterGeom {
            subgraph: si,
            f: f0,
            c: c0,
            flen: f1.saturating_sub(f0).max(1),
            clen: c1.saturating_sub(c0).max(1),
            title,
        });
    }
    // Shallowest first (outer frames drawn under nested ones).
    clusters.sort_by_key(|cl| {
        let mut d = 0usize;
        let mut p = g.subgraphs[cl.subgraph].parent;
        while let Some(pi) = p {
            d += 1;
            p = g.subgraphs[pi].parent;
        }
        d
    });
    (clusters, flow_extent, cross_extent)
}

/// For forward edges that are each endpoint's only attachment, force a shared
/// port cross (clamped into each box) so the shaft does not jog.
fn snap_mono_edge_ports(
    g: &Graph,
    boxes: &[BoxGeom],
    reversed: &[bool],
    out_port: &mut [usize],
    in_port: &mut [usize],
) {
    let mut out_deg = vec![0usize; g.nodes.len()];
    let mut in_deg = vec![0usize; g.nodes.len()];
    for (ei, e) in g.edges.iter().enumerate() {
        if e.from == e.to || reversed[ei] {
            continue;
        }
        out_deg[e.from] += 1;
        in_deg[e.to] += 1;
    }
    for (ei, e) in g.edges.iter().enumerate() {
        if e.from == e.to || reversed[ei] {
            continue;
        }
        if out_deg[e.from] != 1 || in_deg[e.to] != 1 {
            continue;
        }
        let mid = (out_port[ei] + in_port[ei]) / 2;
        let clamp = |ni: usize, p: usize| {
            let b = &boxes[ni];
            let lo = b.c + 1;
            let hi = b.c + b.clen.saturating_sub(2);
            if lo > hi {
                b.c + b.clen / 2
            } else {
                p.clamp(lo, hi)
            }
        };
        out_port[ei] = clamp(e.from, mid);
        in_port[ei] = clamp(e.to, mid);
        // If clamps still disagree (non-overlapping boxes), prefer source port
        // and re-clamp into the target so at least one side is exact.
        if out_port[ei] != in_port[ei] {
            in_port[ei] = clamp(e.to, out_port[ei]);
            if out_port[ei] != in_port[ei] {
                out_port[ei] = clamp(e.from, in_port[ei]);
            }
        }
    }
}

/// When consecutive ranks each hold a single real node connected by a forward
/// edge, share a centerline so the edge can run straight.
fn straighten_mono_chains(
    g: &Graph,
    ranks: &[Vec<Slot>],
    boxes: &mut [BoxGeom],
    reversed: &[bool],
) {
    for r in 0..ranks.len().saturating_sub(1) {
        let reals = |rr: usize| -> Vec<usize> {
            ranks[rr]
                .iter()
                .filter_map(|s| match s {
                    Slot::Real(i) => Some(*i),
                    Slot::Dummy(_) => None,
                })
                .collect()
        };
        let a = reals(r);
        let b = reals(r + 1);
        if a.len() != 1 || b.len() != 1 {
            continue;
        }
        let (ai, bi) = (a[0], b[0]);
        let connected = g.edges.iter().enumerate().any(|(ei, e)| {
            !reversed[ei]
                && e.from != e.to
                && e.from == ai
                && e.to == bi
                && boxes[ai].rank + 1 == boxes[bi].rank
        });
        if !connected {
            continue;
        }
        let ca = boxes[ai].c + boxes[ai].clen / 2;
        let cb = boxes[bi].c + boxes[bi].clen / 2;
        let mid = (ca + cb) / 2;
        boxes[ai].c = mid.saturating_sub(boxes[ai].clen / 2);
        boxes[bi].c = mid.saturating_sub(boxes[bi].clen / 2);
    }
}

/// Width-aware wrap for node labels under B9 pressure. Never truncates; breaks
/// on spaces when possible, otherwise on grapheme-ish char boundaries.
fn wrap_label(text: &str, max_cols: usize) -> Vec<String> {
    let max_cols = max_cols.max(1);
    let mut out = Vec::new();
    for para in text.split('\n') {
        if para.is_empty() {
            out.push(String::new());
            continue;
        }
        if para.width() <= max_cols {
            out.push(para.to_string());
            continue;
        }
        let mut cur = String::new();
        let mut cur_w = 0usize;
        for word in para.split(' ') {
            let ww = word.width();
            if cur.is_empty() {
                if ww <= max_cols {
                    cur = word.to_string();
                    cur_w = ww;
                } else {
                    // Hard-break an overlong token.
                    for ch in word.chars() {
                        let cw = ch.width().unwrap_or(1).max(1);
                        if cur_w + cw > max_cols && !cur.is_empty() {
                            out.push(std::mem::take(&mut cur));
                            cur_w = 0;
                        }
                        cur.push(ch);
                        cur_w += cw;
                    }
                }
                continue;
            }
            if cur_w + 1 + ww <= max_cols {
                cur.push(' ');
                cur.push_str(word);
                cur_w += 1 + ww;
            } else {
                out.push(std::mem::take(&mut cur));
                cur_w = 0;
                if ww <= max_cols {
                    cur = word.to_string();
                    cur_w = ww;
                } else {
                    for ch in word.chars() {
                        let cw = ch.width().unwrap_or(1).max(1);
                        if cur_w + cw > max_cols && !cur.is_empty() {
                            out.push(std::mem::take(&mut cur));
                            cur_w = 0;
                        }
                        cur.push(ch);
                        cur_w += cw;
                    }
                }
            }
        }
        if !cur.is_empty() {
            out.push(cur);
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// DFS in declaration order; an edge to a node on the current stack is a back edge.
fn mark_back_edges(g: &Graph, reversed: &mut [bool]) {
    let n = g.nodes.len();
    let mut state = vec![0u8; n]; // 0 unvisited, 1 on stack, 2 done
    // Iterative DFS carrying the edge list index per node.
    let out: Vec<Vec<usize>> = {
        let mut out = vec![Vec::new(); n];
        for (ei, e) in g.edges.iter().enumerate() {
            if e.from != e.to {
                out[e.from].push(ei);
            }
        }
        out
    };
    for root in 0..n {
        if state[root] != 0 {
            continue;
        }
        let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
        state[root] = 1;
        while let Some(&mut (v, ref mut next)) = stack.last_mut() {
            if *next < out[v].len() {
                let ei = out[v][*next];
                *next += 1;
                let t = g.edges[ei].to;
                match state[t] {
                    0 => {
                        state[t] = 1;
                        stack.push((t, 0));
                    }
                    1 => reversed[ei] = true,
                    _ => {}
                }
            } else {
                state[v] = 2;
                stack.pop();
            }
        }
    }
}

fn slot_neighbors(
    g: &Graph,
    slot: &Slot,
    r: usize,
    reversed: &[bool],
    edge_spans: &[Option<(usize, usize)>],
    toward_prev: bool,
) -> Vec<Slot> {
    let mut out = Vec::new();
    match slot {
        Slot::Real(i) => {
            for (ei, e) in g.edges.iter().enumerate() {
                if e.from == e.to || reversed[ei] {
                    continue;
                }
                let Some((rs, rt)) = edge_spans[ei] else {
                    continue;
                };
                if toward_prev && e.to == *i && rt == r {
                    out.push(if rt - rs == 1 {
                        Slot::Real(e.from)
                    } else {
                        Slot::Dummy(ei)
                    });
                }
                if !toward_prev && e.from == *i && rs == r {
                    out.push(if rt - rs == 1 {
                        Slot::Real(e.to)
                    } else {
                        Slot::Dummy(ei)
                    });
                }
            }
        }
        Slot::Dummy(ei) => {
            let e = &g.edges[*ei];
            let (rs, rt) = edge_spans[*ei].unwrap();
            if toward_prev {
                out.push(if r - 1 == rs {
                    Slot::Real(e.from)
                } else {
                    Slot::Dummy(*ei)
                });
            } else {
                out.push(if r + 1 == rt {
                    Slot::Real(e.to)
                } else {
                    Slot::Dummy(*ei)
                });
            }
        }
    }
    out
}

fn order_by_barycenter(
    g: &Graph,
    ranks: &mut [Vec<Slot>],
    _rank: &[usize],
    reversed: &[bool],
    edge_spans: &[Option<(usize, usize)>],
) {
    let nranks = ranks.len();
    let pos_of =
        |rslots: &[Slot], want: &Slot| -> Option<usize> { rslots.iter().position(|s| s == want) };
    for _ in 0..4 {
        for r in 1..nranks {
            let (before, at) = ranks.split_at_mut(r);
            let prev = &before[r - 1];
            let row = &mut at[0];
            let mut keyed: Vec<(f64, usize, Slot)> = row
                .iter()
                .enumerate()
                .map(|(i, slot)| {
                    let neigh = slot_neighbors(g, slot, r, reversed, edge_spans, true);
                    let positions: Vec<usize> =
                        neigh.iter().filter_map(|s| pos_of(prev, s)).collect();
                    let bc = if positions.is_empty() {
                        i as f64
                    } else {
                        positions.iter().sum::<usize>() as f64 / positions.len() as f64
                    };
                    (bc, i, *slot)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));
            *row = keyed.into_iter().map(|(_, _, s)| s).collect();
        }
        for r in (0..nranks.saturating_sub(1)).rev() {
            let (at, after) = ranks.split_at_mut(r + 1);
            let next = &after[0];
            let row = &mut at[r];
            let mut keyed: Vec<(f64, usize, Slot)> = row
                .iter()
                .enumerate()
                .map(|(i, slot)| {
                    let neigh = slot_neighbors(g, slot, r, reversed, edge_spans, false);
                    let positions: Vec<usize> =
                        neigh.iter().filter_map(|s| pos_of(next, s)).collect();
                    let bc = if positions.is_empty() {
                        i as f64
                    } else {
                        positions.iter().sum::<usize>() as f64 / positions.len() as f64
                    };
                    (bc, i, *slot)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));
            *row = keyed.into_iter().map(|(_, _, s)| s).collect();
        }
    }
}

fn neighbor_centers(
    g: &Graph,
    slot: &Slot,
    r: usize,
    ranks: &[Vec<Slot>],
    slot_cross: &[Vec<usize>],
    boxes: &[BoxGeom],
    reversed: &[bool],
    edge_spans: &[Option<(usize, usize)>],
) -> Option<usize> {
    let center = |rr: usize, s: &Slot| -> Option<usize> {
        let si = ranks[rr].iter().position(|x| x == s)?;
        let c = slot_cross[rr][si];
        Some(match s {
            Slot::Real(i) => c + boxes[*i].clen / 2,
            Slot::Dummy(_) => c,
        })
    };
    let mut centers = Vec::new();
    if r > 0 {
        for nb in slot_neighbors(g, slot, r, reversed, edge_spans, true) {
            if let Some(c) = center(r - 1, &nb) {
                centers.push(c);
            }
        }
    }
    if r + 1 < ranks.len() {
        for nb in slot_neighbors(g, slot, r, reversed, edge_spans, false) {
            if let Some(c) = center(r + 1, &nb) {
                centers.push(c);
            }
        }
    }
    if centers.is_empty() {
        None
    } else {
        Some(centers.iter().sum::<usize>() / centers.len())
    }
}

#[allow(clippy::too_many_arguments)]
fn legalize(
    rslots: &[Slot],
    cross: &mut [usize],
    desired_centers: &[Option<usize>],
    boxes: &[BoxGeom],
    gap: usize,
    horizontal: bool,
    self_loops: &[usize],
    g: &Graph,
) {
    let clen = |slot: &Slot| -> usize {
        match slot {
            Slot::Real(i) => boxes[*i].clen,
            Slot::Dummy(_) => 1,
        }
    };
    let extra_after = |slot: &Slot| -> usize {
        if let Slot::Real(i) = slot {
            if horizontal && self_loops.iter().any(|&ei| g.edges[ei].from == *i) {
                return 2;
            }
        }
        0
    };
    // Forward pass: place at desired, pushed down by predecessor.
    let mut min_start = 0usize;
    for (i, slot) in rslots.iter().enumerate() {
        let len = clen(slot);
        let want = desired_centers[i]
            .map(|ctr| ctr.saturating_sub(len / 2))
            .unwrap_or(cross[i]);
        cross[i] = want.max(min_start);
        min_start = cross[i] + len + gap + extra_after(slot);
    }
}

/// Assign a port cross-row for each forward edge at its real endpoint.
/// Multiple edges on one side spread across the interior rows; overflow shares.
fn port_map(
    g: &Graph,
    boxes: &[BoxGeom],
    reversed: &[bool],
    edge_spans: &[Option<(usize, usize)>],
    pass_through: &[(usize, usize, usize)],
    outgoing: bool,
) -> Vec<usize> {
    let mut port = vec![0usize; g.edges.len()];
    for ni in 0..g.nodes.len() {
        let b = &boxes[ni];
        // Edges attaching to this node on this side, with the cross position of
        // their first stop on the other end (for stable, crossing-free ordering).
        let mut attached: Vec<(usize, usize)> = Vec::new();
        for (ei, e) in g.edges.iter().enumerate() {
            if e.from == e.to || reversed[ei] {
                continue;
            }
            let Some((rs, rt)) = edge_spans[ei] else {
                continue;
            };
            let is_here = if outgoing { e.from == ni } else { e.to == ni };
            if !is_here {
                continue;
            }
            let other_c = if outgoing {
                if rt - rs == 1 {
                    boxes[e.to].c + boxes[e.to].clen / 2
                } else {
                    pass_through
                        .iter()
                        .find(|&&(pe, pr, _)| pe == ei && pr == rs + 1)
                        .map(|&(_, _, c)| c)
                        .unwrap_or(0)
                }
            } else if rt - rs == 1 {
                boxes[e.from].c + boxes[e.from].clen / 2
            } else {
                pass_through
                    .iter()
                    .find(|&&(pe, pr, _)| pe == ei && pr == rt - 1)
                    .map(|&(_, _, c)| c)
                    .unwrap_or(0)
            };
            attached.push((ei, other_c));
        }
        attached.sort_by_key(|&(ei, c)| (c, ei));
        let k = attached.len();
        if k == 0 {
            continue;
        }
        let interior = b.clen.saturating_sub(2).max(1);
        for (idx, &(ei, _)) in attached.iter().enumerate() {
            let offset = if k == 1 {
                // Match box center (`c + clen/2`) so mono-chains stay straight
                // on even widths too (Phase 0.4).
                b.clen / 2
            } else {
                1 + (idx * (interior - 1)) / (k - 1).max(1)
            };
            port[ei] = b.c + offset.min(b.clen - 2).max(1);
        }
    }
    port
}
