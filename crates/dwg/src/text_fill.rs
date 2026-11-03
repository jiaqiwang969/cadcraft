//! The private R2000 transport keeps newer style fields in CADCRAFT DIMVARS xdata.
//! Restore text backgrounds into the native model before writing R2007+ DWG.

use acadrust::{
    CadDocument, DxfVersion,
    xdata::{ExtendedData, XDataValue},
};

/// Mirrors the io reader's item bound; malformed or ambiguous metadata stays opaque.
const MAX_ITEMS: usize = 4096;

fn background(data: &ExtendedData) -> Option<(Option<i16>, Option<i16>)> {
    let mut records = data.records().iter().filter(|r| r.application_name == "CADCRAFT");
    let record = records.next()?;
    if records.next().is_some() || record.values.len() > MAX_ITEMS {
        return None;
    }
    let mut markers = record.values.iter().enumerate().filter_map(|(i, v)| matches!(v, XDataValue::String(s) if s == "DIMVARS").then_some(i));
    let start = markers.next()?;
    if markers.next().is_some() {
        return None;
    }
    let [_, XDataValue::ControlString(open), tail @ ..] = record.values.get(start..)? else { return None };
    if open != "{" {
        return None;
    }
    let end = tail.iter().position(|v| matches!(v, XDataValue::ControlString(_)))?;
    if !matches!(tail.get(end), Some(XDataValue::ControlString(s)) if s == "}") {
        return None;
    }
    let pairs = tail.get(..end)?.chunks_exact(2);
    if !pairs.remainder().is_empty() {
        return None;
    }
    let (mut fill, mut color) = (None, None);
    for pair in pairs {
        let [XDataValue::Integer16(code), value] = pair else { return None };
        match code {
            69 => {
                let XDataValue::Integer16(v @ 0..=2) = value else { return None };
                if fill.replace(*v).is_some() {
                    return None;
                }
            }
            70 => {
                let XDataValue::Integer16(v @ 0..=256) = value else { return None };
                if color.replace(*v).is_some() {
                    return None;
                }
            }
            _ => {}
        }
    }
    Some((fill, color))
}

pub(crate) fn promote(doc: &mut CadDocument, input_version: DxfVersion) {
    // Newer DXF already supplies native fields; stale xdata must not override them.
    // Old DWG versions have no native text-background field, so keep their xdata only.
    if input_version >= DxfVersion::AC1021 || doc.version < DxfVersion::AC1021 {
        return;
    }
    for style in doc.dim_styles.iter_mut() {
        let Some((fill, color)) = doc.object_xdata.get(&style.handle).and_then(background) else { continue };
        if let Some(fill) = fill {
            style.dimtfill = fill;
            if style.handle == doc.header.current_dimstyle_handle {
                doc.header.dim_text_fill = fill;
            }
        }
        if let Some(color) = color {
            style.dimtfillclr = color;
            style.dimtfillclr_true_color = None;
            if style.handle == doc.header.current_dimstyle_handle {
                doc.header.dim_text_fill_color = acadrust::types::Color::from_index(color);
            }
        }
    }
}
