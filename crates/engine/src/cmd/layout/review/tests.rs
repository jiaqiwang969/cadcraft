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
