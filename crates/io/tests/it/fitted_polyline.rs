//! A curve- or spline-fit 2D POLYLINE (PEDIT Fit or Spline) keeps its fit when a drawing is read
//! and saved: the fit flags, the curve type and every vertex with its flags, the spline frame
//! included, in ASCII and binary DXF at every version and through DWG. An edited polyline is saved
//! without the stale frame; a moved, rotated or mirrored one carries its frame along; hostile fit
//! data never panics. Synthetic geometry only.

use cadcraft_doc::{Drawing, EntityKind, LwPolyline};
use cadcraft_geom::{Mat3, Vec2};
use cadcraft_io::{SaveOptions, Version, write_with};

/// A vertex as written: x, y, VERTEX flags, curve-fit tangent (degrees).
type V = (f64, f64, i64, Option<f64>);

/// Spline fit as AutoCAD lays it out: a frame vertex, the generated spline vertices, then the rest
/// of the frame.
const SPLINE: &[V] = &[
    (0.0, 0.0, 16, None),
    (0.0, 0.0, 8, None),
    (1.25, 0.875, 8, None),
    (2.5, 1.125, 8, None),
    (3.75, 0.625, 8, None),
    (4.0, 0.0, 8, None),
    (2.0, 3.0, 16, None),
    (4.0, 0.0, 16, None),
];

/// Curve fit: the original vertices carry their tangents, the extra vertices flag 1.
const CURVE: &[V] =
    &[(0.0, 0.0, 2, Some(30.0)), (0.75, 0.625, 1, None), (2.0, 1.0, 2, Some(0.0)), (3.25, 0.625, 1, None), (4.0, 0.0, 2, Some(-30.0))];

/// An ASCII 2018 DXF with one 2D POLYLINE on layer FIT.
fn polyline_dxf(flags: &str, curve: &str, verts: &[(String, String, String, Option<String>)]) -> Vec<u8> {
    let mut s = String::new();
    let mut put = |c: i32, v: &str| s.push_str(&format!("{c:>3}\r\n{v}\r\n"));
    for (c, v) in [(0, "SECTION"), (2, "HEADER"), (9, "$ACADVER"), (1, "AC1032"), (0, "ENDSEC"), (0, "SECTION"), (2, "ENTITIES"), (0, "POLYLINE")] {
        put(c, v);
    }
    for (c, v) in [(5, "2A"), (8, "FIT"), (66, "1"), (10, "0"), (20, "0"), (30, "0"), (70, flags), (75, curve)] {
        put(c, v);
    }
    for (k, (x, y, f, a)) in verts.iter().enumerate() {
        put(0, "VERTEX");
        put(5, &format!("{:X}", 0x30 + k));
        for (c, v) in [(8, "FIT"), (10, x), (20, y), (30, "0"), (70, f)] {
            put(c, v);
        }
        if let Some(a) = a {
            put(50, a);
        }
    }
    for (c, v) in [(0, "SEQEND"), (5, "3F"), (8, "FIT"), (0, "ENDSEC"), (0, "EOF")] {
        put(c, v);
    }
    s.into_bytes()
}

fn dxf_of(flags: i64, curve: i64, verts: &[V]) -> Vec<u8> {
    let v: Vec<_> = verts.iter().map(|(x, y, f, a)| (x.to_string(), y.to_string(), f.to_string(), a.map(|a| a.to_string()))).collect();
    polyline_dxf(&flags.to_string(), &curve.to_string(), &v)
}

fn polyline(d: &Drawing) -> LwPolyline {
    d.model.iter().find_map(|e| if let EntityKind::LwPolyline(p) = &e.kind { Some(p.clone()) } else { None }).unwrap()
}

/// The first POLYLINE of a saved file: its group 70 and 75, and its vertices as written.
fn saved_polyline(bytes: &[u8]) -> Option<(i64, i64, Vec<V>)> {
    let tags = cadcraft_dxf::parse(bytes).unwrap();
    let start = tags.iter().position(|t| t.code == 0 && t.value.as_str() == "POLYLINE")?;
    let mut flags = 0;
    let mut curve = 0;
    let mut verts: Vec<V> = Vec::new();
    let mut in_vertex = false;
    for t in &tags[start + 1..] {
        match (t.code, in_vertex) {
            (0, _) if t.value.as_str() == "VERTEX" => {
                in_vertex = true;
                verts.push((0.0, 0.0, 0, None));
            }
            (0, _) => break,
            (70, false) => flags = t.value.as_i64(),
            (75, false) => curve = t.value.as_i64(),
            (10, true) => verts.last_mut()?.0 = t.value.as_f64(),
            (20, true) => verts.last_mut()?.1 = t.value.as_f64(),
            (70, true) => verts.last_mut()?.2 = t.value.as_i64(),
            (50, true) => verts.last_mut()?.3 = Some(t.value.as_f64()),
            _ => {}
        }
    }
    Some((flags, curve, verts))
}

fn same(a: &[V], b: &[V]) -> bool {
    let close = |x: f64, y: f64| (x - y).abs() <= 1e-9;
    a.len() == b.len()
        && a.iter().zip(b).all(|(p, q)| {
            close(p.0, q.0) && close(p.1, q.1) && p.2 == q.2 && p.3.is_some() == q.3.is_some() && p.3.zip(q.3).is_none_or(|(x, y)| close(x, y))
        })
}

const VERSIONS: [Version; 6] = [Version::R2000, Version::R2004, Version::R2007, Version::R2010, Version::R2013, Version::R2018];

#[test]
fn spline_fit_polyline_keeps_its_frame_at_every_version() {
    let d = cadcraft_io::read_dxf(&dxf_of(4, 6, SPLINE)).unwrap();
    let p = polyline(&d);
    let drawn: Vec<Vec2> = SPLINE.iter().filter(|v| v.2 & 16 == 0).map(|v| Vec2::new(v.0, v.1)).collect();
    assert_eq!(p.vertices.iter().map(|v| v.p).collect::<Vec<_>>(), drawn, "the polyline draws the spline vertices");
    let fit = p.fit.as_ref().unwrap();
    assert_eq!((fit.flags, fit.curve_type, fit.sequence.len()), (4, 6, SPLINE.len()));
    assert!(p.fit_record().is_some());
    for v in VERSIONS {
        for binary in [false, true] {
            let bytes = write_with(&d, "x.dxf", &SaveOptions { version: Some(v), binary }).unwrap();
            let (flags, curve, verts) = saved_polyline(&bytes).unwrap();
            assert_eq!((flags & 6, curve), (4, 6), "{v:?} binary={binary}");
            assert!(same(&verts, SPLINE), "{v:?} binary={binary}: {verts:?}");
            let back = polyline(&cadcraft_io::read_dxf(&bytes).unwrap());
            assert_eq!(back, p, "{v:?} binary={binary}: read back the same");
        }
    }
}

#[test]
fn curve_fit_polyline_keeps_its_tangents() {
    let d = cadcraft_io::read_dxf(&dxf_of(2, 0, CURVE)).unwrap();
    let p = polyline(&d);
    assert_eq!(p.vertices.len(), CURVE.len(), "curve fit draws every vertex");
    let first = p.fit.as_ref().unwrap().sequence[0].tangent.unwrap();
    assert!((first - 30f64.to_radians()).abs() < 1e-12, "tangents are radians in the model");
    let bytes = write_with(&d, "x.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();
    let (flags, _, verts) = saved_polyline(&bytes).unwrap();
    assert_eq!(flags & 6, 2);
    assert!(same(&verts, CURVE), "{verts:?}");
}

#[test]
fn an_edited_fitted_polyline_is_saved_without_its_stale_frame() {
    let mut d = cadcraft_io::read_dxf(&dxf_of(4, 6, SPLINE)).unwrap();
    let h = d.model.iter().next().unwrap().handle;
    assert!(d.model.modify(h, |e| {
        if let EntityKind::LwPolyline(p) = &mut e.kind {
            p.vertices[1].p.x += 0.5;
        }
    }));
    assert!(polyline(&d).fit_record().is_none());
    let bytes = write_with(&d, "x.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();
    assert!(saved_polyline(&bytes).is_none(), "no POLYLINE: the edited polyline is a plain one");
    let back = polyline(&cadcraft_io::read_dxf(&bytes).unwrap());
    assert!(back.fit.is_none());
    assert_eq!(back.vertices, polyline(&d).vertices);
    let tags = cadcraft_dxf::parse(&bytes).unwrap();
    let start = tags.iter().position(|t| t.code == 0 && t.value.as_str() == "LWPOLYLINE").unwrap();
    let ys: Vec<f64> = tags[start + 1..].iter().take_while(|t| t.code != 0).filter(|t| t.code == 20).map(|t| t.value.as_f64()).collect();
    assert_eq!(ys.len(), 5, "the five drawn vertices");
    assert!(!ys.contains(&3.0), "the frame vertex (2, 3) is not written");
}

#[test]
fn a_moved_rotated_or_mirrored_fitted_polyline_carries_its_frame() {
    let d0 = cadcraft_io::read_dxf(&dxf_of(4, 6, SPLINE)).unwrap();
    let h = d0.model.iter().next().unwrap().handle;
    let moves =
        [Mat3::translate(Vec2::new(10.0, -5.0)), Mat3::rotate(std::f64::consts::FRAC_PI_2), Mat3::mirror(Vec2::new(0.0, 0.0), Vec2::new(0.0, 1.0))];
    for m in moves {
        let mut d = d0.clone();
        assert!(d.model.modify(h, |e| e.kind.transform(&m)));
        let p = polyline(&d);
        assert!(p.fit_record().is_some(), "{m:?}");
        let bytes = write_with(&d, "x.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();
        let (_, _, verts) = saved_polyline(&bytes).unwrap();
        let want: Vec<V> = SPLINE
            .iter()
            .map(|v| {
                let q = m.apply(Vec2::new(v.0, v.1));
                (q.x, q.y, v.2, None)
            })
            .collect();
        assert!(same(&verts, &want), "{m:?}: {verts:?}");
    }
}

#[test]
fn hostile_fit_data_never_panics() {
    let s = |v: &str| v.to_string();
    let cases: Vec<(&str, &str, Vec<(String, String, String, Option<String>)>)> = vec![
        ("4", "6", vec![]),
        ("4", "99999999999", vec![(s("nan"), s("inf"), s("16"), None), (s("1"), s("1"), s("-1"), Some(s("nan")))]),
        ("6", "-7", vec![(s("1e308"), s("-1e308"), s("999999999999"), Some(s("1e308"))), (s("0"), s("0"), s("8"), Some(s("inf")))]),
        ("-1", "5", vec![(s("0"), s("0"), s("16"), None)]),
        ("2", "x", vec![(s("a"), s("b"), s("c"), Some(s("d")))]),
    ];
    for (flags, curve, verts) in cases {
        let Ok(d) = cadcraft_io::read_dxf(&polyline_dxf(flags, curve, &verts)) else { continue };
        for v in [Version::R2000, Version::R2018] {
            for binary in [false, true] {
                let _ = write_with(&d, "x.dxf", &SaveOptions { version: Some(v), binary });
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_fitted_polyline_survives_dwg() {
    let d = cadcraft_io::read_dxf(&dxf_of(4, 6, SPLINE)).unwrap();
    let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
    let back = cadcraft_io::read(&dwg, "x.dwg").unwrap();
    let p = polyline(&back);
    assert_eq!(p.fit.as_ref().map(|f| (f.flags, f.curve_type, f.sequence.len())), Some((4, 6, SPLINE.len())));
    let bytes = write_with(&back, "x.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();
    let (_, _, verts) = saved_polyline(&bytes).unwrap();
    assert!(same(&verts, SPLINE), "{verts:?}");
}
