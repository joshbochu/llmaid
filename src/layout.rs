//! Sugiyama layered layout in integer character-grid coordinates.
//!
//! Works in "flow space": `f` is the rank axis, `c` the cross axis. For LR
//! diagrams flow is screen-x; for TB it is screen-y. `render.rs` maps flow
//! space to the screen per direction, so layout and routing are written once.
//!
//! Determinism: all iteration is over Vecs in declaration order; ties break
//! by declaration index. No HashMap iteration anywhere.

use crate::parse::{Dir, Graph};
use unicode_width::UnicodeWidthStr;

/// Padding inside a box between border and label, in flow-space cross terms
/// this is only horizontal on screen (label lines are padded when rendered).
pub const PAD: usize = 1;

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

#[derive(Debug)]
pub struct Placed {
    pub horizontal: bool, // LR/RL (flow = screen x)
    pub flipped: bool,    // RL/BT (flow axis mirrored on screen)
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

pub fn layout(g: &Graph) -> Placed {
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
        Some(if reversed[ei] { (e.to, e.from) } else { (e.from, e.to) })
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

    // --- Box sizes from labels.
    let mut boxes: Vec<BoxGeom> = g
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let lines: Vec<String> = node.label.split('\n').map(str::to_string).collect();
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
    let base_gap = if horizontal { 1 } else { 4.max(max_label + 3) };
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
                    neighbor_centers(g, slot, r, &ranks, &slot_cross, &boxes, &reversed, &edge_spans)
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
    let mut pass_through = Vec::new();
    for (r, rslots) in ranks.iter().enumerate() {
        for (si, slot) in rslots.iter().enumerate() {
            if let Slot::Dummy(ei) = slot {
                pass_through.push((*ei, r, slot_cross[r][si]));
            }
        }
    }

    let cross_extent = (0..nranks)
        .flat_map(|r| {
            ranks[r]
                .iter()
                .zip(&slot_cross[r])
                .map(|(slot, &c)| c + slot_clen(slot, &boxes))
        })
        .max()
        .unwrap_or(0);

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
    let out_port = port_map(g, &boxes, &reversed, &edge_spans, &pass_through, true);
    let in_port = port_map(g, &boxes, &reversed, &edge_spans, &pass_through, false);

    for ei in 0..g.edges.len() {
        let Some((rs, rt)) = edge_spans[ei] else { continue };
        if reversed[ei] {
            continue; // routed as a perimeter back edge
        }
        for r in rs..rt {
            let from_c = if r == rs { out_port[ei] } else { dummy_cross(ei, r) };
            let to_c = if r + 1 == rt { in_port[ei] } else { dummy_cross(ei, r + 1) };
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
            let labeled_here = g.edges[ei].label.is_some()
                && edge_spans[ei].map(|(rs, _)| rs) == Some(r);
            if labeled_here {
                label_zone = label_zone.max(g.edges[ei].label.as_deref().unwrap().width());
            }
            if seg.from.1 != seg.to.1 {
                tracks += 1;
            }
        }
        let width = 5usize.max(label_zone + 2 * tracks + 3);
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
            segs[ei]
                .iter_mut()
                .find(|s| s.channel == r)
                .unwrap()
                .track = Some(t);
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
        let Some((rs, rt)) = edge_spans[ei] else { continue };
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
                let Some((rs, rt)) = edge_spans[ei] else { continue };
                if toward_prev && e.to == *i && rt == r {
                    out.push(if rt - rs == 1 { Slot::Real(e.from) } else { Slot::Dummy(ei) });
                }
                if !toward_prev && e.from == *i && rs == r {
                    out.push(if rt - rs == 1 { Slot::Real(e.to) } else { Slot::Dummy(ei) });
                }
            }
        }
        Slot::Dummy(ei) => {
            let e = &g.edges[*ei];
            let (rs, rt) = edge_spans[*ei].unwrap();
            if toward_prev {
                out.push(if r - 1 == rs { Slot::Real(e.from) } else { Slot::Dummy(*ei) });
            } else {
                out.push(if r + 1 == rt { Slot::Real(e.to) } else { Slot::Dummy(*ei) });
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
    let pos_of = |rslots: &[Slot], want: &Slot| -> Option<usize> {
        rslots.iter().position(|s| s == want)
    };
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
            let Some((rs, rt)) = edge_spans[ei] else { continue };
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
                (b.clen - 1) / 2
            } else {
                1 + (idx * (interior - 1)) / (k - 1).max(1)
            };
            port[ei] = b.c + offset.min(b.clen - 2).max(1);
        }
    }
    port
}
