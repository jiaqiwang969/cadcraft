//! Explicit dimension origins must keep engineering targets at shared endpoints.
use cadcraft_engine::{
    Session,
    doc::{AssocSnap, Dimension, EntityKind, Handle},
};
use serde_json::{Value, json};

fn handle(v: &Value) -> Handle {
    Handle::parse_hex(v["handle"].as_str().unwrap()).unwrap()
}
fn line(s: &mut Session, a: [f64; 2], b: [f64; 2]) -> Handle {
    let v = s.execute("line", &json!({"points": [a, b]})).unwrap();
    Handle::parse_hex(v["handles"][0].as_str().unwrap()).unwrap()
}
fn dimension(s: &Session, h: Handle) -> Dimension {
    match &s.doc().unwrap().entity(h).unwrap().kind {
        EntityKind::Dimension(d) => d.clone(),
        _ => panic!("expected a dimension"),
    }
}

#[test]
fn explicit_origins_span_round_and_parent_without_coincident_neighbors() {
    let mut s = Session::new();
    let neighbor = line(&mut s, [15.0, 19.5], [15.0, -19.5]);
    let arc = handle(&s.execute("arc", &json!({"center": [15.5, 19.5], "radius": 0.5, "start": 90, "end": 180})).unwrap());
    let parent = line(&mut s, [31.0, 20.0], [45.0, 20.0]);
    line(&mut s, [45.0, 20.0], [45.0, 15.0]);
    let h = handle(
        &s.execute(
            "dimlinear",
            &json!({
                "origins": [{"handle": arc.hex(), "snap": "end"}, {"handle": parent.hex(), "snap": "end"}],
                "rotation": 0, "at": [30, 28]
            }),
        )
        .unwrap(),
    );
    let d = dimension(&s, h);
    assert_eq!(d.measurement, 30.0);
    assert_eq!(d.assoc.len(), 2);
    assert_eq!((d.assoc[0].handle, &d.assoc[0].snap), (arc, &AssocSnap::End));
    assert_eq!((d.assoc[1].handle, &d.assoc[1].snap), (parent, &AssocSnap::End));
    s.execute("move", &json!({"handles": [neighbor.hex()], "delta": [-4, 0]})).unwrap();
    assert_eq!(dimension(&s, h), d, "a coincident neighbor must not control the dimension");
    s.execute("move", &json!({"handles": [arc.hex()], "delta": [-1, 0]})).unwrap();
    assert_eq!(dimension(&s, h).measurement, 31.0);
    s.execute("properties.set", &json!({"handles": [parent.hex()], "end": [47, 20]})).unwrap();
    assert_eq!(dimension(&s, h).measurement, 33.0);
    s.undo().unwrap();
    assert_eq!(dimension(&s, h).measurement, 31.0);
}

#[test]
fn explicit_midpoints_control_diameter_despite_intersecting_edges() {
    let mut s = Session::new();
    line(&mut s, [28.0, -17.0], [28.0, 17.0]);
    let upper = line(&mut s, [25.0, 17.0], [31.0, 17.0]);
    let lower = line(&mut s, [25.0, -17.0], [31.0, -17.0]);
    let h = handle(
        &s.execute(
            "dimlinear",
            &json!({
                "origins": [{"handle": lower.hex(), "snap": "mid"}, {"handle": upper.hex(), "snap": "mid"}],
                "rotation": 90, "at": [22, 0], "text": "%%c<>"
            }),
        )
        .unwrap(),
    );
    assert_eq!(dimension(&s, h).measurement, 34.0);
    s.execute("move", &json!({"handles": [upper.hex()], "delta": [0, 1]})).unwrap();
    s.execute("move", &json!({"handles": [lower.hex()], "delta": [0, -1]})).unwrap();
    assert_eq!(dimension(&s, h).measurement, 36.0);
    assert!(dimension(&s, h).assoc.iter().all(|a| a.snap == AssocSnap::Mid));
}

#[test]
fn explicit_origins_reject_incomplete_or_ambiguous_requests_without_changes() {
    let mut s = Session::new();
    let l = line(&mut s, [0.0, 0.0], [10.0, 0.0]);
    let c = handle(&s.execute("circle", &json!({"center": [0, 0], "radius": 2})).unwrap());
    let a = json!({"handle": l.hex(), "snap": "start"});
    let b = json!({"handle": l.hex(), "snap": "end"});
    let before = cadcraft_io::write_dxf(s.doc().unwrap());
    for origins in [
        Value::Null,
        json!([]),
        json!([a]),
        json!([a, b, b]),
        json!([{"handle": "DEAD", "snap": "start"}, b]),
        json!([{"handle": c.hex(), "snap": "start"}, b]),
        json!([{"handle": l.hex(), "snap": "nearest"}, b]),
        json!([{"handle": l.hex()}, b]),
    ] {
        assert!(s.execute("dimlinear", &json!({"origins": origins, "at": [5, 5]})).is_err());
        assert_eq!(cadcraft_io::write_dxf(s.doc().unwrap()), before);
    }
    for key in ["object", "p1", "p2"] {
        let mut q = json!({"origins": [a, b], "at": [5, 5]});
        q[key] = json!([0, 0]);
        assert!(s.execute("dimlinear", &q).is_err());
        assert_eq!(cadcraft_io::write_dxf(s.doc().unwrap()), before);
    }
    for disabled in [0, 1] {
        s.execute("setvar", &json!({"name": "DIMASSOC", "value": disabled})).unwrap();
        let before = cadcraft_io::write_dxf(s.doc().unwrap());
        assert!(s.execute("dimlinear", &json!({"origins": [a, b], "at": [5, 5]})).is_err());
        assert_eq!(cadcraft_io::write_dxf(s.doc().unwrap()), before);
    }
}

#[test]
fn explicit_origins_survive_standard_dxf_without_private_metadata() {
    let mut s = Session::new();
    line(&mut s, [0.0, 2.0], [0.0, -2.0]);
    let first = line(&mut s, [0.0, 2.0], [4.0, 2.0]);
    let second = line(&mut s, [6.0, 2.0], [10.0, 2.0]);
    let h = handle(
        &s.execute(
            "dimlinear",
            &json!({
                "origins": [{"handle": first.hex(), "snap": "start"}, {"handle": second.hex(), "snap": "end"}],
                "rotation": 0, "at": [5, 8]
            }),
        )
        .unwrap(),
    );
    let expected = dimension(&s, h).assoc;
    let text = cadcraft_io::write_dxf(s.doc().unwrap());
    let lines: Vec<_> = text.lines().collect();
    let standard: String =
        lines.as_chunks::<2>().0.iter().filter(|p| p[0].trim().parse::<i32>().unwrap() < 1000).map(|p| format!("{}\n{}\n", p[0], p[1])).collect();
    let restored = cadcraft_io::read_dxf(standard.as_bytes()).unwrap();
    match &restored.entity(h).unwrap().kind {
        EntityKind::Dimension(d) => assert_eq!(d.assoc, expected),
        _ => panic!("dimension retained"),
    }
}
