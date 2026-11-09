use super::*;

struct Fixture(PathBuf);

impl Fixture {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!("cadcraft-review-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn pack(&self, entries: Value) {
        std::fs::write(self.0.join("cadcraft-fonts.json"), json!({"schema":"cadcraft.font-pack/1","fonts":entries}).to_string()).unwrap();
    }

    fn pack_v2(&self, entries: Value, substitutes: Value) {
        let manifest = json!({"schema":"cadcraft.font-pack/2","fonts":entries,"substitutes":substitutes});
        std::fs::write(self.0.join("cadcraft-fonts.json"), manifest.to_string()).unwrap();
    }

    fn file(&self, name: &str, bytes: &[u8]) -> Value {
        std::fs::write(self.0.join(name), bytes).unwrap();
        json!({"name":name,"sha256":digest(bytes)})
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// An original single-stroke synthetic SHX; no real fonts or customer files.
fn synthetic_font() -> Vec<u8> {
    let mut bytes = b"AutoCAD-86 shapes 1.1\r\n\x1a".to_vec();
    for word in [0u16, 65, 2, 0, 5, 65, 6] {
        bytes.extend(word.to_le_bytes());
    }
    bytes.extend([0, 10, 2, 2, 0]);
    bytes.extend([0, 8, 0, 10, 0, 0]);
    assert!(shx::Face::parse(bytes.clone().into()).is_some());
    bytes
}

fn drawing(font: &str) -> Drawing {
    let mut d = Drawing::new_metric();
    d.text_styles[0].font = font.into();
    d
}

#[test]
fn review_rechecks_asset_bytes_even_if_a_font_is_already_cached() {
    let f = Fixture::new("changed");
    let bytes = synthetic_font();
    std::fs::write(f.0.join("review-changed.shx"), &bytes).unwrap();
    f.pack(json!([{"name":"review-changed.shx","sha256":digest(&bytes)}]));
    let d = drawing("review-changed");
    assert!(verify_fonts(&d, Some(&f.0)).is_ok());
    std::fs::write(f.0.join("review-changed.shx"), b"changed").unwrap();
    assert!(verify_fonts(&d, Some(&f.0)).unwrap_err().to_string().contains("hash mismatch"));
}

#[test]
fn review_missing_bigfont_cannot_be_hidden_by_a_present_base_font() {
    let f = Fixture::new("missing-big");
    let bytes = synthetic_font();
    std::fs::write(f.0.join("review-base.shx"), &bytes).unwrap();
    f.pack(json!([{"name":"review-base.shx","sha256":digest(&bytes)}]));
    let mut d = drawing("review-base");
    d.text_styles[0].big_font = "review-big".into();
    assert!(verify_fonts(&d, Some(&f.0)).unwrap_err().to_string().contains("required Big font"));
    d.text_styles[0].big_font = "review-base".into();
    assert!(verify_fonts(&d, Some(&f.0)).unwrap_err().to_string().contains("both base and Big"));
}

#[test]
fn review_manifest_rejects_path_escape_duplicates_and_unsupported_font_bytes() {
    let f = Fixture::new("manifest");
    for name in ["../font.shx", "nested/font.shx", "C:\\font.shx"] {
        f.pack(json!([{"name":name,"sha256":"0".repeat(64)}]));
        assert!(read_pack(&f.0).err().unwrap().to_string().contains("simple .shx"));
    }
    let bytes = synthetic_font();
    std::fs::write(f.0.join("review-duplicate.shx"), &bytes).unwrap();
    f.pack(json!([{"name":"review-duplicate.shx","sha256":digest(&bytes)},{"name":"REVIEW-DUPLICATE.SHX","sha256":digest(&bytes)}]));
    assert!(read_pack(&f.0).err().unwrap().to_string().contains("duplicate"));
    std::fs::write(f.0.join("invalid.shx"), b"not a font").unwrap();
    f.pack(json!([{"name":"invalid.shx","sha256":digest(b"not a font")}]));
    assert!(read_pack(&f.0).err().unwrap().to_string().contains("invalid SHX"));
}

#[test]
fn review_rejects_missing_assets_and_unqualified_font_modes() {
    let f = Fixture::new("missing");
    assert!(verify_fonts(&drawing("missing-profile-font"), Some(&f.0)).is_err());
    let mut d = Drawing::new_metric();
    assert!(verify_fonts(&d, Some(&f.0)).is_ok());
    d.text_styles[0].vertical = true;
    assert!(required_fonts(&d).unwrap_err().to_string().contains("vertical"));
    d.text_styles[0].vertical = false;
    d.text_styles[0].font = "unverified.ttf".into();
    assert!(required_fonts(&d).unwrap_err().to_string().contains("verified legacy SHX"));
}

#[test]
fn review_rejects_profile_drift_and_partial_source_reads_before_plotting() {
    let mut s = Session::empty();
    s.new_drawing(false);
    for request in [json!({"quality":"draft"}), json!({"resolution":123}), json!({"path":"overwrite.dwg"}), json!({"fontDirectory":4})] {
        assert!(run(&mut s, &request).is_err());
    }
    s.doc_mut().unwrap().read_notes.push("A source entity could not be read".into());
    assert!(run(&mut s, &json!({})).unwrap_err().to_string().contains("read warnings"));
}

#[test]
fn review_preset_keeps_print_geometry_equal_at_both_resolutions() {
    let mut preview = preset(Quality::Preview);
    let mut full = preset(Quality::Full);
    assert_eq!(preview["resolution"], 100);
    assert_eq!(full["resolution"], 300);
    preview.as_object_mut().unwrap().remove("resolution");
    full.as_object_mut().unwrap().remove("resolution");
    assert_eq!(preview, full);
    assert_eq!(preview["plotStyleTable"], "monochrome.ctb");
    assert_eq!(preview["margins"], json!([0, 0, 0, 0]));
}

/// A synthetic Big Font with one glyph (code 0xA1A1); no real fonts.
fn synthetic_bigfont() -> Vec<u8> {
    let mut b = b"AutoCAD-86 bigfont 1.0\r\n\x1a".to_vec();
    let header = b.len();
    // 8, entries, ranges; one range; two entries (metadata 0 and the glyph).
    for word in [8u16, 2, 1, 0xA1A1, 0xA1A1] {
        b.extend(word.to_le_bytes());
    }
    let table = b.len() + 2 * 8;
    let meta: &[u8] = &[0, 10, 2, 2, 0];
    let glyph: &[u8] = &[0, 8, 0, 10, 0, 0];
    b.extend(0u16.to_le_bytes());
    b.extend((meta.len() as u16).to_le_bytes());
    b.extend((table as u32).to_le_bytes());
    b.extend(0xA1A1u16.to_le_bytes());
    b.extend((glyph.len() as u16).to_le_bytes());
    b.extend(((table + meta.len()) as u32).to_le_bytes());
    b.extend(meta);
    b.extend(glyph);
    let _ = header;
    assert!(shx::Face::parse(b.clone().into()).is_some_and(|f| f.is_bigfont()), "synthetic big font");
    b
}

/// Version 2: a font the drawing names is drawn by a declared file of the pack, and the result
/// says which; without the substitute the font is still missing.
#[test]
fn review_v2_substitutes_stand_for_required_fonts() {
    let f = Fixture::new("v2-substitute");
    let base = f.file("review-v2-base.shx", &synthetic_font());
    let big = f.file("review-v2-big.shx", &synthetic_bigfont());
    let mut d = drawing("review-v2-alias");
    d.text_styles[0].big_font = "review-v2-bigalias.shx".into();
    // A Big Font is read in the drawing's code page (a GB2312 drawing here).
    d.header.set_str("DWGCODEPAGE", "ANSI_936");
    f.pack_v2(json!([base.clone(), big.clone()]), json!([]));
    assert!(verify_fonts(&d, Some(&f.0)).unwrap_err().to_string().contains("lacks the required base font: review-v2-alias"));
    f.pack_v2(
        json!([base.clone(), big.clone()]),
        json!([{"font":"review-v2-alias","use":"review-v2-base.shx"},{"font":"review-v2-bigalias.shx","use":"review-v2-big.shx"}]),
    );
    let v = verify_fonts(&d, Some(&f.0)).unwrap();
    assert_eq!(v["kind"], "verified_font_pack");
    assert_eq!(
        v["substitutes"],
        json!([{"font":"review-v2-alias","use":"review-v2-base.shx"},{"font":"review-v2-bigalias.shx","use":"review-v2-big.shx"}])
    );
    assert!(shx::find("review-v2-alias", "review-v2-bigalias", "ANSI_936").is_some_and(|font| font.big.is_some()));
}

#[test]
fn review_substitutes_are_checked() {
    let f = Fixture::new("v2-checks");
    let base = f.file("review-v2c-base.shx", &synthetic_font());
    let mut d = drawing("review-v2c-base");
    d.text_styles[0].big_font = "review-v2c-bigalias".into();
    // A Big Font cannot be drawn by a base font file.
    f.pack_v2(json!([base.clone()]), json!([{"font":"review-v2c-bigalias","use":"review-v2c-base.shx"}]));
    assert!(verify_fonts(&d, Some(&f.0)).unwrap_err().to_string().contains("is not a Big font"));
    // A substitute names a file of the pack, and never one of the pack's own fonts.
    f.pack_v2(json!([base.clone()]), json!([{"font":"elsewhere","use":"missing.shx"}]));
    assert!(read_pack(&f.0).err().unwrap().to_string().contains("names no file"));
    f.pack_v2(json!([base.clone()]), json!([{"font":"review-v2c-base","use":"review-v2c-base.shx"}]));
    assert!(read_pack(&f.0).err().unwrap().to_string().contains("repeats a font"));
    // Version 1 has no substitutes and no TrueType files.
    std::fs::write(
        f.0.join("cadcraft-fonts.json"),
        json!({"schema":"cadcraft.font-pack/1","fonts":[base],"substitutes":[{"font":"a","use":"review-v2c-base.shx"}]}).to_string(),
    )
    .unwrap();
    assert!(read_pack(&f.0).err().unwrap().to_string().contains("cadcraft.font-pack/2"));
    f.pack(json!([{"name":"face.ttf","sha256":"0".repeat(64)}]));
    assert!(read_pack(&f.0).err().unwrap().to_string().contains("simple .shx"));
    let junk = f.file("junk.ttf", b"not a font");
    f.pack_v2(json!([junk]), json!([]));
    assert!(read_pack(&f.0).err().unwrap().to_string().contains("invalid TrueType"));
}

/// A TrueType file of a version 2 pack draws a style that names it, or a face it stands for.
/// Uses the bytes of the first of these common installed fonts; skips without any.
#[test]
fn review_v2_truetype_files_draw_their_styles() {
    let found = ["Arial", "Helvetica", "DejaVuSans", "Verdana", "LiberationSans-Regular"].into_iter().find_map(ttf::find);
    let Some(face) = found else { return };
    let f = Fixture::new("v2-truetype");
    let file = f.file("review-v2-face.ttf", &face.bytes);
    f.pack_v2(json!([file]), json!([{"font":"Review V2 Alias","use":"review-v2-face.ttf"}]));
    for name in ["review-v2-face.ttf", "Review V2 Alias"] {
        let d = drawing(name);
        let v = verify_fonts(&d, Some(&f.0)).unwrap();
        assert_eq!(v["fonts"][0]["kind"], "truetype");
        assert!(ttf::find(name).is_some_and(|got| got.bytes.len() == face.bytes.len()), "{name}");
    }
}
