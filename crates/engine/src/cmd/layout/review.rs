//! A versioned review plot, shared by the CLI, MCP and the command engine.
//! Fonts are explicit runtime assets. No vendor fonts or drawing-specific answers are embedded.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cadcraft_doc::{Drawing, Space};
use cadcraft_fonts::{BUILTIN_FONT, shx, ttf};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::super::{CommandSpec, bad};
use crate::{Result, Session};

const CMD: &str = "reviewplot";
const PROFILE: &str = "mechanical-review-v1";
const MAX_FONT_BYTES: usize = 16 << 20;
/// TrueType files of CJK faces run past the SHX bound (SimSun's collection is 18 MB).
const MAX_OUTLINE_BYTES: usize = 32 << 20;
const MAX_PACK_BYTES: usize = 64 << 20;
const MAX_SUBSTITUTES: usize = 64;

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
    /// Version 2: fonts the drawing names that the pack draws with one of its own files.
    #[serde(default)]
    substitutes: Vec<SubstituteEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubstituteEntry {
    font: String,
    #[serde(rename = "use")]
    with: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FontEntry {
    name: String,
    sha256: String,
}

#[derive(Clone, Copy, PartialEq)]
enum AssetKind {
    Shx {
        big: bool,
    },
    /// A TrueType/OpenType file (version 2 packs).
    Outline,
}

struct Asset {
    name: String,
    sha256: String,
    bytes: Arc<[u8]>,
    kind: AssetKind,
}

/// A verified font pack: its files, and the declared substitutes (required font key → the name
/// as declared and the index of the file that draws it).
struct Pack {
    version: u8,
    assets: Vec<Asset>,
    substitutes: BTreeMap<String, (String, usize)>,
    manifest_hash: String,
}

pub(super) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new(CMD, "Export Engineering Review", run)
        .params("{path?: PNG (else returns base64 data), quality?: preview|8k, fontDirectory?: directory containing cadcraft-fonts.json (default: Fonts beside the executable; schema cadcraft.font-pack/1 lists SHX files, /2 also TrueType files and substitutes {font, use})} → versioned profile, verified font hashes, substitutes used and output SHA-256; fails on missing/changed fonts")
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

/// The key of a TrueType/OpenType file name ("simsun.ttc" → "simsun"); `None` for other names.
fn outline_key(name: &str) -> Option<String> {
    let name = name.rsplit(['/', '\\']).next()?.trim().to_lowercase();
    let stem = [".ttf", ".ttc", ".otf"].iter().find_map(|ext| name.strip_suffix(ext))?;
    font_key(stem)
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

fn read_pack(directory: &Path) -> Result<Pack> {
    let bytes = read_bounded(&directory.join("cadcraft-fonts.json"), 65536)?;
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|e| bad(CMD, format!("invalid font manifest: {e}")))?;
    let version = match manifest.schema.as_str() {
        "cadcraft.font-pack/1" => 1,
        "cadcraft.font-pack/2" => 2,
        _ => 0,
    };
    if version == 0 || manifest.fonts.len() > 64 {
        return Err(bad(CMD, "expected cadcraft.font-pack/1 or /2 with at most 64 fonts"));
    }
    if version == 1 && !manifest.substitutes.is_empty() {
        return Err(bad(CMD, "substitutes need cadcraft.font-pack/2"));
    }
    let mut assets = BTreeMap::new();
    let mut total = 0usize;
    for entry in manifest.fonts {
        let lower = entry.name.to_ascii_lowercase();
        let outline = version >= 2 && [".ttf", ".ttc", ".otf"].iter().any(|ext| lower.ends_with(ext));
        if entry.name.contains(['/', '\\', ':']) || !(lower.ends_with(".shx") || outline) {
            return Err(bad(
                CMD,
                if version == 1 {
                    "font entries must be simple .shx filenames"
                } else {
                    "font entries must be simple .shx, .ttf, .ttc or .otf filenames"
                },
            ));
        }
        let key = if outline { outline_key(&entry.name) } else { font_key(&entry.name) }.ok_or_else(|| bad(CMD, "invalid font filename"))?;
        if assets.contains_key(&key) || entry.sha256.len() != 64 || !entry.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(bad(CMD, "duplicate font name or invalid SHA-256"));
        }
        let data = read_bounded(&directory.join(&entry.name), if outline { MAX_OUTLINE_BYTES } else { MAX_FONT_BYTES })?;
        total = total.checked_add(data.len()).ok_or_else(|| bad(CMD, "font pack size overflow"))?;
        if total > MAX_PACK_BYTES {
            return Err(bad(CMD, "font pack exceeds 64 MiB"));
        }
        let actual = digest(&data);
        if !actual.eq_ignore_ascii_case(&entry.sha256) {
            return Err(bad(CMD, format!("font hash mismatch: {}", entry.name)));
        }
        let kind = if outline {
            if ttf::faces(&Arc::new(data.clone())).is_empty() {
                return Err(bad(CMD, format!("unsupported or invalid TrueType: {}", entry.name)));
            }
            AssetKind::Outline
        } else {
            let face = shx::Face::parse(data.clone().into()).ok_or_else(|| bad(CMD, format!("unsupported or invalid SHX: {}", entry.name)))?;
            AssetKind::Shx { big: face.is_bigfont() }
        };
        assets.insert(key, Asset { name: entry.name, sha256: actual, bytes: data.into(), kind });
    }
    let assets: Vec<Asset> = assets.into_values().collect();
    if manifest.substitutes.len() > MAX_SUBSTITUTES {
        return Err(bad(CMD, format!("at most {MAX_SUBSTITUTES} substitutes")));
    }
    let mut substitutes = BTreeMap::new();
    for sub in manifest.substitutes {
        let key =
            outline_key(&sub.font).or_else(|| font_key(&sub.font)).ok_or_else(|| bad(CMD, format!("invalid substitute font name: {}", sub.font)))?;
        let index = assets
            .iter()
            .position(|a| a.name.eq_ignore_ascii_case(&sub.with))
            .ok_or_else(|| bad(CMD, format!("substitute for {} names no file of the pack: {}", sub.font, sub.with)))?;
        let own = |a: &Asset| match a.kind {
            AssetKind::Outline => outline_key(&a.name),
            AssetKind::Shx { .. } => font_key(&a.name),
        };
        if assets.iter().any(|a| own(a).as_deref() == Some(key.as_str())) || substitutes.insert(key, (sub.font.clone(), index)).is_some() {
            return Err(bad(CMD, format!("substitute for {} repeats a font of the pack", sub.font)));
        }
    }
    Ok(Pack { version, assets, substitutes, manifest_hash: digest(&bytes) })
}

fn required_fonts(drawing: &Drawing) -> Result<BTreeMap<String, bool>> {
    Ok(required_font_names(drawing, false)?.into_iter().map(|(k, (big, _))| (k, big)).collect())
}

/// The fonts a drawing's text styles need: key → (Big Font, name as the style gives it). With
/// `outline`, a style may name a TrueType/OpenType file (version 2 packs).
fn required_font_names(drawing: &Drawing, outline: bool) -> Result<BTreeMap<String, (bool, String)>> {
    let mut required = BTreeMap::new();
    for style in &drawing.text_styles {
        if style.vertical {
            return Err(bad(CMD, format!("vertical text is not qualified by this profile: {}", style.name)));
        }
        for (name, big) in [(&style.font, false), (&style.big_font, true)] {
            if (big && name.is_empty()) || (!big && name.eq_ignore_ascii_case(BUILTIN_FONT)) {
                continue;
            }
            let key = (outline && !big)
                .then(|| outline_key(name))
                .flatten()
                .or_else(|| font_key(name))
                .ok_or_else(|| bad(CMD, format!("this profile requires builtin or verified legacy SHX fonts: {name}")))?;
            if required.insert(key, (big, name.clone())).is_some_and(|(previous, _)| previous != big) {
                return Err(bad(CMD, "the same font cannot be both base and Big Font"));
            }
        }
    }
    Ok(required)
}

fn verify_fonts(drawing: &Drawing, directory: Option<&Path>) -> Result<Value> {
    let required = required_font_names(drawing, true)?;
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
    let pack = read_pack(&directory)?;
    if pack.version == 1 {
        // Version 1 is SHX only, as before: a TrueType file name is not a verified font.
        required_fonts(drawing)?;
    }
    // Each required font: a file of the pack under its own name, else a declared substitute.
    let fits = |a: &Asset, big: bool| match a.kind {
        AssetKind::Shx { big: b } => b == big,
        AssetKind::Outline => !big,
    };
    let mut used = BTreeMap::new();
    for (key, (big, name)) in &required {
        let own = pack.assets.iter().find(|a| {
            let k = match a.kind {
                AssetKind::Outline => outline_key(&a.name),
                AssetKind::Shx { .. } => font_key(&a.name),
            };
            k.as_ref() == Some(key)
        });
        let found = match own {
            Some(a) => fits(a, *big),
            None => match pack.substitutes.get(key) {
                Some((declared, index)) => {
                    let a = pack.assets.get(*index).ok_or_else(|| bad(CMD, "substitute index"))?;
                    if !fits(a, *big) {
                        return Err(bad(
                            CMD,
                            format!(
                                "substitute for the {} font {name} is not a {} font: {}",
                                if *big { "Big" } else { "base" },
                                if *big { "Big" } else { "base" },
                                a.name
                            ),
                        ));
                    }
                    used.insert(key.clone(), json!({"font": declared, "use": a.name}));
                    true
                }
                None => false,
            },
        };
        if !found {
            return Err(bad(CMD, format!("font pack lacks the required {} font: {key}", if *big { "Big" } else { "base" })));
        }
    }
    // SHX files under their names and SHX substitutes under the names they stand for, in one
    // registration; TrueType files under their file names and the names they stand for.
    let mut registrations: Vec<(String, Arc<[u8]>)> =
        pack.assets.iter().filter(|a| matches!(a.kind, AssetKind::Shx { .. })).map(|a| (a.name.clone(), a.bytes.clone())).collect();
    for (key, (_, index)) in &pack.substitutes {
        if let Some(a) = pack.assets.get(*index) {
            match a.kind {
                AssetKind::Shx { .. } => registrations.push((format!("{key}.shx"), a.bytes.clone())),
                AssetKind::Outline => ttf::register(key, a.bytes.to_vec()),
            }
        }
    }
    if !shx::register_many(&registrations) {
        return Err(bad(CMD, "font pack exceeds the shared font cache bounds"));
    }
    for a in pack.assets.iter().filter(|a| a.kind == AssetKind::Outline) {
        ttf::register(&a.name, a.bytes.to_vec());
    }
    // Check the same resolver the renderer uses; an unknown Big Font code page must not fall back.
    for style in &drawing.text_styles {
        if style.font.eq_ignore_ascii_case(BUILTIN_FONT) && style.big_font.is_empty() {
            continue;
        }
        if is_outline(&pack, &style.font) {
            if ttf::find(&style.font).is_none() {
                return Err(bad(CMD, format!("font resolution failed: {}", style.name)));
            }
            continue;
        }
        let font = shx::find(&style.font, &style.big_font, &drawing.header.str("DWGCODEPAGE", "ANSI_1252"));
        if !font.is_some_and(|font| style.big_font.is_empty() || font.big.is_some()) {
            return Err(bad(CMD, format!("font/code-page resolution failed: {}", style.name)));
        }
    }
    let fonts: Vec<Value> = pack
        .assets
        .iter()
        .map(|a| match (pack.version, a.kind) {
            (1, _) => json!({"name":a.name,"sha256":a.sha256}),
            (_, AssetKind::Outline) => json!({"name":a.name,"sha256":a.sha256,"kind":"truetype"}),
            _ => json!({"name":a.name,"sha256":a.sha256,"kind":"shx"}),
        })
        .collect();
    Ok(if pack.version == 1 {
        json!({"kind":"verified_shx_pack", "manifestSha256":pack.manifest_hash, "fonts":fonts})
    } else {
        json!({"kind":"verified_font_pack", "manifestSha256":pack.manifest_hash, "fonts":fonts, "substitutes":used.into_values().collect::<Vec<_>>()})
    })
}

/// Whether a style's base font is drawn by a TrueType file of the pack (its own or a substitute).
fn is_outline(pack: &Pack, font: &str) -> bool {
    let key = outline_key(font).or_else(|| font_key(font)).unwrap_or_default();
    let own = pack.assets.iter().find(|a| match a.kind {
        AssetKind::Outline => outline_key(&a.name).as_deref() == Some(key.as_str()),
        AssetKind::Shx { .. } => font_key(&a.name).as_deref() == Some(key.as_str()),
    });
    match own {
        Some(a) => a.kind == AssetKind::Outline,
        None => pack.substitutes.get(&key).and_then(|(_, i)| pack.assets.get(*i)).is_some_and(|a| a.kind == AssetKind::Outline),
    }
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
