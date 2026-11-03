//! Whole, endpoint-connected LINE/ARC loops with analytic circular bulges.
//!
//! This finite path does not trim crossings or bridge gaps. The general picked-
//! region tracer remains responsible for those cases. A result retains every
//! original source edge, rather than substituting a tessellated boundary.
use std::collections::BTreeSet;

use cadcraft_doc::{Drawing, EntityKind, Handle};
use cadcraft_geom::{Arc, Line, PolyVertex, Polyline, Segment, Vec2, arc_to_bulge, intersect_segments};

const MAX_EDGES: usize = 512;
const JOIN: f64 = 1e-8;

fn finite(p: Vec2) -> bool {
    p.is_finite() && p.x.abs() <= 1e9 && p.y.abs() <= 1e9
}

fn source(d: &Drawing, h: Handle) -> Option<Segment> {
    let s = match &d.entity(h)?.kind {
        EntityKind::Line(l) if l.a.z.abs() <= JOIN && l.b.z.abs() <= JOIN => Segment::Line(Line::new(l.a.xy(), l.b.xy())),
        EntityKind::Arc(a)
            if a.center.z.abs() <= JOIN
                && finite(a.center.xy())
                && a.radius.is_finite()
                && (JOIN..=1e9).contains(&a.radius)
                && a.start.is_finite()
                && a.end.is_finite() =>
        {
            Segment::Arc { arc: Arc::new(a.center.xy(), a.radius, a.start, a.end), ccw: true }
        }
        _ => return None,
    };
    (finite(s.start()) && finite(s.end()) && s.len().is_finite() && s.len() > JOIN && !s.start().near(s.end(), JOIN)).then_some(s)
}

fn reverse(s: Segment) -> Segment {
    match s {
        Segment::Line(l) => Segment::Line(Line::new(l.b, l.a)),
        Segment::Arc { arc, ccw } => Segment::Arc { arc, ccw: !ccw },
    }
}

fn vertices(ss: &[Segment]) -> Vec<PolyVertex> {
    ss.iter()
        .map(|s| {
            let bulge = match s {
                Segment::Line(_) => 0.0,
                Segment::Arc { arc, ccw } => arc_to_bulge(arc.sweep() * if *ccw { 1.0 } else { -1.0 }),
            };
            PolyVertex::with_bulge(s.start(), bulge)
        })
        .collect()
}

fn endpoint(s: &Segment, p: Vec2) -> bool {
    p.near(s.start(), JOIN) || p.near(s.end(), JOIN)
}

/// Refuse self-intersections, nonadjacent contacts and overlapping spans.
fn simple(ss: &[Segment]) -> bool {
    for (i, a) in ss.iter().enumerate() {
        for (j, b) in ss.iter().enumerate().skip(i + 1) {
            let adjacent = j == i + 1 || (i == 0 && j + 1 == ss.len());
            if intersect_segments(a, b).iter().any(|p| !(adjacent && endpoint(a, *p) && endpoint(b, *p))) {
                return false;
            }
            // Coincident lines/circles have no discrete intersections. Their
            // endpoints and interior points still expose an overlapping span.
            for (x, y) in [(a, b), (b, a)] {
                if y.closest(x.mid()).dist(x.mid()) <= JOIN {
                    return false;
                }
                for p in [x.start(), x.end()] {
                    if y.closest(p).dist(p) <= JOIN && !(adjacent && endpoint(y, p)) {
                        return false;
                    }
                }
            }
        }
    }
    true
}

/// Trace one simple whole-edge loop in either source order or orientation.
/// Unsupported geometry, gaps, branches, disconnected loops and hostile input
/// return `None`; no drawing state is changed.
pub(super) fn trace(d: &Drawing, hs: &[Handle]) -> Option<Vec<PolyVertex>> {
    if !(2..=MAX_EDGES).contains(&hs.len()) || hs.iter().collect::<BTreeSet<_>>().len() != hs.len() {
        return None;
    }
    let ss: Vec<_> = hs.iter().map(|h| source(d, *h)).collect::<Option<_>>()?;
    let first = *ss.first()?;
    let mut ordered = vec![first];
    let mut used = BTreeSet::from([0usize]);
    let mut end = first.end();
    while used.len() < ss.len() {
        let mut next = None;
        for (i, s) in ss.iter().enumerate().filter(|(i, _)| !used.contains(i)) {
            let candidate = if end.near(s.start(), JOIN) {
                Some(*s)
            } else if end.near(s.end(), JOIN) {
                Some(reverse(*s))
            } else {
                None
            };
            if let Some(s) = candidate {
                if next.is_some() {
                    return None;
                }
                next = Some((i, s));
            }
        }
        let (i, s) = next?;
        used.insert(i);
        end = s.end();
        ordered.push(s);
    }
    if !end.near(first.start(), JOIN) || !simple(&ordered) {
        return None;
    }
    let mut vs = vertices(&ordered);
    let area = Polyline { vertices: vs.clone(), closed: true }.area();
    if !area.is_finite() || area.abs() <= JOIN * JOIN {
        return None;
    }
    if area < 0.0 {
        ordered = ordered.into_iter().rev().map(reverse).collect();
        vs = vertices(&ordered);
    }
    vs.iter().all(|v| finite(v.p) && v.bulge.is_finite()).then_some(vs)
}
