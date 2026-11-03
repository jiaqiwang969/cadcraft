//! Original synthetic DXF inputs; inspect DWG native fields, not CADCraft's xdata reader.
#![cfg(not(any(target_arch = "wasm32", all(target_os = "freebsd", target_arch = "aarch64"))))]

use std::io::Cursor;

fn drawing(version: &str, native: &str, app: &str, payload: &str) -> String {
    format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\n{version}\n9\n$DIMSTYLE\n2\nMasked\n0\nENDSEC\n\
         0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nAPPID\n70\n1\n0\nAPPID\n5\n110\n100\nAcDbSymbolTableRecord\n100\nAcDbRegAppTableRecord\n2\n{app}\n70\n0\n0\nENDTAB\n\
         0\nTABLE\n2\nDIMSTYLE\n70\n2\n0\nDIMSTYLE\n105\n120\n100\nAcDbSymbolTableRecord\n100\nAcDbDimStyleTableRecord\n2\nMasked\n70\n0\n{native}\
         1001\n{app}\n1000\nDIMVARS\n1002\n{{\n{payload}\n\
         0\nDIMSTYLE\n105\n121\n100\nAcDbSymbolTableRecord\n100\nAcDbDimStyleTableRecord\n2\nPlain\n70\n0\n0\nENDTAB\n0\nENDSEC\n0\nEOF\n"
    )
}

fn converted(input: &str, output_version: &str) -> acadrust::CadDocument {
    let dwg = cadcraft_dwg::dxf_to_dwg_as(input.as_bytes(), output_version).unwrap();
    acadrust::DwgReader::from_stream(Cursor::new(dwg)).read().unwrap()
}

#[test]
fn text_background_reaches_native_dwg_style_for_supported_versions() {
    for version in ["AC1021", "AC1024", "AC1027", "AC1032"] {
        for (fill, color) in [(0, 0), (1, 0), (2, 3), (2, 256)] {
            let payload = format!("1070\n49\n1040\n3.5\n1070\n69\n1070\n{fill}\n1070\n70\n1070\n{color}\n1002\n}}");
            let doc = converted(&drawing("AC1015", "", "CADCRAFT", &payload), version);
            let style = doc.dim_styles.get("Masked").unwrap();
            assert_eq!(style.dimtfill, fill, "{version} background mode");
            assert_eq!(style.dimtfillclr, color, "{version} background ACI");
            assert_eq!(doc.header.dim_text_fill, fill, "current style and header agree");
            assert_eq!(doc.dim_styles.get("Plain").unwrap().dimtfill, 0, "another style is untouched");
        }
    }
}

#[test]
fn old_formats_keep_extension_without_claiming_native_background_support() {
    let input = drawing("AC1015", "", "CADCRAFT", "1070\n69\n1070\n1\n1002\n}");
    for version in ["AC1015", "AC1018"] {
        let doc = converted(&input, version);
        let style = doc.dim_styles.get("Masked").unwrap();
        assert_eq!(style.dimtfill, 0);
        assert!(doc.object_xdata(style.handle).is_some(), "original extension must survive");
    }
}

#[test]
fn newer_dxf_native_value_wins_over_stale_extension() {
    let input = drawing("AC1032", "69\n2\n70\n4\n", "CADCRAFT", "1070\n69\n1070\n1\n1002\n}");
    let doc = converted(&input, "AC1032");
    let style = doc.dim_styles.get("Masked").unwrap();
    assert_eq!((style.dimtfill, style.dimtfillclr), (2, 4));
}

#[test]
fn unrelated_or_malformed_extensions_do_not_enable_native_background() {
    let cases = [
        ("OTHER", "1070\n69\n1070\n1\n1002\n}".to_owned()),
        ("CADCRAFT", "1070\n69\n1070\n-1\n1002\n}".to_owned()),
        ("CADCRAFT", "1070\n69\n1070\n3\n1002\n}".to_owned()),
        ("CADCRAFT", "1070\n69\n1040\n1\n1002\n}".to_owned()),
        ("CADCRAFT", "1070\n69\n1070\n1\n1070\n69\n1070\n2\n1002\n}".to_owned()),
        ("CADCRAFT", "1070\n69\n1070\n1".to_owned()),
        ("CADCRAFT", "1070\n69\n1070\n1\n1070\n70\n1070\n-1\n1002\n}".to_owned()),
        ("CADCRAFT", "1002\n{\n1070\n69\n1070\n1\n1002\n}\n1002\n}".to_owned()),
        ("CADCRAFT", format!("{}1070\n69\n1070\n1\n1002\n}}", "1070\n49\n1040\n1\n".repeat(2048))),
    ];
    for (app, payload) in cases {
        let doc = converted(&drawing("AC1015", "", app, &payload), "AC1032");
        assert_eq!(doc.dim_styles.get("Masked").unwrap().dimtfill, 0, "{app}: {payload:.160}");
    }
}
