//! Programmatic SPLINE keeps the degree within 1..=10 like the Degree prompt (issue #268): a high
//! degree on many control points made tessellation and extents take minutes.

use cadcraft_engine::Session;
use cadcraft_engine::doc::EntityKind;
use serde_json::json;

#[test]
fn programmatic_spline_degree_is_limited() {
    let control: Vec<[f64; 2]> = (0..3000).map(|i| [f64::from(i), f64::from(i % 7)]).collect();
    for (degree, want) in [(json!(2_147_483_647), 10), (json!(2999), 10), (json!(11), 10), (json!(0), 1), (json!(4), 4)] {
        let mut s = Session::new();
        s.execute("spline", &json!({ "control": control, "degree": degree })).unwrap();
        let d = s.doc().unwrap();
        let Some(EntityKind::Spline(sp)) = d.model.iter().last().map(|e| &e.kind) else { panic!("no spline") };
        assert_eq!(sp.degree, want);
        let ext = d.extents(&s.space());
        assert!(ext.min.x <= 0.0 && ext.max.x >= 2999.0, "{ext:?}");
    }
}

/// Control points are capped like `spline.cv` (issue #719): one more than the cap is refused
/// without adding anything, and the cap itself still draws.
#[test]
fn programmatic_spline_control_count_is_limited() {
    let control = |n: i32| -> Vec<[f64; 2]> { (0..n).map(|i| [f64::from(i), f64::from(i % 7)]).collect() };
    let mut s = Session::new();
    assert!(s.execute("spline", &json!({ "control": control(20_001) })).is_err());
    assert_eq!(s.doc().unwrap().model.len(), 0);
    s.execute("spline", &json!({ "control": control(20_000) })).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 1);
}

#[test]
fn programmatic_rational_quadratic_preserves_the_circle_and_undo_redo() {
    let mut s = Session::new();
    let w = std::f64::consts::FRAC_1_SQRT_2;
    s.execute("spline", &json!({"control":[[1.,0.],[1.,1.],[0.,1.]],"degree":2,"weights":[1.,w,1.]})).unwrap();
    for _ in 0..2 {
        let d = s.doc().unwrap();
        let Some(EntityKind::Spline(sp)) = d.model.iter().last().map(|e| &e.kind) else { panic!("no rational spline") };
        assert_eq!(sp.degree, 2);
        assert_eq!(sp.knots, vec![0., 0., 0., 1., 1., 1.]);
        assert_eq!(sp.weights, vec![1., w, 1.]);
        assert!(sp.fit.is_empty());
        for t in [0., 0.125, 0.5, 0.875, 1.] {
            let p = sp.eval(t);
            assert!((p.x * p.x + p.y * p.y - 1.).abs() < 1e-12);
        }
        let midpoint = sp.eval(0.5);
        assert!((midpoint.x - w).abs() < 1e-12 && (midpoint.y - w).abs() < 1e-12);
        s.execute("undo", &json!({})).unwrap();
        assert_eq!(s.doc().unwrap().model.len(), 0);
        s.execute("redo", &json!({})).unwrap();
    }
}

#[test]
fn programmatic_rational_spline_refuses_invalid_weights_before_adding_an_entity() {
    for weights in [
        json!([]),
        json!([1., 1.]),
        json!([1., 0., 1.]),
        json!([1., -1., 1.]),
        json!([1., null, 1.]),
        json!([1., "1", 1.]),
        json!([1., 1e-13, 1.]),
        json!([1., 1e13, 1.]),
        json!("invalid"),
    ] {
        let mut s = Session::new();
        assert!(s.execute("spline", &json!({"control":[[1.,0.],[1.,1.],[0.,1.]],"degree":2,"weights":weights})).is_err());
        assert_eq!(s.doc().unwrap().model.len(), 0);
    }
    for request in [
        json!({"fit":[[0.,0.],[1.,1.]],"weights":[1.,1.]}),
        json!({"object":true,"weights":[1.,1.]}),
        json!({"control":[[1e13,0.],[1.,1.]],"weights":[1.,1.]}),
    ] {
        let mut s = Session::new();
        assert!(s.execute("spline", &request).is_err());
        assert_eq!(s.doc().unwrap().model.len(), 0);
    }
}

#[test]
fn rational_command_survives_dwg_and_dxf_save_reopen_twice() {
    let mut s = Session::new();
    let w = std::f64::consts::FRAC_1_SQRT_2;
    s.execute("spline", &json!({"control":[[1.,0.],[1.,1.],[0.,1.]],"degree":2,"weights":[1.,w,1.]})).unwrap();
    for name in ["rational.dwg", "rational.dxf"] {
        let mut bytes = cadcraft_io::write(s.doc().unwrap(), name).unwrap();
        for _ in 0..2 {
            let drawing = cadcraft_io::read(&bytes, name).unwrap();
            assert_eq!(drawing.model.len(), 1);
            let Some(EntityKind::Spline(sp)) = drawing.model.iter().next().map(|e| &e.kind) else { panic!("missing spline") };
            assert_eq!(sp.degree, 2);
            assert_eq!(sp.knots, vec![0., 0., 0., 1., 1., 1.]);
            assert_eq!(sp.weights.len(), 3);
            for (a, b) in sp.weights.iter().zip([1., w, 1.]) {
                assert!((a - b).abs() < 1e-14);
            }
            for t in [0., 0.125, 0.5, 0.875, 1.] {
                let p = sp.eval(t);
                assert!((p.x * p.x + p.y * p.y - 1.).abs() < 1e-12);
            }
            bytes = cadcraft_io::write(&drawing, name).unwrap();
        }
    }
}
