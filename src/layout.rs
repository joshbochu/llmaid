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
/// Padding between member boxes and subgraph frame (each side).
pub const CLUSTER_PAD: usize = 2;
/// Interior rows under the top border reserved for title (+ optional blank).
pub const CLUSTER_TITLE_BAND: usize = 2;

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
        // Jog first, then reserve the remainder of the channel for labels on
        // the outgoing branch. This makes a label visibly belong to the path
        // it describes instead of floating on the other side of the bend.
        self.start + 1 + 2 * t
    }
}

#[derive(Debug)]
pub struct Placed {
    pub horizontal: bool, // LR/RL (flow = screen x)
    pub flipped: bool,    // RL/BT (flow axis mirrored on screen)
    /// Prefer the right of two equally near text positions when normalized
    /// vertical-chain label parity cannot land on the half-cell box center.
    pub bias_odd_box_labels_right: bool,
    pub boxes: Vec<BoxGeom>,
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
    if scene_width(g, &normal) <= max_width {
        return normal;
    }
    let compact = layout_fit(g, FIT_COMPACT);
    if scene_width(g, &compact) <= max_width {
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

fn scene_width(g: &Graph, placed: &Placed) -> usize {
    crate::route::route(g, placed).bounds().w.max(0) as usize
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
            if let Some((s, t)) = ranked_edge(ei)
                && s == v
            {
                rank[t] = rank[t].max(rank[v] + 1);
                indeg[t] -= 1;
                if indeg[t] == 0 {
                    queue.push(t);
                }
            }
        }
    }

    separate_group_boundary_forks(g, &mut rank, &reversed);
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

    // A simple vertical chain reads as one column, so its standard boxes use
    // one width: the largest label determines the column and every arrow can
    // stay on the same integer attachment line.
    if !horizontal && is_simple_chain(g, &reversed) {
        let mut column_width = boxes.iter().map(|b| b.clen).max().unwrap_or(0);
        if flipped
            && let Some(terminal) = (0..g.nodes.len()).find(|&node| {
                !g.edges.iter().enumerate().any(|(edge_index, edge)| {
                    edge.from != edge.to && !reversed[edge_index] && edge.from == node
                })
            })
        {
            let terminal_width = boxes[terminal]
                .lines
                .iter()
                .map(|line| line.width())
                .max()
                .unwrap_or(1);
            if column_width % 2 != terminal_width % 2 {
                column_width += 1;
            }
        }
        for b in &mut boxes {
            b.clen = column_width;
        }
    }

    // B12: genuinely parallel edges need distinct ports. Horizontal symmetric
    // diamonds share a centered trunk. Vertical distinct-peer junctions defer
    // spatial widening until coordinates are known below.
    for (ni, b) in boxes.iter_mut().enumerate() {
        let out_degree = g
            .edges
            .iter()
            .enumerate()
            .filter(|(ei, edge)| edge.from != edge.to && !reversed[*ei] && edge.from == ni)
            .count();
        let in_degree = g
            .edges
            .iter()
            .enumerate()
            .filter(|(ei, edge)| edge.from != edge.to && !reversed[*ei] && edge.to == ni)
            .count();
        let out_parallel = (0..n)
            .map(|target| {
                g.edges
                    .iter()
                    .enumerate()
                    .filter(|(ei, e)| {
                        e.from != e.to && !reversed[*ei] && e.from == ni && e.to == target
                    })
                    .count()
            })
            .max()
            .unwrap_or(0);
        let in_parallel = (0..n)
            .map(|source| {
                g.edges
                    .iter()
                    .enumerate()
                    .filter(|(ei, e)| {
                        e.from != e.to && !reversed[*ei] && e.to == ni && e.from == source
                    })
                    .count()
            })
            .max()
            .unwrap_or(0);
        let out_shared = out_degree > 1
            && out_parallel == 1
            && (!horizontal || is_symmetric_junction(g, &reversed, ni, true));
        let in_shared = in_degree > 1
            && in_parallel == 1
            && (!horizontal || is_symmetric_junction(g, &reversed, ni, false));
        let out_need = if out_shared { 1 } else { out_degree };
        let in_need = if in_shared { 1 } else { in_degree };
        let need = out_need.max(in_need);
        if need > 0 {
            b.clen = b.clen.max(need + 2);
        }
    }

    // --- Build rank slot lists with dummies for long edges.
    let mut ranks: Vec<Vec<Slot>> = vec![Vec::new(); nranks];
    for (i, b) in boxes.iter().enumerate() {
        ranks[b.rank].push(Slot::Real(i));
    }
    // (edge, rank) -> position recorded later; collect dummies in edge order.
    let mut edge_spans: Vec<Option<(usize, usize)>> = vec![None; g.edges.len()];
    for (ei, span) in edge_spans.iter_mut().enumerate() {
        if let Some((s, t)) = ranked_edge(ei) {
            *span = Some((rank[s], rank[t]));
            for row in ranks.iter_mut().take(rank[t]).skip(rank[s] + 1) {
                row.push(Slot::Dummy(ei));
            }
        }
    }

    // --- Crossing reduction: barycenter sweeps.
    order_by_barycenter(g, &mut ranks, &reversed, &edge_spans);

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
    let edge_label_pad = if fit.compact { 1 } else { EDGE_LABEL_PAD };
    // Vertical: a bit more air between ranks (TB chains inside groups).
    let channel_min = if fit.compact { 3 } else { 5 };
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
        for (index, slot) in rslots.iter().enumerate() {
            row.push(cur);
            let mut adv =
                slot_clen(slot, &boxes) + slot_gap(g, slot, rslots.get(index + 1), base_gap);
            if let Slot::Real(i) = slot
                && horizontal
                && has_self_loop(*i)
            {
                adv += 2; // room for the loop return below the box
            }
            cur += adv;
        }
        slot_cross.push(row);
    }

    // Alignment sweeps: pull slots toward the barycenter of their neighbors.
    // End on a forward sweep so downstream merges observe the final positions
    // of their parents inside the same coordinated legalization process.
    for sweep in 0..5 {
        let range: Vec<usize> = if sweep % 2 == 0 {
            (0..nranks).collect()
        } else {
            (0..nranks).rev().collect()
        };
        for &r in &range {
            let desired: Vec<Option<usize>> = {
                let context = NeighborContext {
                    graph: g,
                    ranks: &ranks,
                    slot_cross: &slot_cross,
                    boxes: &boxes,
                    reversed: &reversed,
                    edge_spans: &edge_spans,
                };
                ranks[r]
                    .iter()
                    .map(|slot| neighbor_centers(&context, slot, r))
                    .collect()
            };
            legalize(
                &ranks[r],
                &mut slot_cross[r],
                &desired,
                &boxes,
                base_gap,
                horizontal,
                &self_loops,
                &reversed,
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

    // A lone junction box may widen across adjacent attachment columns. In
    // horizontal flow this is limited to acyclic, non-reconverging forks so
    // approved diamond trunks and feedback routing retain their topology.
    if !horizontal {
        straighten_vertical_dummy_shafts(
            g,
            &ranks,
            &mut slot_cross,
            &boxes,
            &reversed,
            &edge_spans,
        );
        widen_cross_junction_boxes(
            g,
            &ranks,
            &slot_cross,
            &mut boxes,
            &reversed,
            &edge_spans,
            false,
        );
    } else {
        widen_cross_junction_boxes(
            g,
            &ranks,
            &slot_cross,
            &mut boxes,
            &reversed,
            &edge_spans,
            true,
        );
    }

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
    let mut out_port = port_map(
        g,
        &boxes,
        &reversed,
        &edge_spans,
        &pass_through,
        true,
        horizontal,
    );
    let mut in_port = port_map(
        g,
        &boxes,
        &reversed,
        &edge_spans,
        &pass_through,
        false,
        horizontal,
    );
    align_mono_ports(g, &boxes, &reversed, &mut out_port, &mut in_port);

    for ei in 0..g.edges.len() {
        let Some((rs, rt)) = edge_spans[ei] else {
            continue;
        };
        if reversed[ei] {
            continue; // routed as a perimeter back edge
        }
        for (r, edges_in_channel) in channel_edges.iter_mut().enumerate().take(rt).skip(rs) {
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
            edges_in_channel.push(ei);
        }
    }

    // Assign the lowest reusable track to each bend. Disjoint cross-axis
    // intervals share a track; overlapping intervals remain separated.
    let mut channel_track_count = vec![0usize; nranks.saturating_sub(1)];
    for r in 0..channel_track_count.len() {
        let mut benders: Vec<usize> = channel_edges[r]
            .iter()
            .copied()
            .filter(|&ei| {
                let seg = segs[ei].iter().find(|seg| seg.channel == r).unwrap();
                seg.from.1 != seg.to.1
            })
            .collect();
        benders.sort_by_key(|&ei| {
            let seg = segs[ei].iter().find(|seg| seg.channel == r).unwrap();
            (seg.from.1.min(seg.to.1), seg.from.1.max(seg.to.1), ei)
        });
        let assigned = allocate_junction_tracks(g, r, &benders, &segs, &edge_spans);
        for (&ei, track) in benders.iter().zip(assigned) {
            segs[ei]
                .iter_mut()
                .find(|seg| seg.channel == r)
                .unwrap()
                .track = Some(track);
            channel_track_count[r] = channel_track_count[r].max(track + 1);
        }
    }

    // --- Channel widths: label zone + reusable jog tracks.
    let mut channels = Vec::new();
    for r in 0..nranks.saturating_sub(1) {
        let mut label_zone = 0usize;
        let mut vertical_labels = 0usize;
        for &ei in &channel_edges[r] {
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
                    // Vertical flow: each label receives its own row beside the
                    // shaft. Rows are separated by one blank routing row.
                    vertical_labels += 1;
                }
            }
        }
        if vertical_labels > 0 {
            label_zone = label_zone.max(2 * vertical_labels - 1);
        }
        let slack = if fit.compact {
            2
        } else if horizontal {
            3
        } else {
            2
        };
        let width = channel_min.max(label_zone + 2 * channel_track_count[r] + slack);
        channels.push(Channel {
            start: 0,
            width,
            label_zone,
        });
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

    for (node, b) in boxes.iter_mut().enumerate() {
        let (start, width) = rank_span[b.rank];
        // Siblings inside a symmetric horizontal diamond share a clean leading
        // edge. Other approved layouts retain their established rank centering.
        b.f = if horizontal && is_diamond_branch(g, &reversed, node) {
            start
        } else {
            start + (width - b.flen) / 2
        };
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

    Placed {
        horizontal,
        flipped,
        bias_odd_box_labels_right: !horizontal && is_simple_chain(g, &reversed),
        boxes,
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

fn is_simple_chain(g: &Graph, reversed: &[bool]) -> bool {
    if !g.subgraphs.is_empty() || g.nodes.is_empty() {
        return false;
    }
    let mut out_degree = vec![0usize; g.nodes.len()];
    let mut in_degree = vec![0usize; g.nodes.len()];
    let mut forward_edges = 0usize;
    for (edge_index, edge) in g.edges.iter().enumerate() {
        if edge.from == edge.to || reversed[edge_index] {
            return false;
        }
        out_degree[edge.from] += 1;
        in_degree[edge.to] += 1;
        forward_edges += 1;
    }
    forward_edges + 1 == g.nodes.len()
        && out_degree.iter().all(|&degree| degree <= 1)
        && in_degree.iter().all(|&degree| degree <= 1)
}

fn is_symmetric_junction(g: &Graph, reversed: &[bool], node: usize, outgoing: bool) -> bool {
    let mut peers = Vec::new();
    for (edge_index, edge) in g.edges.iter().enumerate() {
        if edge.from == edge.to || reversed[edge_index] {
            continue;
        }
        let peer = if outgoing && edge.from == node {
            Some(edge.to)
        } else if !outgoing && edge.to == node {
            Some(edge.from)
        } else {
            None
        };
        if let Some(peer) = peer
            && !peers.contains(&peer)
        {
            peers.push(peer);
        }
    }
    if peers.len() < 2 {
        return false;
    }

    (0..g.nodes.len()).any(|common| {
        peers.iter().all(|&peer| {
            g.edges.iter().enumerate().any(|(edge_index, edge)| {
                edge.from != edge.to
                    && !reversed[edge_index]
                    && if outgoing {
                        edge.from == peer && edge.to == common
                    } else {
                        edge.from == common && edge.to == peer
                    }
            })
        })
    })
}

fn is_diamond_branch(g: &Graph, reversed: &[bool], node: usize) -> bool {
    (0..g.nodes.len()).any(|fork| {
        is_symmetric_junction(g, reversed, fork, true)
            && g.edges.iter().enumerate().any(|(edge_index, edge)| {
                edge.from != edge.to
                    && !reversed[edge_index]
                    && edge.from == fork
                    && edge.to == node
            })
    })
}

/// Align an edge when it is each endpoint's only attachment. Coordinate
/// assignment cannot always share an integer center between even/odd boxes,
/// so clamp one common port into both interiors.
fn align_mono_ports(
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
        if e.from == e.to || reversed[ei] || out_deg[e.from] != 1 || in_deg[e.to] != 1 {
            continue;
        }
        let clamp = |node: usize, port: usize| {
            let b = &boxes[node];
            let lo = b.c + 1;
            let hi = b.c + b.clen.saturating_sub(2);
            if lo > hi {
                b.c + b.clen / 2
            } else {
                port.clamp(lo, hi)
            }
        };
        let middle = (out_port[ei] + in_port[ei]) / 2;
        out_port[ei] = clamp(e.from, middle);
        in_port[ei] = clamp(e.to, out_port[ei]);
        if out_port[ei] != in_port[ei] {
            out_port[ei] = clamp(e.from, in_port[ei]);
        }
    }
}

/// When consecutive ranks each hold a single real node connected by a forward
/// **mono** edge (sole out of source, sole in of target), share a centerline
/// so the edge can run straight. Skip merges/forks — pulling them onto a
/// one-parent centerline undoes merge barycenters and can route other edges
/// through boxes.
fn straighten_mono_chains(
    g: &Graph,
    ranks: &[Vec<Slot>],
    boxes: &mut [BoxGeom],
    reversed: &[bool],
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
        // Sole forward edge between consecutive single-node ranks.
        if out_deg[ai] != 1 || in_deg[bi] != 1 {
            continue;
        }
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
        if in_deg[ai] > 1 {
            // Source is a merge: keep its barycenter; snap the child only.
            boxes[bi].c = ca.saturating_sub(boxes[bi].clen / 2);
        } else if out_deg[bi] > 1 {
            // Target is a fork: keep its position; snap the parent only.
            boxes[ai].c = cb.saturating_sub(boxes[ai].clen / 2);
        } else {
            // Pure mono chain: share a mid centerline.
            let mid = (ca + cb) / 2;
            boxes[ai].c = mid.saturating_sub(boxes[ai].clen / 2);
            boxes[bi].c = mid.saturating_sub(boxes[bi].clen / 2);
        }
    }
}

fn straighten_vertical_dummy_shafts(
    g: &Graph,
    ranks: &[Vec<Slot>],
    slot_cross: &mut [Vec<usize>],
    boxes: &[BoxGeom],
    reversed: &[bool],
    edge_spans: &[Option<(usize, usize)>],
) {
    for (edge_index, edge) in g.edges.iter().enumerate() {
        if edge.from == edge.to || reversed[edge_index] {
            continue;
        }
        let Some((start, end)) = edge_spans[edge_index] else {
            continue;
        };
        if end <= start + 1 {
            continue;
        }

        let source = boxes[edge.from].c + boxes[edge.from].clen / 2;
        let target = boxes[edge.to].c + boxes[edge.to].clen / 2;
        let clear = |cross: usize| {
            (start + 1..end).all(|rank| {
                ranks[rank].iter().all(|slot| match slot {
                    Slot::Real(node) => {
                        cross < boxes[*node].c || cross >= boxes[*node].c + boxes[*node].clen
                    }
                    Slot::Dummy(_) => true,
                })
            })
        };
        let Some(cross) = [source, target].into_iter().find(|&cross| clear(cross)) else {
            continue;
        };

        for rank in start + 1..end {
            let slot = ranks[rank]
                .iter()
                .position(|candidate| *candidate == Slot::Dummy(edge_index))
                .expect("dummy exists for spanned rank");
            slot_cross[rank][slot] = cross;
        }
    }
}

fn widen_cross_junction_boxes(
    g: &Graph,
    ranks: &[Vec<Slot>],
    slot_cross: &[Vec<usize>],
    boxes: &mut [BoxGeom],
    reversed: &[bool],
    edge_spans: &[Option<(usize, usize)>],
    horizontal_forks_only: bool,
) {
    let centers: Vec<usize> = boxes.iter().map(|b| b.c + b.clen / 2).collect();
    let dummy_cross = |edge: usize, rank: usize| {
        let slot = ranks[rank]
            .iter()
            .position(|candidate| *candidate == Slot::Dummy(edge))
            .expect("dummy exists for spanned rank");
        slot_cross[rank][slot]
    };
    let mut updates = vec![None; boxes.len()];

    for node in 0..boxes.len() {
        let rank = boxes[node].rank;
        let real_count = ranks[rank]
            .iter()
            .filter(|slot| matches!(slot, Slot::Real(_)))
            .count();
        if real_count != 1 {
            continue;
        }
        let has_feedback = g.edges.iter().enumerate().any(|(edge_index, edge)| {
            reversed[edge_index] && (edge.from == node || edge.to == node)
        });
        if horizontal_forks_only && has_feedback {
            continue;
        }

        let mut attachments = Vec::new();
        for outgoing in [true, false] {
            if horizontal_forks_only && !outgoing {
                continue;
            }
            if horizontal_forks_only && is_symmetric_junction(g, reversed, node, outgoing) {
                continue;
            }
            let mut side = Vec::new();
            let mut peers = Vec::new();
            for (edge_index, edge) in g.edges.iter().enumerate() {
                if edge.from == edge.to || reversed[edge_index] {
                    continue;
                }
                let is_here = if outgoing {
                    edge.from == node
                } else {
                    edge.to == node
                };
                if !is_here {
                    continue;
                }
                let peer = if outgoing { edge.to } else { edge.from };
                peers.push(peer);
                let (start, end) = edge_spans[edge_index].unwrap();
                let cross = if outgoing {
                    if end - start == 1 {
                        centers[edge.to]
                    } else {
                        dummy_cross(edge_index, start + 1)
                    }
                } else if end - start == 1 {
                    centers[edge.from]
                } else {
                    dummy_cross(edge_index, end - 1)
                };
                side.push(cross);
            }
            let all_distinct = peers
                .iter()
                .enumerate()
                .all(|(index, peer)| !peers[..index].contains(peer));
            if side.len() > 1 && all_distinct {
                attachments.extend(side);
            }
        }

        if attachments.is_empty() {
            continue;
        }
        let minimum = attachments.iter().copied().min().unwrap();
        let maximum = attachments.iter().copied().max().unwrap();
        let incoming = g
            .edges
            .iter()
            .enumerate()
            .filter(|(edge_index, edge)| {
                edge.from != edge.to && !reversed[*edge_index] && edge.to == node
            })
            .count();
        let mut outgoing_peers = Vec::new();
        for (edge_index, edge) in g.edges.iter().enumerate() {
            if edge.from != edge.to
                && !reversed[edge_index]
                && edge.from == node
                && !outgoing_peers.contains(&edge.to)
            {
                outgoing_peers.push(edge.to);
            }
        }
        let margin = if !horizontal_forks_only
            && top_level_group(g, node).is_none()
            && incoming == 0
            && outgoing_peers.len() > 1
        {
            2
        } else {
            1
        };
        let box_ = &boxes[node];
        let left = box_.c.min(minimum.saturating_sub(margin));
        let right = (box_.c + box_.clen - 1).max(maximum + margin);
        updates[node] = Some((left, right - left + 1));
    }

    for (box_, update) in boxes.iter_mut().zip(updates) {
        if let Some((cross, length)) = update {
            box_.c = cross;
            box_.clen = length;
        }
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

fn separate_group_boundary_forks(g: &Graph, rank: &mut [usize], reversed: &[bool]) {
    loop {
        let mut changed = false;
        for source in 0..g.nodes.len() {
            let Some(group) = top_level_group(g, source) else {
                continue;
            };
            let mut internal = Vec::new();
            let mut external = Vec::new();
            for (edge_index, edge) in g.edges.iter().enumerate() {
                if edge.from == edge.to || reversed[edge_index] || edge.from != source {
                    continue;
                }
                if top_level_group(g, edge.to) == Some(group) {
                    internal.push(edge.to);
                } else {
                    external.push(edge.to);
                }
            }
            if internal.is_empty() || external.is_empty() {
                continue;
            }
            let minimum_external_rank = internal.iter().map(|&node| rank[node]).max().unwrap() + 1;
            for target in external {
                if rank[target] < minimum_external_rank {
                    rank[target] = minimum_external_rank;
                    changed = true;
                }
            }
        }

        for (edge_index, edge) in g.edges.iter().enumerate() {
            if edge.from == edge.to || reversed[edge_index] {
                continue;
            }
            let minimum = rank[edge.from] + 1;
            if rank[edge.to] < minimum {
                rank[edge.to] = minimum;
                changed = true;
            }
        }
        if !changed {
            break;
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
            let mut keyed: Vec<(usize, usize, usize, Slot)> = row
                .iter()
                .enumerate()
                .map(|(i, slot)| {
                    let neigh = slot_neighbors(g, slot, r, reversed, edge_spans, true);
                    let positions: Vec<usize> =
                        neigh.iter().filter_map(|s| pos_of(prev, s)).collect();
                    let (sum, count) = if positions.is_empty() {
                        (i, 1)
                    } else {
                        (positions.iter().sum(), positions.len())
                    };
                    (sum, count, i, *slot)
                })
                .collect();
            keyed.sort_by(compare_barycenters);
            *row = keyed.into_iter().map(|(_, _, _, slot)| slot).collect();
        }
        for r in (0..nranks.saturating_sub(1)).rev() {
            let (at, after) = ranks.split_at_mut(r + 1);
            let next = &after[0];
            let row = &mut at[r];
            let mut keyed: Vec<(usize, usize, usize, Slot)> = row
                .iter()
                .enumerate()
                .map(|(i, slot)| {
                    let neigh = slot_neighbors(g, slot, r, reversed, edge_spans, false);
                    let positions: Vec<usize> =
                        neigh.iter().filter_map(|s| pos_of(next, s)).collect();
                    let (sum, count) = if positions.is_empty() {
                        (i, 1)
                    } else {
                        (positions.iter().sum(), positions.len())
                    };
                    (sum, count, i, *slot)
                })
                .collect();
            keyed.sort_by(compare_barycenters);
            *row = keyed.into_iter().map(|(_, _, _, slot)| slot).collect();
        }
    }
}

fn compare_barycenters(
    a: &(usize, usize, usize, Slot),
    b: &(usize, usize, usize, Slot),
) -> std::cmp::Ordering {
    let left = a.0 as u128 * b.1 as u128;
    let right = b.0 as u128 * a.1 as u128;
    left.cmp(&right).then(a.2.cmp(&b.2))
}

struct NeighborContext<'a> {
    graph: &'a Graph,
    ranks: &'a [Vec<Slot>],
    slot_cross: &'a [Vec<usize>],
    boxes: &'a [BoxGeom],
    reversed: &'a [bool],
    edge_spans: &'a [Option<(usize, usize)>],
}

fn neighbor_centers(context: &NeighborContext<'_>, slot: &Slot, r: usize) -> Option<usize> {
    let center = |rr: usize, s: &Slot| -> Option<usize> {
        let si = context.ranks[rr].iter().position(|x| x == s)?;
        let c = context.slot_cross[rr][si];
        Some(match s {
            Slot::Real(i) => 2 * c + context.boxes[*i].clen - 1,
            Slot::Dummy(_) => 2 * c,
        })
    };
    let (toward_previous, toward_next) = match slot {
        Slot::Real(node) => {
            let mut incoming = Vec::new();
            let mut outgoing = Vec::new();
            let mut has_feedback = false;
            for (edge_index, edge) in context.graph.edges.iter().enumerate() {
                if context.reversed[edge_index] && (edge.from == *node || edge.to == *node) {
                    has_feedback = true;
                }
                if edge.from == edge.to || context.reversed[edge_index] {
                    continue;
                }
                if edge.to == *node && !incoming.contains(&edge.from) {
                    incoming.push(edge.from);
                }
                if edge.from == *node && !outgoing.contains(&edge.to) {
                    outgoing.push(edge.to);
                }
            }
            if has_feedback {
                // Cyclic junctions do not define a clean fork/merge axis; use
                // both directions and leave the back edge to perimeter routing.
                (true, true)
            } else if incoming.len() >= 2 {
                // A merge's structural anchor is its parent barycenter. Its
                // downstream child follows later via mono-chain alignment.
                (true, false)
            } else if outgoing.len() >= 2 {
                // A fork centers over its children; pulling it toward its own
                // parent would make equivalent branches visibly lopsided.
                (false, true)
            } else {
                (true, true)
            }
        }
        Slot::Dummy(_) => (true, true),
    };
    let mut centers = Vec::new();
    if toward_previous && r > 0 {
        for nb in preferred_neighbors(context, slot, r, true) {
            if let Some(c) = center(r - 1, &nb) {
                centers.push(c);
            }
        }
    }
    if toward_next && r + 1 < context.ranks.len() {
        for nb in preferred_neighbors(context, slot, r, false) {
            if let Some(c) = center(r + 1, &nb) {
                centers.push(c);
            }
        }
    }
    if centers.is_empty() {
        None
    } else {
        Some((centers.iter().sum::<usize>() + centers.len() / 2) / centers.len())
    }
}

fn preferred_neighbors(
    context: &NeighborContext<'_>,
    slot: &Slot,
    rank: usize,
    toward_previous: bool,
) -> Vec<Slot> {
    let neighbors = slot_neighbors(
        context.graph,
        slot,
        rank,
        context.reversed,
        context.edge_spans,
        toward_previous,
    );
    let Slot::Real(node) = slot else {
        return neighbors;
    };
    let Some(group) = top_level_group(context.graph, *node) else {
        return neighbors;
    };
    let internal: Vec<Slot> = neighbors
        .iter()
        .copied()
        .filter(|neighbor| {
            matches!(neighbor, Slot::Real(other) if top_level_group(context.graph, *other) == Some(group))
        })
        .collect();
    if internal.is_empty() {
        neighbors
    } else {
        internal
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
    reversed: &[bool],
    g: &Graph,
) {
    let clen = |slot: &Slot| -> usize {
        match slot {
            Slot::Real(i) => boxes[*i].clen,
            Slot::Dummy(_) => 1,
        }
    };
    let extra_after = |slot: &Slot| -> usize {
        if let Slot::Real(i) = slot
            && horizontal
            && self_loops.iter().any(|&ei| g.edges[ei].from == *i)
        {
            return 2;
        }
        0
    };
    // Forward pass: place at desired, pushed down by predecessor.
    let mut min_start = 0usize;
    for (i, slot) in rslots.iter().enumerate() {
        let len = clen(slot);
        let want = desired_centers[i]
            .map(|center2| {
                center2
                    .saturating_sub(len.saturating_sub(1))
                    .saturating_add(1)
                    / 2
            })
            .unwrap_or(cross[i]);
        cross[i] = want.max(min_start);
        min_start = cross[i] + len + slot_gap(g, slot, rslots.get(i + 1), gap) + extra_after(slot);
    }

    // A forward-only overlap push biases a crowded rank toward increasing
    // cross coordinates. Translate the complete legalized rank back by its
    // mean overshoot so siblings remain balanced around their desired centers.
    // The translation preserves every separation constraint established above.
    let mut overshoot = 0usize;
    let mut desired_count = 0usize;
    for (i, slot) in rslots.iter().enumerate() {
        if let Some(desired) = desired_centers[i] {
            let actual2 = 2 * cross[i] + clen(slot) - 1;
            overshoot += actual2.saturating_sub(desired);
            desired_count += 1;
        }
    }
    let feedback_rank = rslots.iter().any(|slot| {
        let Slot::Real(node) = slot else {
            return false;
        };
        g.edges.iter().enumerate().any(|(edge_index, edge)| {
            reversed[edge_index] && (edge.from == *node || edge.to == *node)
        })
    });
    if desired_count > 0 && !feedback_rank {
        let shift = ((overshoot + desired_count) / (2 * desired_count))
            .min(cross.iter().copied().min().unwrap_or(0));
        for position in cross {
            *position -= shift;
        }
    }
}

fn slot_gap(g: &Graph, left: &Slot, right: Option<&Slot>, base: usize) -> usize {
    let (Slot::Real(left), Some(Slot::Real(right))) = (left, right) else {
        return base;
    };
    let left_group = top_level_group(g, *left);
    let right_group = top_level_group(g, *right);
    if left_group != right_group && (left_group.is_some() || right_group.is_some()) {
        base + 2 * CLUSTER_PAD
    } else {
        base
    }
}

fn top_level_group(g: &Graph, node: usize) -> Option<usize> {
    let mut group = g
        .subgraphs
        .iter()
        .position(|subgraph| subgraph.members.contains(&node))?;
    while let Some(parent) = g.subgraphs[group].parent {
        group = parent;
    }
    Some(group)
}

/// Assign a port cross-row for each forward edge at its real endpoint.
/// Horizontal diamond branches share a centered junction port. Vertical
/// distinct-peer junctions align each port with its adjacent shaft after the
/// box has widened to cover those columns. Parallel edges retain distinct lanes.
fn port_map(
    g: &Graph,
    boxes: &[BoxGeom],
    reversed: &[bool],
    edge_spans: &[Option<(usize, usize)>],
    pass_through: &[(usize, usize, usize)],
    outgoing: bool,
    horizontal: bool,
) -> Vec<usize> {
    let mut port = vec![0usize; g.edges.len()];
    for ni in 0..g.nodes.len() {
        let b = &boxes[ni];
        // Edges attaching to this node on this side, with the cross position of
        // their first stop on the other end (for stable, crossing-free ordering).
        let mut attached: Vec<(usize, usize, usize)> = Vec::new();
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
            let peer = if outgoing { e.to } else { e.from };
            attached.push((ei, other_c, peer));
        }
        attached.sort_by_key(|&(ei, c, peer)| (c, peer, ei));
        if attached.is_empty() {
            continue;
        }
        let distinct_junction = attached.len() > 1
            && attached
                .iter()
                .all(|&(_, _, peer)| attached.iter().filter(|&&(_, _, p)| p == peer).count() == 1);
        let symmetric_junction = is_symmetric_junction(g, reversed, ni, outgoing);
        let has_feedback =
            g.edges.iter().enumerate().any(|(edge_index, edge)| {
                reversed[edge_index] && (edge.from == ni || edge.to == ni)
            });
        let aligned_junction = distinct_junction
            && (!horizontal || (outgoing && !symmetric_junction && !has_feedback));
        let centered_junction = distinct_junction && symmetric_junction;
        let interior = b.clen.saturating_sub(2).max(1);
        for (index, &(ei, other_c, _)) in attached.iter().enumerate() {
            if aligned_junction {
                port[ei] = other_c.clamp(b.c + 1, b.c + b.clen.saturating_sub(2));
                continue;
            }
            let offset = if centered_junction || attached.len() == 1 {
                b.clen / 2
            } else {
                1 + (index * (interior - 1)) / (attached.len() - 1)
            };
            port[ei] = b.c + offset.min(b.clen - 2).max(1);
        }
    }
    port
}

fn allocate_junction_tracks(
    g: &Graph,
    channel: usize,
    benders: &[usize],
    segs: &[Vec<Seg>],
    edge_spans: &[Option<(usize, usize)>],
) -> Vec<usize> {
    let mut component: Vec<usize> = (0..benders.len()).collect();
    let root = |parents: &mut [usize], mut index: usize| {
        while parents[index] != index {
            index = parents[index];
        }
        index
    };
    for left in 0..benders.len() {
        for right in left + 1..benders.len() {
            let left_edge = benders[left];
            let right_edge = benders[right];
            let left_seg = segs[left_edge]
                .iter()
                .find(|seg| seg.channel == channel)
                .unwrap();
            let right_seg = segs[right_edge]
                .iter()
                .find(|seg| seg.channel == channel)
                .unwrap();
            let (left_start, left_end) = edge_spans[left_edge].unwrap();
            let (right_start, right_end) = edge_spans[right_edge].unwrap();
            let shared_fork = channel == left_start
                && channel == right_start
                && g.edges[left_edge].from == g.edges[right_edge].from
                && left_seg.from.1 == right_seg.from.1;
            let shared_merge = channel + 1 == left_end
                && channel + 1 == right_end
                && g.edges[left_edge].to == g.edges[right_edge].to
                && left_seg.to.1 == right_seg.to.1;
            if shared_fork || shared_merge {
                let left_root = root(&mut component, left);
                let right_root = root(&mut component, right);
                component[right_root] = left_root;
            }
        }
    }

    for index in 0..component.len() {
        component[index] = root(&mut component, index);
    }
    let mut roots = Vec::new();
    for &candidate in &component {
        if !roots.contains(&candidate) {
            roots.push(candidate);
        }
    }
    let intervals: Vec<(usize, usize)> = roots
        .iter()
        .map(|&candidate| {
            benders
                .iter()
                .enumerate()
                .filter(|(index, _)| component[*index] == candidate)
                .fold((usize::MAX, 0usize), |(lo, hi), (_, &edge)| {
                    let seg = segs[edge]
                        .iter()
                        .find(|seg| seg.channel == channel)
                        .unwrap();
                    (
                        lo.min(seg.from.1.min(seg.to.1)),
                        hi.max(seg.from.1.max(seg.to.1)),
                    )
                })
        })
        .collect();
    let group_tracks = allocate_tracks(&intervals);
    component
        .iter()
        .map(|candidate| {
            let group = roots.iter().position(|root| root == candidate).unwrap();
            group_tracks[group]
        })
        .collect()
}

fn allocate_tracks(intervals: &[(usize, usize)]) -> Vec<usize> {
    let mut track_ends: Vec<usize> = Vec::new();
    let mut assigned = Vec::with_capacity(intervals.len());
    for &(a, b) in intervals {
        let start = a.min(b);
        let end = a.max(b);
        let track = track_ends
            .iter()
            .position(|&occupied_end| occupied_end.saturating_add(1) < start)
            .unwrap_or(track_ends.len());
        if track == track_ends.len() {
            track_ends.push(end);
        } else {
            track_ends[track] = end;
        }
        assigned.push(track);
    }
    assigned
}

#[cfg(test)]
mod tests {
    use super::allocate_tracks;

    #[test]
    fn disjoint_bend_intervals_reuse_the_lowest_track() {
        assert_eq!(
            allocate_tracks(&[(0, 3), (2, 6), (5, 7), (8, 10)]),
            vec![0, 1, 0, 1]
        );
    }
}
