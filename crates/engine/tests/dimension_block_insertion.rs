//! Imported dimension caches must be selected and snapped at the position they draw.

use cadcraft_doc::{Drawing, EntityKind, Space};
use cadcraft_engine::{
    select::pick,
    snap::{self, mode},
};
use cadcraft_geom::Vec2;

fn drawing() -> Drawing {
    cadcraft_io::read_dxf(
        b"0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\n*D1\n70\n1\n10\n0\n20\n0\n\
        0\nLINE\n5\nA1\n8\n0\n10\n0\n20\n5\n11\n10\n21\n5\n\
        0\nMTEXT\n5\nA2\n8\n0\n10\n50\n20\n50\n40\n5\n71\n5\n1\nH H\n\
        0\nTEXT\n5\nA3\n8\n0\n10\n50\n20\n70\n40\n5\n1\nH H\n\
        0\nENDBLK\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
        0\nDIMENSION\n5\nD1\n8\n0\n2\n*D1\n70\n32\n\
        10\n100\n20\n205\n11\n105\n21\n206\n12\n100\n22\n200\n13\n100\n23\n200\n14\n110\n24\n200\n50\n0\n\
        0\nENDSEC\n0\nEOF\n",
    )
    .unwrap()
}

#[test]
fn endpoints_and_midpoints_follow_the_cache_but_definition_nodes_stay_in_wcs() {
    let d = drawing();
    for (point, mode) in [(Vec2::new(110.0, 205.0), mode::END), (Vec2::new(105.0, 205.0), mode::MID), (Vec2::new(100.0, 200.0), mode::NOD)] {
        let found = snap::osnap(&d, &Space::Model, point + Vec2::new(0.02, -0.01), 0.1, mode, None, false).unwrap();
        assert_eq!(found.point, point);
        assert_eq!(found.mode, mode);
    }
    assert!(snap::osnap(&d, &Space::Model, Vec2::new(10.0, 5.0), 0.1, mode::END, None, false).is_none());
}

#[test]
fn picking_the_cached_line_and_text_uses_the_same_offset_as_rendering() {
    let d = drawing();
    let h = d.model.iter().next().unwrap().handle;
    assert_eq!(pick(&d, &Space::Model, Vec2::new(105.0, 205.0), 0.01), Some(h));
    assert_eq!(pick(&d, &Space::Model, Vec2::new(5.0, 5.0), 0.01), None);
    let block = d.block("*D1").unwrap();
    for e in block.entities.iter() {
        let center = match &e.kind {
            EntityKind::MText(t) => cadcraft_render::layout_mtext_entity(&d, t).bounds.center(),
            EntityKind::Text(_) => cadcraft_doc::entity_bounds(&d, e, 0).center(),
            _ => continue,
        };
        assert_eq!(pick(&d, &Space::Model, center + Vec2::new(100.0, 200.0), 1e-6), Some(h), "on cached text");
        assert_eq!(pick(&d, &Space::Model, center, 1e-6), None, "not at the untranslated text");
    }
}
