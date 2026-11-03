use super::*;

#[test]
fn repeated_pack_registration_does_not_exhaust_cache_and_invalid_batch_is_atomic() {
    let mut padded = shapes(&[(65, &[8, 0, 10, 0])]).bytes.to_vec();
    padded.resize(1 << 20, 0);
    let good: Arc<[u8]> = padded.into();
    let entries = [("review-repeat-base".to_owned(), good.clone())];
    for _ in 0..300 {
        assert!(register_many(&entries));
    }
    assert!(find("review-repeat-base", "", "ANSI_1252").is_some());
    let bad = [("review-batch-only".to_owned(), good), ("review-batch-bad".to_owned(), Arc::from(&b"invalid"[..]))];
    assert!(!register_many(&bad));
    assert!(find("review-batch-only", "", "ANSI_1252").is_none());
}

// Original synthetic instruction streams; no vendor fonts or extracted glyphs.
fn shapes(entries: &[(u16, &[u8])]) -> Arc<Face> {
    let mut b = b"AutoCAD-86 shapes 1.1\r\n\x1a".to_vec();
    b.extend(0u16.to_le_bytes());
    b.extend(258u16.to_le_bytes());
    b.extend(((entries.len() + 1) as u16).to_le_bytes());
    let mut all = vec![(0, vec![0, 10, 2, 2, 0])];
    all.extend(entries.iter().map(|(c, p)| {
        let mut bytes = vec![0];
        bytes.extend_from_slice(p);
        (*c, bytes)
    }));
    for (c, data) in &all {
        b.extend(c.to_le_bytes());
        b.extend((data.len() as u16).to_le_bytes());
    }
    for (_, data) in all {
        b.extend(data);
    }
    Face::parse(b.into()).expect("synthetic shapes font")
}

fn bigfont(entries: &[(u16, &[u8])]) -> Arc<Face> {
    let mut b = b"AutoCAD-86 bigfont 1.0\r\n\x1a".to_vec();
    b.extend(8u16.to_le_bytes());
    b.extend(((entries.len() + 2) as u16).to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(0xa1u16.to_le_bytes());
    b.extend(0xfeu16.to_le_bytes());
    let mut all = vec![(0, vec![0, 10, 2, 2, 0])];
    all.extend(entries.iter().map(|(c, p)| {
        let mut data = vec![0];
        data.extend_from_slice(p);
        (*c, data)
    }));
    let mut offset = b.len() + (entries.len() + 2) * 8;
    for (c, data) in &all {
        b.extend(c.to_le_bytes());
        b.extend((data.len() as u16).to_le_bytes());
        b.extend((offset as u32).to_le_bytes());
        offset += data.len();
    }
    b.extend([0; 8]); // Vacant hash slot must not overwrite shape zero.
    for (_, data) in all {
        b.extend(data);
    }
    Face::parse(b.into()).expect("synthetic bigfont")
}

fn font() -> Font {
    let base =
        shapes(&[(65, &[8, 0, 10, 2, 8, 8, 246, 0]), (32, &[2, 8, 5, 0, 0]), (256, &[8, 1, 1, 0]), (257, &[8, 2, 2, 0]), (258, &[8, 3, 3, 0])]);
    let big = bigfont(&[
        (0xafa1, &[5, 2, 8, 0, 5, 3, 2, 0]), // Save, raise, halve.
        (0xafa2, &[4, 2, 6, 0]),             // Restore scale AND baseline position.
        (0xb9a5, &[8, 10, 10, 2, 8, 0, 246, 0]),
    ]);
    Font::new(base, Some(big), encoding_rs::GBK)
}

#[test]
fn private_use_controls_keep_position_and_scale_across_glyphs() {
    let f = font();
    let sh = f.shape("A\u{e1d6}A\u{e1d7}A", 10.0, 1.0, 0.0);
    assert_eq!(sh.strokes.len(), 3);
    assert_eq!(sh.strokes[0], [Vec2::ZERO, Vec2::new(0.0, 10.0)]);
    assert_eq!(sh.strokes[1], [Vec2::new(8.0, 5.0), Vec2::new(8.0, 10.0)]);
    assert_eq!(sh.strokes[2], [Vec2::new(8.0, 0.0), Vec2::new(8.0, 10.0)]);
    assert_eq!(sh.width, 8.0);
    assert_eq!(f.advance("A\u{e1d6}A\u{e1d7}A", 10.0, 1.0), 16.0);
    assert_eq!(f.width("A\u{e1d6}A\u{e1d7}A", 10.0, 1.0), sh.width);
}

#[test]
fn mtext_does_not_split_control_sequence_at_padding_space() {
    let text = "A\u{e1d6} A\u{e1d7}A";
    let f = font();
    let expected = f.shape(text, 10.0, 1.0, 0.0);
    let mut params = crate::MTextParams::new(Vec2::new(0.0, 10.0), 10.0);
    params.font = crate::TextFont::Shx(f);
    let laid = crate::layout_mtext_with(text, &params);
    assert_eq!(laid.strokes, expected.strokes);
    assert!((laid.bounds.width() - expected.width).abs() < 1e-9);
}

#[test]
fn mtext_word_spacing_and_centering_use_ink_and_advance_separately() {
    let f = font();
    let text = "A A";
    let expected = f.shape(text, 10.0, 1.0, 0.0);
    let mut params = crate::MTextParams::new(Vec2::new(0.0, 10.0), 10.0);
    params.font = crate::TextFont::Shx(f);
    let laid = crate::layout_mtext_with(text, &params);
    assert_eq!(laid.strokes, expected.strokes);
    assert!((laid.bounds.width() - 13.0).abs() < 1e-9);
    params.attach = 2;
    let centered = crate::layout_mtext_with(text, &params);
    assert!((centered.bounds.center().x).abs() < 1e-9);
    assert_eq!(centered.strokes[0][0].x, -6.5);
}

#[test]
fn font_controlled_superscripts_expand_the_mtext_line() {
    let original = font();
    let big = bigfont(&[(0xafa1, &[5, 2, 8, 0, 20, 3, 2, 0]), (0xafa2, &[4, 2, 6, 0])]);
    let font = Font::new(original.base, Some(big), encoding_rs::GBK);
    let mut params = crate::MTextParams::new(Vec2::ZERO, 10.0);
    params.font = crate::TextFont::Shx(font);
    let l = crate::layout_mtext_with("A\u{e1d6}A\u{e1d7}A", &params);
    assert!((l.bounds.height() - 25.0).abs() < 1e-9);
    let ink = cadcraft_geom::Bounds2::from_points(l.strokes.iter().flatten().copied());
    assert!(ink.max.y.abs() < 1e-9);
    assert!((ink.min.y + 25.0).abs() < 1e-9);
}

#[test]
fn big_font_maps_cjk_and_does_not_guess_another_codepage() {
    let f = font();
    let sh = f.shape("攻", 10.0, 1.0, 0.0);
    assert_eq!(sh.strokes, vec![vec![Vec2::ZERO, Vec2::new(10.0, 10.0)]]);
    let wrong = Font::new(f.base, f.big, encoding_rs::SHIFT_JIS);
    assert!(wrong.glyph('攻').is_none());
}

#[test]
fn percent_drafting_symbols_use_legacy_shape_numbers() {
    let sh = font().shape("%%d%%p%%c", 10.0, 1.0, 0.0);
    assert_eq!(sh.strokes.len(), 3);
    assert!((sh.width - 6.0).abs() < 1e-9);
}

#[test]
fn horizontal_skip_consumes_variable_length_instruction_without_scaling() {
    let base = shapes(&[(65, &[14, 3, 2, 14, 9, 9, 9, 0, 0, 8, 0, 10, 0])]);
    let f = Font::new(base, None, encoding_rs::GBK);
    let sh = f.shape("AA", 10.0, 1.0, 0.0);
    assert_eq!(sh.strokes[0][1], Vec2::new(0.0, 10.0));
    assert_eq!(sh.strokes[1][1], Vec2::new(0.0, 20.0));
}

#[test]
fn cyclic_and_invalid_programs_are_bounded_and_transactional() {
    for p in [&[7, 65, 0][..], &[3, 0, 0], &[6, 0], &[14, 14, 14], &[8, 1]] {
        let base = shapes(&[(65, p), (66, &[8, 0, 10, 2, 8, 8, 246, 0])]);
        let f = Font::new(base, None, encoding_rs::GBK);
        let sh = f.shape("AB", 10.0, 1.0, 0.0);
        assert!(sh.strokes.iter().flatten().all(|p| p.x.is_finite() && p.y.is_finite()));
        assert_eq!(sh.strokes.last().unwrap()[1].y, 10.0);
    }
}

#[test]
fn truncated_containers_and_hostile_offsets_are_rejected() {
    let f = font();
    for face in [&f.base, f.big.as_ref().unwrap()] {
        for n in 0..face.bytes.len() {
            assert!(Face::parse(face.bytes[..n].into()).is_none(), "truncated at {n}");
        }
    }
    let mut bytes = f.big.unwrap().bytes.to_vec();
    let offset = b"AutoCAD-86 bigfont 1.0\r\n\x1a".len() + 6 + 4 + 4;
    bytes[offset..offset + 4].fill(255);
    assert!(Face::parse(bytes.into()).is_none());
}

#[test]
fn width_factor_and_rotation_preserve_shape_geometry() {
    let font = crate::TextFont::Shx(font());
    let p = crate::TextParams {
        insert: Vec2::new(20.0, 30.0),
        height: 10.0,
        width_factor: 2.0,
        rotation: std::f64::consts::FRAC_PI_2,
        ..crate::TextParams::new(Vec2::ZERO, 10.0)
    };
    let (sh, _) = crate::place(&font, "A", &p);
    assert!((sh.strokes[0][1] - Vec2::new(10.0, 30.0)).len() < 1e-9);
    assert_eq!(sh.width, 0.0);
}
