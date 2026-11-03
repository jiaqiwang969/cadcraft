//! Synthetic dimensions whose cached geometry has a nonzero DXF group 12 insertion.
//! The definition points are already in WCS; only the cached block needs the translation.

use cadcraft_doc::{Block, Common, Dimension, Drawing, Entity, EntityKind, EntityStore, Insert, Space};
use cadcraft_geom::{Mat3, Vec2, Vec3};

fn source() -> String {
    let mut dxf = String::from(
        "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\n*D1\n70\n1\n10\n0\n20\n0\n30\n0\n\
         0\nLINE\n5\nA1\n8\n0\n10\n0\n20\n5\n11\n10\n21\n5\n\
         0\nSOLID\n5\nA2\n8\n0\n10\n10\n20\n5\n11\n8\n21\n4.5\n12\n8\n22\n5.5\n13\n8\n23\n5.5\n\
         0\nENDBLK\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n",
    );
    for (handle, x, y) in [("D1", 100.0, 200.0), ("D2", -50.0, -80.0), ("D3", 0.0, 0.0)] {
        dxf.push_str(&format!(
            "0\nDIMENSION\n5\n{handle}\n8\n0\n2\n*D1\n70\n32\n\
             10\n{x}\n20\n{}\n11\n{}\n21\n{}\n\
             12\n{x}\n22\n{y}\n32\n0\n13\n{x}\n23\n{y}\n14\n{}\n24\n{y}\n50\n0\n",
            y + 5.0,
            x + 5.0,
            y + 6.0,
            x + 10.0,
        ));
    }
    dxf.push_str("0\nENDSEC\n0\nEOF\n");
    dxf
}

fn drawing() -> Drawing {
    cadcraft_io::read_dxf(source().as_bytes()).unwrap()
}

fn dimension(e: &Entity) -> &Dimension {
    let EntityKind::Dimension(dm) = &e.kind else { panic!("dimension") };
    dm
}

fn rendered(d: &Drawing, e: &Entity) -> Vec<Vec2> {
    let list = cadcraft_render::build_entities(d, std::iter::once(e), &cadcraft_render::Options { text: false, ..Default::default() });
    list.prims.iter().flat_map(|p| list.points(p).iter().copied()).collect()
}

#[test]
fn cached_dimension_arrows_apply_each_references_insertion() {
    let d = drawing();
    for (e, offset) in d.model.iter().zip([Vec2::new(100.0, 200.0), Vec2::new(-50.0, -80.0), Vec2::ZERO]) {
        assert!(matches!(e.kind, EntityKind::Dimension(_)));
        let points = rendered(&d, e);
        let tip = offset + Vec2::new(10.0, 5.0);
        assert!(points.iter().any(|p| p.dist(tip) < 1e-9), "arrow at {tip:?}, got {points:?}");
        assert!(points.iter().all(|p| p.x >= offset.x && p.x <= offset.x + 10.0 && p.y >= offset.y + 4.5 && p.y <= offset.y + 5.5));
    }
    // Three references share the same block: applying one offset must not mutate it.
    let block = d.block("*D1").unwrap();
    let EntityKind::Line(line) = &block.entities.iter().next().unwrap().kind else { panic!("line") };
    assert_eq!(line.a.xy(), Vec2::new(0.0, 5.0));
}

#[test]
fn cached_dimension_extents_exclude_the_untranslated_block() {
    let d = drawing();
    let e = d.model.iter().next().unwrap();
    let b = cadcraft_doc::entity_bounds(&d, e, 0);
    assert!(b.min.x > 90.0 && b.min.y > 190.0, "untranslated block enlarged the bounds: {b:?}");
    assert!(b.contains(Vec2::new(110.0, 205.0)));
}

#[test]
fn cached_and_regenerated_dimensions_keep_positions_on_resave() {
    let mut d = drawing();
    for _ in 0..3 {
        // The first reference keeps the existing block; the other two get regenerated blocks.
        d = cadcraft_io::read_dxf(cadcraft_io::write_dxf(&d).as_bytes()).unwrap();
        for (i, (e, offset)) in d.model.iter().zip([Vec2::new(100.0, 200.0), Vec2::new(-50.0, -80.0), Vec2::ZERO]).enumerate() {
            let points = rendered(&d, e);
            let tip = offset + Vec2::new(10.0, 5.0);
            assert!(points.iter().any(|p| p.dist(tip) < 1e-9), "{points:?}");
            assert!(points.iter().all(|p| (p.x - offset.x).abs() < 20.0 && (p.y - offset.y).abs() < 20.0), "offset applied twice: {points:?}");
            assert_eq!(dimension(e).block_offset.xy(), if i == 0 { offset } else { Vec2::ZERO });
        }
    }
}

#[test]
fn insertion_is_read_in_ocs_and_saved_in_the_output_frame() {
    for (normal, expected) in
        [("210\n0\n220\n0\n230\n-1\n", Vec3::new(-100.0, 200.0, 0.0)), ("210\n0\n220\n1\n230\n0\n", Vec3::new(-100.0, 0.0, 200.0))]
    {
        let src = source().replace("50\n0\n", &format!("50\n0\n{normal}"));
        let d = cadcraft_io::read_dxf(src.as_bytes()).unwrap();
        let e = d.model.iter().next().unwrap();
        assert_eq!(dimension(e).block_offset, expected);
        assert!(rendered(&d, e).iter().any(|p| p.dist(expected.xy() + Vec2::new(10.0, 5.0)) < 1e-9));
        let back = cadcraft_io::read_dxf(cadcraft_io::write_dxf(&d).as_bytes()).unwrap();
        let be = back.model.iter().next().unwrap();
        assert_eq!(dimension(be).block_offset, expected);
        assert_eq!(rendered(&back, be), rendered(&d, e));
    }
}

#[test]
fn a_nested_dimension_composes_its_offset_before_the_outer_insert_and_resaves() {
    let mut d = drawing();
    let dim = d.model.iter().next().unwrap().as_ref().clone();
    d.model = EntityStore::new();
    let mut part = Block::new("Part");
    part.base = Vec3::new(3.0, 4.0, 0.0);
    part.entities.push(dim);
    d.blocks.insert("Part".into(), std::sync::Arc::new(part));
    d.add(
        &Space::Model,
        Common::default(),
        EntityKind::Insert(Insert {
            block: "Part".into(),
            insert: Vec3::new(-20.0, 50.0, 0.0),
            scale: Vec3::new(2.0, 3.0, 1.0),
            rotation: std::f64::consts::FRAC_PI_2,
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        }),
    )
    .unwrap();
    // (110, 205) minus the base (3, 4), scaled (2, 3), turned 90°, then moved (-20, 50).
    let tip = Vec2::new(-623.0, 264.0);
    for _ in 0..3 {
        let e = d.model.iter().next().unwrap();
        assert!(rendered(&d, e).iter().any(|p| p.dist(tip) < 1e-9));
        assert!(cadcraft_doc::entity_bounds(&d, e, 0).contains(tip));
        assert_eq!(dimension(d.block("Part").unwrap().entities.iter().next().unwrap()).block_offset, Vec3::new(100.0, 200.0, 0.0));
        d = cadcraft_io::read_dxf(cadcraft_io::write_dxf(&d).as_bytes()).unwrap();
    }
}

#[test]
fn editing_clears_the_cached_offset_without_moving_definition_points_twice() {
    let mut d = drawing();
    let mut e = d.model.iter().next().unwrap().as_ref().clone();
    e.kind.transform(&Mat3::translate(Vec2::new(7.0, 9.0)));
    assert!(dimension(&e).block.is_none());
    assert_eq!(dimension(&e).block_offset, Vec3::ZERO);
    assert_eq!(dimension(&e).p13, Vec3::new(107.0, 209.0, 0.0));
    d.model = EntityStore::new();
    d.model.push(e);
    let back = cadcraft_io::read_dxf(cadcraft_io::write_dxf(&d).as_bytes()).unwrap();
    let e = back.model.iter().next().unwrap();
    assert!(rendered(&back, e).iter().any(|p| p.dist(Vec2::new(117.0, 214.0)) < 1e-9));
}

#[test]
fn older_serialized_dimensions_default_to_zero_offset() {
    let d = drawing();
    let mut value = serde_json::to_value(dimension(d.model.iter().next().unwrap())).unwrap();
    value.as_object_mut().unwrap().remove("blockOffset");
    let dm: Dimension = serde_json::from_value(value).unwrap();
    assert_eq!(dm.block_offset, Vec3::ZERO);
}
