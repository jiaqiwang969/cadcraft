//! Member identity: what an entity inside the block of a block reference or of a dimension draws
//! names that entity ([`DisplayList::member_path`]), while [`DPrim::handle`] keeps naming the
//! top-level entity as before.
//!
//! [`DPrim::handle`]: cadcraft_render::DPrim::handle

use cadcraft_color::Color;
use cadcraft_doc::{
    Attrib, Block, Circle, Common, DimKind, DimStyle, Dimension, Drawing, Entity, EntityKind, Handle, Insert, Line, MLeader, MText, Solid, Space,
    Text, Viewport, Wipeout,
};
use cadcraft_geom::{Bounds2, Vec2, Vec3};
use cadcraft_render::{DisplayList, Kind, Options, VIEWPORT_CONTENT, build, build_plot, viewport_content};

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> EntityKind {
    EntityKind::Line(Line { a: Vec3::new(x0, y0, 0.0), b: Vec3::new(x1, y1, 0.0) })
}

fn insert(block: &str, x: f64, y: f64) -> Insert {
    Insert {
        block: block.into(),
        insert: Vec3::new(x, y, 0.0),
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: 0.0,
        attribs: vec![],
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    }
}

fn define(d: &mut Drawing, name: &str, entities: Vec<(u64, EntityKind)>) {
    let mut blk = Block::new(name);
    for (h, k) in entities {
        blk.entities.push(Entity::new(Handle(h), k));
    }
    d.blocks.insert(name.into(), std::sync::Arc::new(blk));
}

/// Three blocks, each holding the next: "Top" (a line 0x1A and "Mid" as 0x1B), "Mid" (a circle
/// 0x2A and "Leaf" as 0x2B) and "Leaf" (a line 0x3A).
fn nested(d: &mut Drawing) {
    define(d, "Leaf", vec![(0x3A, line(0.0, 0.0, 1.0, 1.0))]);
    let circle = EntityKind::Circle(Circle { center: Vec3::new(2.0, 2.0, 0.0), radius: 1.0 });
    define(d, "Mid", vec![(0x2A, circle), (0x2B, EntityKind::Insert(insert("Leaf", 4.0, 0.0)))]);
    define(d, "Top", vec![(0x1A, line(0.0, 5.0, 9.0, 5.0)), (0x1B, EntityKind::Insert(insert("Mid", 0.0, 1.0)))]);
}

/// The member paths of the primitives of top-level entity `top`, in drawing order, each once.
fn paths(list: &DisplayList, top: Handle) -> Vec<Vec<u64>> {
    let mut out: Vec<Vec<u64>> = Vec::new();
    for (i, p) in list.prims.iter().enumerate() {
        let path: Vec<u64> = list.member_path(i).iter().map(|h| h.0).collect();
        if p.handle == top && out.last() != Some(&path) {
            out.push(path);
        }
    }
    out
}

/// A linear dimension from (0, 0) to (10, 0) with its line at y = 5, in style "Fill".
fn dimension(block: Option<&str>) -> Dimension {
    Dimension {
        kind: DimKind::Linear { rotation: 0.0 },
        defpt: Vec3::new(0.0, 5.0, 0.0),
        text_mid: Vec3::ZERO,
        p13: Vec3::ZERO,
        p14: Vec3::new(10.0, 0.0, 0.0),
        p15: Vec3::ZERO,
        p16: Vec3::ZERO,
        text: String::new(),
        style: "Fill".into(),
        measurement: 0.0,
        text_rotation: 0.0,
        user_text_pos: false,
        block: block.map(Into::into),
        block_offset: Default::default(),
        overrides: Default::default(),
        assoc: Vec::new(),
        marks: Default::default(),
    }
}

fn mtext(x: f64, y: f64, contents: &str) -> EntityKind {
    EntityKind::MText(MText {
        insert: Vec3::new(x, y, 0.0),
        height: 0.18,
        width: 0.0,
        attach: 5,
        rotation: 0.0,
        style: "Standard".into(),
        contents: contents.into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
    })
}

fn attrib(tag: &str, value: &str) -> Attrib {
    Attrib {
        tag: tag.into(),
        text: Text {
            insert: Vec3::new(1.0, 1.0, 0.0),
            align_pt: None,
            height: 1.0,
            value: value.into(),
            rotation: 0.0,
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign: Default::default(),
            valign: Default::default(),
        },
        invisible: false,
        constant: false,
        prompt: String::new(),
        props: Default::default(),
        multiline: None,
        modes: Default::default(),
    }
}

#[test]
fn nested_block_references_name_every_level() {
    let mut d = Drawing::new_imperial();
    nested(&mut d);
    let plain = d.add(&Space::Model, Common::default(), line(-5.0, 0.0, -1.0, 0.0)).unwrap();
    let top = d.add(&Space::Model, Common::default(), EntityKind::Insert(insert("Top", 0.0, 0.0))).unwrap();
    let list = build(&d, &Space::Model, &Options::default());
    // The top-level entity is still the owner of everything it shows.
    assert!(list.prims.iter().all(|p| p.handle == plain || p.handle == top));
    assert_eq!(paths(&list, plain), [Vec::<u64>::new()], "a line is drawn by itself");
    assert_eq!(paths(&list, top), [vec![0x1A], vec![0x1B, 0x2A], vec![0x1B, 0x2B, 0x3A]]);
    assert!(list.prim_members.len() <= list.prims.len());
}

#[test]
fn every_copy_and_every_reference_keeps_its_own_top_level_entity() {
    let mut d = Drawing::new_imperial();
    nested(&mut d);
    let first = d.add(&Space::Model, Common::default(), EntityKind::Insert(insert("Leaf", 0.0, 0.0))).unwrap();
    // A second reference of the same block, as a 2 x 3 array.
    let array = Insert { cols: 2, rows: 3, col_spacing: 10.0, row_spacing: 10.0, ..insert("Leaf", 50.0, 0.0) };
    let second = d.add(&Space::Model, Common::default(), EntityKind::Insert(array)).unwrap();
    let list = build(&d, &Space::Model, &Options::default());
    assert_eq!(list.prims.iter().filter(|p| p.handle == first).count(), 1);
    assert_eq!(list.prims.iter().filter(|p| p.handle == second).count(), 6);
    assert_eq!(paths(&list, first), [vec![0x3A]]);
    assert_eq!(paths(&list, second), [vec![0x3A]], "each copy of the array is the same member");
}

#[test]
fn an_entity_without_a_handle_stays_with_what_holds_it() {
    let mut d = Drawing::new_imperial();
    define(&mut d, "Loose", vec![(0, line(0.0, 0.0, 1.0, 1.0)), (0x4C, line(0.0, 1.0, 1.0, 2.0))]);
    define(&mut d, "Holder", vec![(0x5D, EntityKind::Insert(insert("Loose", 0.0, 0.0)))]);
    let top = d.add(&Space::Model, Common::default(), EntityKind::Insert(insert("Loose", 0.0, 0.0))).unwrap();
    let held = d.add(&Space::Model, Common::default(), EntityKind::Insert(insert("Holder", 9.0, 0.0))).unwrap();
    let list = build(&d, &Space::Model, &Options::default());
    assert_eq!(paths(&list, top), [vec![], vec![0x4C]]);
    assert_eq!(paths(&list, held), [vec![0x5D], vec![0x5D, 0x4C]]);
}

#[test]
fn a_dimension_block_names_its_members() {
    let mut d = Drawing::new_imperial();
    d.dim_styles.push(DimStyle { name: "Fill".into(), ..DimStyle::default() });
    // The block as read from a file: a line, an arrowhead block reference and the text.
    let tick = Solid { corners: [(0.0, 0.0), (0.5, 0.1), (0.5, -0.1), (0.5, -0.1)].map(|(x, y)| Vec3::new(x, y, 0.0)) };
    define(&mut d, "_Tick", vec![(0x6E, EntityKind::Solid(tick))]);
    define(
        &mut d,
        "*D7",
        vec![(0x7A, line(0.0, 5.0, 10.0, 5.0)), (0x7B, EntityKind::Insert(insert("_Tick", 0.0, 5.0))), (0x7C, mtext(5.0, 5.5, "10.0000"))],
    );
    let with_block = d.add(&Space::Model, Common::default(), EntityKind::Dimension(dimension(Some("*D7")))).unwrap();
    let generated = d.add(&Space::Model, Common::default(), EntityKind::Dimension(dimension(None))).unwrap();
    let list = build(&d, &Space::Model, &Options::default());
    assert_eq!(paths(&list, with_block), [vec![0x7A], vec![0x7B, 0x6E], vec![0x7C]]);
    assert_eq!(paths(&list, generated), [Vec::<u64>::new()], "geometry made for a dimension without a block is the dimension's own");
}

#[test]
fn a_text_background_belongs_to_its_text() {
    let mut d = Drawing::new_imperial();
    d.dim_styles.push(DimStyle { name: "Fill".into(), text_gap: 0.5, text_fill: 1, text_fill_color: Color::ByBlock, ..DimStyle::default() });
    // A measurement and a tolerance as two texts of the block, after its line.
    define(&mut d, "*D7", vec![(0x7A, line(0.0, 5.0, 10.0, 5.0)), (0x7C, mtext(20.0, 20.0, "10.0000")), (0x7D, mtext(20.0, 22.0, "+0.1"))]);
    let dim = d.add(&Space::Model, Common::default(), EntityKind::Dimension(dimension(Some("*D7")))).unwrap();
    let generated = d.add(&Space::Model, Common::default(), EntityKind::Dimension(dimension(None))).unwrap();
    let list = build(&d, &Space::Model, &Options::default());
    let masks: Vec<(usize, f64)> = list
        .prims
        .iter()
        .enumerate()
        .filter(|(_, p)| p.kind == Kind::Mask && p.handle == dim)
        .map(|(i, p)| (i, list.points(p).iter().map(|q| q.y).sum::<f64>() / list.points(p).len() as f64))
        .collect();
    let [(lower, y0), (upper, y1)] = masks.as_slice() else { panic!("two backgrounds: {masks:?}") };
    assert!((y0 - 20.0).abs() < 0.2 && (y1 - 22.0).abs() < 0.2, "{masks:?}");
    assert_eq!(list.member_path(*lower), [Handle(0x7C)]);
    assert_eq!(list.member_path(*upper), [Handle(0x7D)]);
    // Both backgrounds come before the first text, each still its own text's.
    assert_eq!(paths(&list, dim), [vec![0x7A], vec![0x7C], vec![0x7D], vec![0x7C], vec![0x7D]]);
    // The background of a dimension drawn without a block is the dimension's own.
    let own = list.prims.iter().position(|p| p.kind == Kind::Mask && p.handle == generated).expect("a background");
    assert!(list.member_path(own).is_empty());
}

#[test]
fn an_attribute_is_drawn_by_its_block_reference() {
    // An attribute has no handle of its own in the drawing: its text stays with the reference
    // that carries it, and the definition in the block is not drawn.
    let mut d = Drawing::new_imperial();
    define(&mut d, "Tagged", vec![(0x8A, line(0.0, 0.0, 10.0, 0.0)), (0x8B, EntityKind::AttDef(attrib("TAG", "")))]);
    let tagged = Insert { attribs: vec![attrib("TAG", "A1")], ..insert("Tagged", 0.0, 0.0) };
    define(&mut d, "Holder", vec![(0x9C, EntityKind::Insert(tagged.clone()))]);
    let top = d.add(&Space::Model, Common::default(), EntityKind::Insert(tagged)).unwrap();
    let held = d.add(&Space::Model, Common::default(), EntityKind::Insert(insert("Holder", 30.0, 0.0))).unwrap();
    let list = build(&d, &Space::Model, &Options::default());
    assert!(list.prims.iter().filter(|p| p.handle == top).count() > 2, "the line and the attribute's strokes");
    assert_eq!(paths(&list, top), [vec![0x8A], vec![]]);
    assert_eq!(paths(&list, held), [vec![0x9C, 0x8A], vec![0x9C]]);
}

#[test]
fn a_wipeout_in_a_block_is_a_member_and_stays_a_mask() {
    let square = || {
        EntityKind::Wipeout(Wipeout { boundary: [(2.0, 2.0), (8.0, 2.0), (8.0, 8.0), (2.0, 8.0)].iter().map(|&(x, y)| Vec2::new(x, y)).collect() })
    };
    let mut d = Drawing::new_imperial();
    define(&mut d, "Cover", vec![(0xAD, square())]);
    let held = d.add(&Space::Model, Common::default(), EntityKind::Insert(insert("Cover", 0.0, 0.0))).unwrap();
    let own = d.add(&Space::Model, Common::default(), square()).unwrap();
    let list = build(&d, &Space::Model, &Options::default());
    for (top, path) in [(held, vec![Handle(0xAD)]), (own, vec![])] {
        let mask = list.prims.iter().position(|p| p.handle == top && p.kind == Kind::Mask).expect("a mask");
        assert_eq!(list.member_path(mask), path);
        let frame = list.prims.iter().position(|p| p.handle == top && p.kind == Kind::Polyline).expect("a frame");
        assert_eq!(list.member_path(frame), path);
    }
}

#[test]
fn the_block_a_multileader_shows_is_part_of_the_multileader() {
    let leader = |block: &str| {
        EntityKind::MLeader(MLeader {
            leaders: vec![vec![Vec3::new(0.0, 0.0, 0.0)]],
            landing: Vec3::new(5.0, 5.0, 0.0),
            dogleg: 1.0,
            text: None,
            style: "Standard".into(),
            arrow_size: 1.0,
            spline: false,
            block: Some(insert(block, 7.0, 5.0)),
        })
    };
    let mut d = Drawing::new_imperial();
    nested(&mut d);
    define(&mut d, "Callout", vec![(0xBE, leader("Top"))]);
    let own = d.add(&Space::Model, Common::default(), leader("Top")).unwrap();
    let held = d.add(&Space::Model, Common::default(), EntityKind::Insert(insert("Callout", 40.0, 0.0))).unwrap();
    let list = build(&d, &Space::Model, &Options::default());
    // The leader line, the landing and the three levels of the block: one identity.
    assert!(list.prims.iter().filter(|p| p.handle == own).count() >= 5);
    assert_eq!(paths(&list, own), [Vec::<u64>::new()]);
    assert_eq!(paths(&list, held), [vec![0xBE]]);
}

#[test]
fn model_space_seen_through_a_viewport_has_no_members() {
    let mut d = Drawing::new_imperial();
    nested(&mut d);
    d.add(&Space::Model, Common::default(), EntityKind::Insert(insert("Top", 0.0, 0.0))).unwrap();
    let paper = Space::Paper("Layout1".into());
    let vp: Viewport = serde_json::from_value(serde_json::json!({
        "center": [50.0, 50.0, 0.0], "width": 100.0, "height": 100.0, "viewCenter": [5.0, 5.0], "viewHeight": 20.0, "id": 2,
    }))
    .unwrap();
    let view = d.add(&paper, Common::default(), EntityKind::Viewport(vp)).unwrap();
    // A block reference on the sheet itself, drawn after the viewport's contents.
    let on_paper = d.add(&paper, Common::default(), EntityKind::Insert(insert("Top", 200.0, 0.0))).unwrap();
    for list in [build(&d, &paper, &Options::default()), build_plot(&d, &paper, &Options::default())] {
        let seen: Vec<usize> = list.prims.iter().enumerate().filter(|(_, p)| p.handle == VIEWPORT_CONTENT).map(|(i, _)| i).collect();
        assert!(seen.len() >= 3, "the block reference shows through the viewport");
        assert!(seen.iter().all(|i| list.member_path(*i).is_empty()));
        assert_eq!(paths(&list, view), [Vec::<u64>::new()], "the viewport's border");
        assert_eq!(paths(&list, on_paper), [vec![0x1A], vec![0x1B, 0x2A], vec![0x1B, 0x2B, 0x3A]]);
    }
    let area = Bounds2::new(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
    let content = viewport_content(&d, &paper, view, area, &Options::default());
    assert!(!content.prims.is_empty() && content.prim_members.is_empty() && content.members.is_empty());
}

#[test]
fn a_list_built_on_all_cores_keeps_its_members() {
    // Enough top-level entities for the build to split them into chunks built side by side
    // (a plot is built on one thread): lines, and block references spread over the chunks.
    let mut d = Drawing::new_imperial();
    nested(&mut d);
    let mut references = Vec::new();
    for i in 0..700 {
        let x = f64::from(i) * 20.0;
        if i % 7 == 3 {
            references.push(d.add(&Space::Model, Common::default(), EntityKind::Insert(insert("Top", x, 0.0))).unwrap());
        } else {
            d.add(&Space::Model, Common::default(), line(x, 0.0, x + 9.0, 3.0)).unwrap();
        }
    }
    let list = build(&d, &Space::Model, &Options::default());
    let one_thread = build_plot(&d, &Space::Model, &Options::default());
    assert_eq!(list.prims.len(), one_thread.prims.len());
    for i in 0..list.prims.len() {
        assert_eq!(list.prims[i].handle, one_thread.prims[i].handle);
        assert_eq!(list.member_path(i), one_thread.member_path(i), "primitive {i}");
    }
    assert_eq!(references.len(), 100);
    for top in references {
        assert_eq!(paths(&list, top), [vec![0x1A], vec![0x1B, 0x2A], vec![0x1B, 0x2B, 0x3A]]);
    }
}
