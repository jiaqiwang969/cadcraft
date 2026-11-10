//! SVG export names the entity inside a block reference's or a dimension's block that drew a
//! group (`data-member`: the handles from the top-level entity down to it, joined by `/`), next
//! to the top-level entity (`data-entity`). The names are the handles of the drawing's DXF.

use std::collections::{BTreeMap, BTreeSet};

use cadcraft_doc::{Attrib, Block, Circle, Common, DimKind, Dimension, Drawing, Entity, EntityKind, Handle, Insert, Line, Space, Text};
use cadcraft_geom::Vec3;
use cadcraft_io::svg;

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

fn text(value: &str) -> Text {
    Text {
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
    }
}

fn attrib(tag: &str, value: &str) -> Attrib {
    Attrib {
        tag: tag.into(),
        text: text(value),
        invisible: false,
        constant: false,
        prompt: String::new(),
        props: Default::default(),
        multiline: None,
        modes: Default::default(),
    }
}

/// A block of `entities`, each with a fresh handle of the drawing; the handles, in order.
fn define(d: &mut Drawing, name: &str, entities: Vec<EntityKind>) -> Vec<Handle> {
    let mut blk = Block::new(name);
    let mut handles = Vec::new();
    for k in entities {
        let h = d.new_handle();
        handles.push(h);
        blk.entities.push(Entity::new(h, k));
    }
    d.blocks.insert(name.into(), std::sync::Arc::new(blk));
    handles
}

/// A linear dimension from (0, 20) to (10, 20) with its line at y = 25, without a block: the
/// DXF writer makes its `*D` block.
fn dimension() -> Dimension {
    Dimension {
        kind: DimKind::Linear { rotation: 0.0 },
        defpt: Vec3::new(0.0, 25.0, 0.0),
        text_mid: Vec3::ZERO,
        p13: Vec3::new(0.0, 20.0, 0.0),
        p14: Vec3::new(10.0, 20.0, 0.0),
        p15: Vec3::ZERO,
        p16: Vec3::ZERO,
        text: String::new(),
        style: "Standard".into(),
        measurement: 0.0,
        text_rotation: 0.0,
        user_text_pos: false,
        block: None,
        block_offset: Default::default(),
        overrides: Default::default(),
        assoc: Vec::new(),
        marks: Default::default(),
    }
}

/// A line, a reference of a block that holds a line and a second block (a circle and a third
/// block with a line), a reference with an attribute, and a dimension.
struct Sample {
    d: Drawing,
    plain: Handle,
    nested: Handle,
    /// The block entities under `nested`: the outer line, the reference of the middle block, the
    /// circle, the reference of the inner block and the inner line.
    held: [Handle; 5],
    tagged: Handle,
    tagged_line: Handle,
    dimension: Handle,
}

fn sample() -> Sample {
    let mut d = Drawing::new_metric();
    let inner = define(&mut d, "Inner", vec![line(0.0, 0.0, 1.0, 1.0)]);
    let circle = EntityKind::Circle(Circle { center: Vec3::new(2.0, 2.0, 0.0), radius: 1.0 });
    let middle = define(&mut d, "Middle", vec![circle, EntityKind::Insert(insert("Inner", 4.0, 0.0))]);
    let outer = define(&mut d, "Outer", vec![line(0.0, 5.0, 9.0, 5.0), EntityKind::Insert(insert("Middle", 0.0, 1.0))]);
    let tag = define(&mut d, "Tagged", vec![line(0.0, 0.0, 10.0, 0.0), EntityKind::AttDef(attrib("TAG", ""))]);
    let plain = d.add(&Space::Model, Common::default(), line(-9.0, 0.0, -1.0, 0.0)).unwrap();
    let nested = d.add(&Space::Model, Common::default(), EntityKind::Insert(insert("Outer", 0.0, 0.0))).unwrap();
    let with_attribute = Insert { attribs: vec![attrib("TAG", "K7")], ..insert("Tagged", 30.0, 0.0) };
    let tagged = d.add(&Space::Model, Common::default(), EntityKind::Insert(with_attribute)).unwrap();
    let dimension = d.add(&Space::Model, Common::default(), EntityKind::Dimension(dimension())).unwrap();
    Sample { d, plain, nested, held: [outer[0], outer[1], middle[0], middle[1], inner[0]], tagged, tagged_line: tag[0], dimension }
}

/// Each group's `data-entity` and `data-member` ("" when it has none), in order.
fn groups(s: &str) -> Vec<(String, String)> {
    let value = |g: &str, name: &str| g.split(&format!("{name}=\"")).nth(1).and_then(|r| r.split('"').next()).unwrap_or_default().to_owned();
    s.split("<g ").skip(1).map(|g| g.split('>').next().unwrap_or_default()).map(|g| (value(g, "data-entity"), value(g, "data-member"))).collect()
}

/// The `data-member` values of the groups of top-level entity `top`, in order, each once.
fn members(s: &str, top: Handle) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (e, m) in groups(s) {
        if e == top.hex() && out.last() != Some(&m) {
            out.push(m);
        }
    }
    out
}

/// `s` without its ` data-member="…"` attributes.
fn without_members(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(at) = rest.find(" data-member=\"") {
        out.push_str(&rest[..at]);
        let value = &rest[at + " data-member=\"".len()..];
        rest = &value[value.find('"').map_or(value.len(), |e| e + 1)..];
    }
    out.push_str(rest);
    out
}

#[test]
fn groups_name_the_nested_entity_that_drew_them() {
    let s = sample();
    let out = svg::export(&s.d, &Space::Model);
    let id = |path: &[Handle]| path.iter().map(|h| h.hex()).collect::<Vec<_>>().join("/");
    let [outer_line, middle_ref, circle, inner_ref, inner_line] = s.held;
    assert_eq!(members(&out, s.plain), [""], "a top-level line names no member");
    assert_eq!(
        members(&out, s.nested),
        [id(&[s.nested, outer_line]), id(&[s.nested, middle_ref, circle]), id(&[s.nested, middle_ref, inner_ref, inner_line])]
    );
    // The attribute's text is drawn by the reference itself; the definition is not drawn.
    assert_eq!(members(&out, s.tagged), [id(&[s.tagged, s.tagged_line]), String::new()]);
    // A dimension without a block draws its own geometry.
    assert_eq!(members(&out, s.dimension), [""]);
    assert!(out.contains(&format!(r#"<g data-entity="{}" data-member="{}"><polyline "#, s.nested.hex(), id(&[s.nested, outer_line]))), "{out}");
}

#[test]
fn only_the_member_attribute_is_added() {
    let s = sample();
    let out = svg::export(&s.d, &Space::Model);
    let mut list = cadcraft_render::build(&s.d, &Space::Model, &cadcraft_render::Options { transparency: true, ..Default::default() });
    assert_eq!(out, svg::export_list(&list, None));
    assert!(!list.prim_members.is_empty() && !list.members.is_empty());
    list.prim_members.clear();
    list.members.clear();
    let plain = svg::export_list(&list, None);
    assert!(!plain.contains("data-member") && out.contains("data-member"));
    assert_eq!(without_members(&out), plain, "the same groups, order, attributes and numbers");
}

#[test]
fn model_space_seen_through_a_viewport_names_no_member() {
    let mut s = sample();
    let paper = Space::Paper("Layout1".into());
    let vp: cadcraft_doc::Viewport = serde_json::from_value(serde_json::json!({
        "center": [50.0, 50.0, 0.0], "width": 100.0, "height": 100.0, "viewCenter": [5.0, 5.0], "viewHeight": 40.0, "id": 2,
    }))
    .unwrap();
    s.d.add(&paper, Common::default(), EntityKind::Viewport(vp)).unwrap();
    let on_paper = s.d.add(&paper, Common::default(), EntityKind::Insert(insert("Inner", 150.0, 0.0))).unwrap();
    let out = svg::export(&s.d, &paper);
    // The model's block references show through the viewport, owned by no entity of the sheet.
    assert!(out.matches(r#"<g data-unowned="viewport-content">"#).count() >= 3, "{out}");
    assert_eq!(out.matches("data-unowned").count(), out.matches(r#"<g data-unowned="viewport-content">"#).count());
    // The only members are those of the block reference on the sheet itself.
    let named: Vec<(String, String)> = groups(&out).into_iter().filter(|(_, m)| !m.is_empty()).collect();
    assert_eq!(named, [(on_paper.hex(), format!("{}/{}", on_paper.hex(), s.held[4].hex()))]);
}

/// The records of a DXF text: each as its type and its groups, with the section and the block
/// it is in.
struct Record {
    kind: String,
    section: String,
    block: String,
    tags: Vec<(i32, String)>,
}

impl Record {
    fn get(&self, code: i32) -> Option<&str> {
        self.tags.iter().find(|t| t.0 == code).map(|t| t.1.as_str())
    }
}

fn records(dxf: &str) -> Vec<Record> {
    let lines: Vec<&str> = dxf.lines().collect();
    let mut out: Vec<Record> = Vec::new();
    let (mut section, mut block) = (String::new(), String::new());
    for pair in lines.chunks(2) {
        let [code, value] = pair else { break };
        let (code, value) = (code.trim().parse::<i32>().unwrap(), value.trim_end_matches('\r'));
        if code == 0 {
            if value == "ENDSEC" {
                section.clear();
            }
            if value == "ENDBLK" {
                block.clear();
            }
            out.push(Record { kind: value.into(), section: section.clone(), block: block.clone(), tags: Vec::new() });
            continue;
        }
        let Some(r) = out.last_mut() else { continue };
        if code == 2 && r.kind == "SECTION" {
            section = value.into();
        }
        if code == 2 && r.kind == "BLOCK" {
            block = value.into();
        }
        r.tags.push((code, value.into()));
    }
    out
}

/// Every member a reader of the DXF finds under the model-space entities: the handles from a
/// top-level entity through the blocks that block references and dimensions name, joined by `/`,
/// with the record's type.
fn dxf_members(dxf: &str) -> BTreeMap<String, String> {
    fn held(all: &[Record], r: &Record, path: &str, depth: usize, out: &mut BTreeMap<String, String>) {
        let (Some(block), true) = (r.get(2), matches!(r.kind.as_str(), "INSERT" | "DIMENSION") && depth < 8) else { return };
        for m in all.iter().filter(|m| m.section == "BLOCKS" && m.block == block && !matches!(m.kind.as_str(), "BLOCK" | "ENDBLK" | "SEQEND")) {
            let id = format!("{path}/{}", m.get(5).unwrap());
            out.insert(id.clone(), m.kind.clone());
            held(all, m, &id, depth + 1, out);
        }
    }
    let all = records(dxf);
    let mut out = BTreeMap::new();
    let mut top = String::new();
    for r in all.iter().filter(|r| r.section == "ENTITIES" && !matches!(r.kind.as_str(), "ENDSEC" | "SEQEND") && r.get(67).unwrap_or("0") == "0") {
        if r.kind == "ATTRIB" {
            // An attribute follows its block reference.
            out.insert(format!("{top}/{}", r.get(5).unwrap()), r.kind.clone());
            continue;
        }
        top = r.get(5).unwrap().to_owned();
        held(&all, r, &top, 0, &mut out);
    }
    out
}

#[test]
fn member_names_are_the_handles_of_the_written_dxf() {
    // Written and read again, as a converted drawing is: the dimension has its block now.
    let dxf = cadcraft_io::write_dxf(&sample().d);
    let d = cadcraft_io::read_dxf(dxf.as_bytes()).unwrap();
    let out = svg::export(&d, &Space::Model);
    let known = dxf_members(&dxf);
    let kinds: BTreeSet<&str> = known.values().map(String::as_str).collect();
    assert!(kinds.is_superset(&BTreeSet::from(["LINE", "CIRCLE", "INSERT", "ATTDEF", "ATTRIB", "MTEXT"])), "{kinds:?}");
    let mut named = BTreeSet::new();
    for (entity, member) in groups(&out) {
        if member.is_empty() {
            continue;
        }
        assert!(known.contains_key(&member), "{member} is no member in the DXF");
        assert!(member.starts_with(&format!("{entity}/")), "{member} under {entity}");
        named.insert(member);
    }
    // Every member that draws something has a group: all but the block references (their
    // contents are members themselves), the attribute definition (not drawn in a reference) and
    // the attribute (drawn by its reference, which keeps no handle for it).
    let silent = |kind: &str| matches!(kind, "INSERT" | "ATTDEF" | "ATTRIB");
    let drawn: BTreeSet<String> = known.iter().filter(|(_, kind)| !silent(kind)).map(|(id, _)| id.clone()).collect();
    assert_eq!(named, drawn);
    // The dimension's groups all name members of its block.
    let dims: Vec<&String> = known.iter().filter(|(_, kind)| *kind == "MTEXT").map(|(id, _)| id).collect();
    let [dim_text] = dims.as_slice() else { panic!("one dimension text: {dims:?}") };
    let top = dim_text.split('/').next().unwrap();
    assert!(groups(&out).iter().filter(|(e, _)| e == top).all(|(_, m)| !m.is_empty()));
}
