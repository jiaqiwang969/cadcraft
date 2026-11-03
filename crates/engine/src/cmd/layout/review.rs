//! A versioned review plot, shared by the CLI, MCP and the command engine.
//! Fonts are explicit runtime assets. No vendor fonts or drawing-specific answers are embedded.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cadcraft_doc::{Drawing, Space};
use cadcraft_fonts::{BUILTIN_FONT, shx};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::super::{CommandSpec, bad};
use crate::{Result, Session};

const CMD: &str = "reviewplot";
const PROFILE: &str = "mechanical-review-v1";
const MAX_FONT_BYTES: usize = 16 << 20;
const MAX_PACK_BYTES: usize = 64 << 20;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Request {
    path: Option<String>,
    #[serde(default)]
    quality: Quality,
    font_directory: Option<PathBuf>,
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Quality {
    #[default]
    Preview,
    #[serde(rename = "8k")]
    Full,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    fonts: Vec<FontEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FontEntry {
    name: String,
    sha256: String,
}

struct Asset {
    name: String,
    sha256: String,
    bytes: Arc<[u8]>,
    big: bool,
}

pub(super) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new(CMD, "Export Engineering Review", run)
        .params("{path?: PNG (else returns base64 data), quality?: preview|8k, fontDirectory?: directory containing cadcraft-fonts.json (default: Fonts beside the executable)} → versioned profile, verified font hashes and output SHA-256; fails on missing/changed fonts")
        .noundo()]
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn font_key(name: &str) -> Option<String> {
    let name = name.rsplit(['/', '\\']).next()?.trim().to_lowercase();
    let name = name.strip_suffix(".shx").unwrap_or(&name);
    (!name.is_empty() && name != "." && name != ".." && !name.contains('.')).then(|| name.to_owned())
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| bad(CMD, format!("{}: {e}", path.display())))?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes).map_err(|e| bad(CMD, e.to_string()))?;
    if bytes.len() > limit {
        return Err(bad(CMD, format!("{} exceeds the {limit}-byte limit", path.display())));
    }
    Ok(bytes)
}

fn read_pack(directory: &Path) -> Result<(Vec<Asset>, String)> {
    let bytes = read_bounded(&directory.join("cadcraft-fonts.json"), 65536)?;
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|e| bad(CMD, format!("invalid font manifest: {e}")))?;
    if manifest.schema != "cadcraft.font-pack/1" || manifest.fonts.len() > 64 {
        return Err(bad(CMD, "expected cadcraft.font-pack/1 with at most 64 fonts"));
    }
    let mut assets = BTreeMap::new();
    let mut total = 0usize;
    for entry in manifest.fonts {
        if entry.name.contains(['/', '\\', ':']) || !entry.name.to_ascii_lowercase().ends_with(".shx") {
            return Err(bad(CMD, "font entries must be simple .shx filenames"));
        }
        let key = font_key(&entry.name).ok_or_else(|| bad(CMD, "invalid font filename"))?;
        if assets.contains_key(&key) || entry.sha256.len() != 64 || !entry.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(bad(CMD, "duplicate font name or invalid SHA-256"));
        }
        let data = read_bounded(&directory.join(&entry.name), MAX_FONT_BYTES)?;
        total = total.checked_add(data.len()).ok_or_else(|| bad(CMD, "font pack size overflow"))?;
        if total > MAX_PACK_BYTES {
            return Err(bad(CMD, "font pack exceeds 64 MiB"));
        }
        let actual = digest(&data);
        if !actual.eq_ignore_ascii_case(&entry.sha256) {
            return Err(bad(CMD, format!("font hash mismatch: {}", entry.name)));
        }
        let data: Arc<[u8]> = data.into();
        let face = shx::Face::parse(data.clone()).ok_or_else(|| bad(CMD, format!("unsupported or invalid SHX: {}", entry.name)))?;
        assets.insert(key, Asset { name: entry.name, sha256: actual, bytes: data, big: face.is_bigfont() });
    }
    Ok((assets.into_values().collect(), digest(&bytes)))
}

fn required_fonts(drawing: &Drawing) -> Result<BTreeMap<String, bool>> {
    let mut required = BTreeMap::new();
    for style in &drawing.text_styles {
        if style.vertical {
            return Err(bad(CMD, format!("vertical text is not qualified by this profile: {}", style.name)));
        }
        for (name, big) in [(&style.font, false), (&style.big_font, true)] {
            if (big && name.is_empty()) || (!big && name.eq_ignore_ascii_case(BUILTIN_FONT)) {
                continue;
            }
            let key = font_key(name).ok_or_else(|| bad(CMD, format!("this profile requires builtin or verified legacy SHX fonts: {name}")))?;
            if required.insert(key, big).is_some_and(|previous| previous != big) {
                return Err(bad(CMD, "the same font cannot be both base and Big Font"));
            }
        }
    }
    Ok(required)
}

fn verify_fonts(drawing: &Drawing, directory: Option<&Path>) -> Result<Value> {
    let required = required_fonts(drawing)?;
    if required.is_empty() {
        return Ok(json!({"kind":"builtin", "fonts":[]}));
    }
    if std::env::var_os("CADCRAFT_SHX_CODEPAGE").is_some_and(|v| !v.is_empty()) {
        return Err(bad(CMD, "remove CADCRAFT_SHX_CODEPAGE; this profile uses the drawing's recorded code page"));
    }
    let directory = match directory {
        Some(path) => path.to_owned(),
        None => std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("Fonts")))
            .ok_or_else(|| bad(CMD, "provide fontDirectory for the verified font pack"))?,
    };
    let (assets, manifest_hash) = read_pack(&directory)?;
    for (name, big) in &required {
        if !assets.iter().any(|asset| font_key(&asset.name).as_ref() == Some(name) && asset.big == *big) {
            return Err(bad(CMD, format!("font pack lacks the required {} font: {name}", if *big { "Big" } else { "base" })));
        }
    }
    let registrations: Vec<_> = assets.iter().map(|a| (a.name.clone(), a.bytes.clone())).collect();
    if !shx::register_many(&registrations) {
        return Err(bad(CMD, "font pack exceeds the shared font cache bounds"));
    }
    // Check the same resolver the renderer uses; an unknown Big Font code page must not fall back.
    for style in &drawing.text_styles {
        if style.font.eq_ignore_ascii_case(BUILTIN_FONT) && style.big_font.is_empty() {
            continue;
        }
        let font = shx::find(&style.font, &style.big_font, &drawing.header.str("DWGCODEPAGE", "ANSI_1252"));
        if !font.is_some_and(|font| style.big_font.is_empty() || font.big.is_some()) {
            return Err(bad(CMD, format!("font/code-page resolution failed: {}", style.name)));
        }
    }
    Ok(
        json!({"kind":"verified_shx_pack", "manifestSha256":manifest_hash, "fonts":assets.iter().map(|a| json!({"name":a.name,"sha256":a.sha256})).collect::<Vec<_>>()}),
    )
}

fn preset(quality: Quality) -> Value {
    let resolution = match quality {
        Quality::Preview => 100,
        Quality::Full => 300,
    };
    json!({"layout":"Model","output":"png","width":365.76,"height":650.24,"margins":[0,0,0,0],"resolution":resolution,"landscape":true,"fit":true,"center":true,"lineweights":true,"plotArea":"extents","plotStyleTable":"monochrome.ctb","plotTransparency":false})
}

fn run(s: &mut Session, params: &Value) -> Result<Value> {
    let request: Request = serde_json::from_value(params.clone()).map_err(|e| bad(CMD, format!("invalid review profile request: {e}")))?;
    if request.path.as_ref().is_some_and(|p| !Path::new(p).extension().is_some_and(|e| e.eq_ignore_ascii_case("png"))) {
        return Err(bad(CMD, "this versioned profile exports PNG only"));
    }
    if request.path.as_ref().is_some_and(|p| Path::new(p).exists()) {
        return Err(bad(CMD, "the review output already exists; choose a new output path"));
    }
    let drawing = s.doc()?;
    if !drawing.read_notes.is_empty() || drawing.skipped_records.values().any(|count| *count != 0) {
        return Err(bad(CMD, "the input has read warnings or skipped records; review them before using this profile"));
    }
    if drawing.plot_style_tables.iter().any(|table| table.name.eq_ignore_ascii_case("monochrome.ctb")) {
        return Err(bad(CMD, "a drawing-local monochrome.ctb shadows the fixed builtin plot style"));
    }
    let fonts = verify_fonts(drawing, request.font_directory.as_deref())?;
    let preset = preset(request.quality);
    let options = super::super::plotters::plot_options(s, &preset, "plot")?;
    let mut render_drawing = drawing.clone();
    render_drawing.header.set_i64("LWDEFAULT", 70);
    let hook = super::io().and_then(|hooks| hooks.plot).ok_or_else(|| bad(CMD, "plotting is not available in this build"))?;
    let bytes = hook(&render_drawing, &Space::Model, &options).map_err(|e| bad(CMD, e))?;
    let mut result = json!({"profile":PROFILE,"quality":match request.quality { Quality::Preview=>"preview", Quality::Full=>"8k" },"fonts":fonts,"settings":preset,"defaultLineweightMm":0.70,"bytes":bytes.len(),"sha256":digest(&bytes),"sourceDrawingModified":false});
    if let Some(path) = request.path {
        #[cfg(not(target_arch = "wasm32"))]
        crate::atomic_write::write_new(Path::new(&path), &bytes).map_err(|e| bad(CMD, format!("{path}: {e}")))?;
        #[cfg(target_arch = "wasm32")]
        super::super::exchange::deliver_file(CMD, &path, &bytes, super::super::file::deliver())?;
        result["path"] = json!(path);
    } else {
        result["data"] = json!(super::base64_encode(&bytes));
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
