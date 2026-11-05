//! Wide polylines are drawn with their linetype: each dash a filled piece of the band (issue #293).

use cadcraft_doc::{Common, Drawing, EntityKind, LwPolyline, Space, library};
use cadcraft_geom::{PolyVertex, Vec2};
use cadcraft_render::{Kind, Options, build, build_plot};

/// A 0.2-wide polyline through `xs` (on y = 0) with linetype `lt` (DASHED: 0.5 on, 0.25 off).
fn drawing(xs: &[f64], lt: &str, plinegen: bool) -> Drawing {
    let mut d = Drawing::new_imperial();
    d.linetypes.extend(library::standard_linetypes().into_iter().filter(|l| l.name == "DASHED"));
    let p = LwPolyline {
        fit: None,
        vertices: xs.iter().map(|&x| PolyVertex::new(Vec2::new(x, 0.0))).collect(),
        closed: false,
        const_width: 0.2,
        elevation: 0.0,
        plinegen,
    };
    d.add(&Space::Model, Common { linetype: lt.into(), ..Default::default() }, EntityKind::LwPolyline(p)).unwrap();
    d
}

/// Filled triangles of the drawing.
fn tris(d: &Drawing) -> Vec<Vec2> {
    let l = build(d, &Space::Model, &Options::default());
    l.prims.iter().filter(|p| p.kind == Kind::Tris).flat_map(|p| l.points(p).to_vec()).collect()
}

fn area(t: &[Vec2]) -> f64 {
    t.chunks(3).map(|t| ((t[1] - t[0]).cross(t[2] - t[0]) / 2.0).abs()).sum()
}

#[test]
fn wide_polyline_dashes_are_filled_bands() {
    let full = tris(&drawing(&[0.0, 3.0], "Continuous", false));
    assert!((area(&full) - 0.6).abs() < 1e-9);
    // 3.0 long = four 0.5 dashes with 0.25 gaps between.
    let dashed = tris(&drawing(&[0.0, 3.0], "DASHED", false));
    assert!((area(&dashed) - 0.4).abs() < 1e-9, "{}", area(&dashed));
    assert!(dashed.iter().all(|q| !(q.x > 0.5 + 1e-9 && q.x < 0.75 - 1e-9)), "the first gap is empty");
    assert!(dashed.iter().all(|q| (q.y.abs() - 0.1).abs() < 1e-9), "dashes keep the full width");
}

#[test]
fn plinegen_runs_the_pattern_along_the_whole_polyline() {
    // Two 0.6 segments: per segment each gets one 0.5 dash; along the whole 1.2 the pattern
    // gives 0.5 + 0.45.
    let per_segment = area(&tris(&drawing(&[0.0, 0.6, 1.2], "DASHED", false)));
    let whole = area(&tris(&drawing(&[0.0, 0.6, 1.2], "DASHED", true)));
    assert!((per_segment - 0.2).abs() < 1e-9, "{per_segment}");
    assert!((whole - 0.19).abs() < 1e-9, "{whole}");
}

fn variable_drawing(vertices: Vec<PolyVertex>, closed: bool, lt: &str, plinegen: bool) -> Drawing {
    let mut d = drawing(&[], lt, plinegen);
    let p = LwPolyline { fit: None, vertices, closed, const_width: 0.0, elevation: 0.0, plinegen };
    let common = Common {
        linetype: lt.into(),
        lineweight: cadcraft_doc::Lineweight::Mm100(50),
        transparency: cadcraft_doc::Transparency::Percent(50),
        ..Default::default()
    };
    d.add(&Space::Model, common, EntityKind::LwPolyline(p)).unwrap();
    d
}

fn vertex(x: f64, y: f64, start_width: f64, end_width: f64) -> PolyVertex {
    PolyVertex { p: Vec2::new(x, y), start_width, end_width, ..Default::default() }
}

#[test]
fn repeated_vertex_keeps_the_following_arrow_widths() {
    // A thin leader followed by a duplicate tip and a tapered arrow returning downwards.
    // Skipping the duplicate must not shift the width lookup to the previous, thin segment.
    for (lt, want) in [("Continuous", 1.125), ("DASHED", 0.6875)] {
        let d = variable_drawing(
            vec![vertex(0.0, 0.0, 0.0, 0.0), vertex(0.0, 20.0, 0.0, 0.0), vertex(0.0, 20.0, 0.0, 0.75), vertex(0.0, 17.0, 0.75, 0.75)],
            false,
            lt,
            false,
        );
        let got = area(&tris(&d));
        assert!((got - want).abs() < 1e-9, "{lt}: arrow area {got}, expected {want}");
    }
}

#[test]
fn mixed_width_polyline_plots_thin_segments_with_their_dash_phase() {
    for (lt, plinegen, want) in [("Continuous", false, 0.12), ("DASHED", false, 0.10), ("DASHED", true, 0.09)] {
        let d = variable_drawing(
            vec![vertex(0.0, 0.0, 0.0, 0.0), vertex(0.6, 0.0, 0.0, 0.0), vertex(0.6, 0.0, 0.2, 0.2), vertex(1.2, 0.0, 0.0, 0.0)],
            false,
            lt,
            plinegen,
        );
        let opts = Options { lineweights: true, transparency: true, min_dash: 0.0, ..Default::default() };
        for l in [build(&d, &Space::Model, &opts), build_plot(&d, &Space::Model, &opts)] {
            let thin: Vec<_> = l.prims.iter().filter(|p| p.kind == Kind::Polyline).collect();
            assert!(!thin.is_empty(), "{lt}, PLINEGEN {plinegen}: the zero-width leader must be stroked");
            assert!(thin.iter().all(|p| (p.lw - 0.5).abs() < 1e-6 && p.transparency == 50));
            let pts: Vec<_> = thin.iter().flat_map(|p| l.points(p)).collect();
            let end = if lt == "Continuous" { 0.6 } else { 0.5 };
            assert!(pts.iter().any(|p| p.x.abs() < 1e-9));
            assert!(pts.iter().any(|p| (p.x - end).abs() < 1e-9));
            assert!(pts.iter().all(|p| p.x <= end + 1e-9), "no stroke over the wide band or dash gap");
            let band: Vec<_> = l.prims.iter().filter(|p| p.kind == Kind::Tris).flat_map(|p| l.points(p).to_vec()).collect();
            assert!((area(&band) - want).abs() < 1e-9, "{lt}, PLINEGEN {plinegen}: {}", area(&band));
        }
    }
}

#[test]
fn repeated_vertex_preserves_widths_on_the_closing_segment() {
    let d = variable_drawing(
        vec![vertex(0.0, 0.0, 0.0, 0.0), vertex(1.0, 0.0, 0.0, 0.0), vertex(1.0, 0.0, 0.0, 0.5), vertex(1.0, 1.0, 0.2, 0.2)],
        true,
        "Continuous",
        false,
    );
    let got = area(&tris(&d));
    assert!((got - (0.25 + 2.0_f64.sqrt() * 0.2)).abs() < 1e-9, "area {got}");
}

#[test]
fn mixed_width_polyline_keeps_a_zero_width_arc_in_exports() {
    let mut a = vertex(0.0, 0.0, 0.0, 0.0);
    a.bulge = 1.0;
    let d = variable_drawing(vec![a, vertex(2.0, 0.0, 0.4, 0.4), vertex(3.0, 0.0, 0.0, 0.0)], false, "Continuous", false);
    let l = build_plot(&d, &Space::Model, &Options { tolerance: 0.001, ..Default::default() });
    let pts: Vec<_> = l.prims.iter().filter(|p| p.kind == Kind::Polyline).flat_map(|p| l.points(p)).collect();
    assert!(pts.iter().any(|p| p.y < -0.99), "the semicircular thin segment must remain curved and visible");
    assert!(pts.iter().all(|p| p.x <= 2.0 + 1e-9), "no centre stroke over the wide segment");
}

#[test]
fn dash_ending_at_a_width_change_uses_the_preceding_width() {
    let d = variable_drawing(vec![vertex(0.0, 0.0, 0.0, 0.0), vertex(0.5, 0.0, 0.2, 0.2), vertex(1.0, 0.0, 0.0, 0.0)], false, "DASHED", true);
    let l = build_plot(&d, &Space::Model, &Options::default());
    let band: Vec<_> = l.prims.iter().filter(|p| p.kind == Kind::Tris).flat_map(|p| l.points(p).to_vec()).collect();
    assert!((area(&band) - 0.05).abs() < 1e-9, "only the wide dash from 0.75 to 1.0 has area");
    let thin: Vec<_> = l.prims.iter().filter(|p| p.kind == Kind::Polyline).flat_map(|p| l.points(p)).collect();
    assert!(thin.iter().any(|p| (p.x - 0.5).abs() < 1e-9), "the first dash ends as a thin stroke");
}
