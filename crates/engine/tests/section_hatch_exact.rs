//! Exact analytic boundary sources needed by engineering section views.
use cadcraft_engine::Session;
use cadcraft_engine::doc::{EntityKind, Handle, Hatch};
use cadcraft_engine::geom::Polyline;
use serde_json::{Value, json};

fn handle(v: &Value) -> String {
    v["handle"].as_str().unwrap().to_string()
}

fn hatch(s: &Session, h: &str) -> Hatch {
    match &s.doc().unwrap().entity(Handle::parse_hex(h).unwrap()).unwrap().kind {
        EntityKind::Hatch(x) => x.clone(),
        _ => panic!("Expected hatch"),
    }
}

fn circle_edges(s: &mut Session) -> Vec<String> {
    [0.0, 180.0]
        .into_iter()
        .map(|start| handle(&s.execute("arc", &json!({"center":[0,0],"radius":10,"start":start,"end":start+180.0})).unwrap()))
        .collect()
}

#[test]
fn full_arc_boundaries_keep_analytic_area_and_original_handles() {
    let mut s = Session::new();
    let edges = circle_edges(&mut s);
    let h = handle(&s.execute("hatch", &json!({"handles":edges,"pattern":"ANSI31","associative":true})).unwrap());
    let h = hatch(&s, &h);
    assert!(h.associative);
    assert_eq!(h.loops.len(), 1);
    assert_eq!(h.loops[0].sources.len(), 2);
    assert_eq!(h.loops[0].vertices.len(), 2);
    let p = Polyline { vertices: h.loops[0].vertices.clone(), closed: true };
    assert!((p.area() - 100.0 * std::f64::consts::PI).abs() < 1e-9);
    let mut actual: Vec<_> = h.loops[0].sources.iter().map(|x| x.hex()).collect();
    let mut expected = edges;
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
}

fn line(s: &mut Session, a: [f64; 2], b: [f64; 2]) -> String {
    s.execute("line", &json!({"points":[a,b]})).unwrap()["handles"][0].as_str().unwrap().to_string()
}

fn capsule(s: &mut Session) -> Vec<String> {
    vec![
        line(s, [0.0, -2.0], [10.0, -2.0]),
        handle(&s.execute("arc", &json!({"center":[10,0],"radius":2,"start":270,"end":90})).unwrap()),
        // Deliberately reversed relative to the loop traversal.
        line(s, [0.0, 2.0], [10.0, 2.0]),
        handle(&s.execute("arc", &json!({"center":[0,0],"radius":2,"start":90,"end":270})).unwrap()),
    ]
}

fn exact_area(s: &Session, h: &str, vertices: usize, expected: f64) {
    let h = hatch(s, h);
    assert!(h.associative);
    assert_eq!(h.loops.len(), 1);
    assert_eq!(h.loops[0].vertices.len(), vertices);
    assert_eq!(h.loops[0].sources.len(), vertices);
    let area = Polyline { vertices: h.loops[0].vertices.clone(), closed: true }.area();
    assert!((area - expected).abs() < 1e-8, "actual {area}, expected {expected}");
}

#[test]
fn mixed_curves_follow_stretch_move_scale_and_undo_without_chords() {
    let mut s = Session::new();
    let mut edges = capsule(&mut s);
    edges.swap(0, 2);
    let h = handle(&s.execute("hatch", &json!({"handles":edges,"associative":true})).unwrap());
    let base = 40.0 + 4.0 * std::f64::consts::PI;
    exact_area(&s, &h, 4, base);
    s.execute("stretch", &json!({"window":[[8,-3],[13,3]],"delta":[2,0]})).unwrap();
    exact_area(&s, &h, 4, base + 8.0);
    s.execute("u", &json!({})).unwrap();
    exact_area(&s, &h, 4, base);
    s.execute("move", &json!({"handles":edges,"delta":[123,-47]})).unwrap();
    exact_area(&s, &h, 4, base);
    s.execute("scale", &json!({"handles":edges,"base":[123,-47],"factor":2})).unwrap();
    exact_area(&s, &h, 4, base * 4.0);
}

#[test]
fn analytic_sources_survive_dxf_and_dwg_then_follow_a_new_edit() {
    let mut s = Session::new();
    let edges = capsule(&mut s);
    let h = handle(&s.execute("hatch", &json!({"handles":edges,"associative":true})).unwrap());
    for name in ["section.dxf", "section.dwg"] {
        let bytes = cadcraft_io::write(s.doc().unwrap(), name).unwrap();
        let drawing = cadcraft_io::read(&bytes, name).unwrap();
        let mut next = Session::new();
        next.open_drawing(drawing, name, None);
        exact_area(&next, &h, 4, 40.0 + 4.0 * std::f64::consts::PI);
        next.execute("stretch", &json!({"window":[[8,-3],[13,3]],"delta":[2,0]})).unwrap();
        exact_area(&next, &h, 4, 48.0 + 4.0 * std::f64::consts::PI);
        let written = cadcraft_io::write(next.doc().unwrap(), name).unwrap();
        let mut again = Session::new();
        again.open_drawing(cadcraft_io::read(&written, name).unwrap(), name, None);
        exact_area(&again, &h, 4, 48.0 + 4.0 * std::f64::consts::PI);
    }
}

#[test]
fn missing_duplicate_open_branched_crossing_and_oversized_sets_reject_without_mutation() {
    let mut s = Session::new();
    let edges = capsule(&mut s);
    let before = s.doc().unwrap().model.len();
    for bad in [vec![edges[0].clone(), "FFFFF".into()], vec![edges[0].clone(), edges[0].clone()], edges[..3].to_vec(), vec![edges[0].clone(); 513]] {
        assert!(s.execute("hatch", &json!({"handles":bad})).is_err());
        assert_eq!(s.doc().unwrap().model.len(), before);
    }
    let branch = line(&mut s, [10.0, -2.0], [20.0, -2.0]);
    let mut branched = edges;
    branched.push(branch);
    assert!(s.execute("hatch", &json!({"handles":branched})).is_err());
    let mut bow = Session::new();
    let pts = [[0.0, 0.0], [10.0, 10.0], [0.0, 10.0], [10.0, 0.0], [0.0, 0.0]];
    let cross: Vec<_> = pts.windows(2).map(|p| line(&mut bow, p[0], p[1])).collect();
    assert!(bow.execute("hatch", &json!({"handles":cross})).is_err());
    assert_eq!(bow.doc().unwrap().model.len(), 4);
}

#[test]
fn whole_edge_creation_does_not_invent_a_gap_closure() {
    let mut s = Session::new();
    let edges = capsule(&mut s);
    s.execute("move", &json!({"handles":[edges[2]],"delta":[0,0.001]})).unwrap();
    s.execute("setvar", &json!({"name":"HPGAPTOL","value":10})).unwrap();
    assert!(s.execute("hatch", &json!({"handles":edges})).is_err());
    assert_eq!(s.doc().unwrap().model.len(), 4);
}

#[test]
fn open_boundary_and_detached_hatch_do_not_claim_association() {
    let mut s = Session::new();
    let edges = capsule(&mut s);
    let h = handle(&s.execute("hatch", &json!({"handles":edges,"associative":true})).unwrap());
    s.execute("setvar", &json!({"name":"HPGAPTOL","value":0})).unwrap();
    s.execute("move", &json!({"handles":[edges[2]],"delta":[0,5]})).unwrap();
    let broken = hatch(&s, &h);
    assert!(!broken.associative);
    assert!(broken.loops.iter().all(|l| l.sources.is_empty()));
    s.execute("u", &json!({})).unwrap();
    exact_area(&s, &h, 4, 40.0 + 4.0 * std::f64::consts::PI);
    s.execute("move", &json!({"handles":[h],"delta":[0,7]})).unwrap();
    assert!(!hatch(&s, &h).associative);
}

#[test]
fn disconnected_loops_and_coincident_curves_are_not_one_material_region() {
    let mut s = Session::new();
    let mut edges = circle_edges(&mut s);
    let mut other = circle_edges(&mut s);
    s.execute("move", &json!({"handles":other,"delta":[30,0]})).unwrap();
    edges.append(&mut other);
    assert!(s.execute("hatch", &json!({"handles":edges})).is_err());
    let mut duplicate = Session::new();
    let mut edges = circle_edges(&mut duplicate);
    edges.extend(circle_edges(&mut duplicate));
    assert!(duplicate.execute("hatch", &json!({"handles":edges})).is_err());
}

#[test]
fn too_many_unique_boundary_edges_reject_before_tracing() {
    let mut s = Session::new();
    let mut edges = Vec::new();
    let p = |i: usize| {
        let t = i as f64 * std::f64::consts::TAU / 513.0;
        [100.0 * t.cos(), 100.0 * t.sin()]
    };
    for i in 0..513 {
        edges.push(line(&mut s, p(i), p(i + 1)));
    }
    assert!(s.execute("hatch", &json!({"handles":edges})).is_err());
    assert_eq!(s.doc().unwrap().model.len(), 513);
}
