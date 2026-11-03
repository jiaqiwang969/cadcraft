//! Explicit user font directories, cached with bounded memory. No vendor installation scan.
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use super::{Face, Font, Kind, MAX_FILE_BYTES};

#[derive(Default)]
struct Cache {
    faces: HashMap<String, Option<Arc<Face>>>,
    bytes: usize,
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

fn key(name: &str) -> String {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name).trim().to_lowercase();
    file.strip_suffix(".shx").unwrap_or(&file).to_owned()
}

/// Register user-supplied font bytes (also usable in web builds). Invalid data is rejected.
pub fn register(name: &str, bytes: Arc<[u8]>) -> bool {
    register_many(&[(name.to_owned(), bytes)])
}

/// Validate and install a whole explicit font pack together. Replacing an existing face does
/// not consume another cache entry or count its old bytes a second time.
pub fn register_many(fonts: &[(String, Arc<[u8]>)]) -> bool {
    if fonts.len() > 256 {
        return false;
    }
    let Some(input_bytes) = fonts.iter().try_fold(0usize, |total, (_, bytes)| total.checked_add(bytes.len())) else { return false };
    if input_bytes > 64 << 20 {
        return false;
    }
    let mut faces = HashMap::new();
    for (name, bytes) in fonts {
        let name = key(name);
        let Some(face) = Face::parse(bytes.clone()) else { return false };
        if name.is_empty() || name == "." || name == ".." || faces.insert(name, face).is_some() {
            return false;
        }
    }
    let mut c = cache().lock().unwrap_or_else(PoisonError::into_inner);
    let added = faces.keys().filter(|name| !c.faces.contains_key(*name)).count();
    let removed_bytes: usize = faces.keys().filter_map(|name| c.faces.get(name).and_then(Option::as_ref)).map(|f| f.bytes.len()).sum();
    let Some(added_bytes) = faces.values().try_fold(0usize, |total, f| total.checked_add(f.bytes.len())) else { return false };
    let Some(bytes) = c.bytes.saturating_sub(removed_bytes).checked_add(added_bytes) else { return false };
    if c.faces.len().saturating_add(added) > 256 || bytes > 64 << 20 {
        return false;
    }
    c.bytes = bytes;
    for (name, face) in faces {
        c.faces.insert(name, Some(face));
    }
    true
}

fn load(name: &str) -> Option<Arc<Face>> {
    if name.is_empty() || name.eq_ignore_ascii_case(crate::BUILTIN_FONT) {
        return None;
    }
    if Path::new(name).extension().is_some_and(|e| !e.eq_ignore_ascii_case("shx")) {
        return None;
    }
    let name = key(name);
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    let mut c = cache().lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(face) = c.faces.get(&name) {
        return face.clone();
    }
    if c.faces.len() >= 256 || c.bytes >= 64 << 20 {
        return None;
    }
    let mut found = None;
    // Set before starting the process. Use the OS path-list separator (: on Unix, ; on
    // Windows). Restrict lookup to these directories, even for paths stored in a drawing.
    if let Some(paths) = std::env::var_os("CADCRAFT_SHX_PATH") {
        'dirs: for dir in std::env::split_paths(&paths).take(32) {
            let Ok(entries) = std::fs::read_dir(dir) else { continue };
            for entry in entries.take(4096).flatten() {
                let path = entry.path();
                if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("shx"))
                    || !path.file_stem().is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(&name))
                {
                    continue;
                }
                let Ok(meta) = entry.metadata() else { continue };
                if !meta.is_file() || meta.len() > MAX_FILE_BYTES as u64 {
                    continue;
                }
                // Take bounds the allocation even if the file grows after metadata().
                use std::io::Read;
                let Ok(file) = std::fs::File::open(path) else { continue };
                let mut bytes = Vec::new();
                if file.take(MAX_FILE_BYTES as u64 + 1).read_to_end(&mut bytes).is_err() || bytes.len() > MAX_FILE_BYTES {
                    continue;
                }
                if let Some(face) = Face::parse(bytes.into())
                    && c.bytes + face.bytes.len() <= 64 << 20
                {
                    c.bytes += face.bytes.len();
                    found = Some(face);
                    break 'dirs;
                }
            }
        }
    }
    c.faces.insert(name, found.clone());
    found
}

/// Resolve a legacy base + Big Font pair using the drawing's DWGCODEPAGE. The host can
/// override the encoding with CADCRAFT_SHX_CODEPAGE (e.g. gbk or shift_jis). Unknown encodings
/// do not guess a Big Font mapping; missing characters retain the normal fallback.
pub fn find(name: &str, big_name: &str, code_page: &str) -> Option<Font> {
    let base = load(name).filter(|f| f.kind == Kind::Shapes)?;
    let override_page = std::env::var("CADCRAFT_SHX_CODEPAGE").ok();
    let label = override_page.as_deref().unwrap_or(code_page).trim();
    let encoding = match label.to_ascii_uppercase().as_str() {
        "ANSI_936" => Some(encoding_rs::GBK),
        "ANSI_932" => Some(encoding_rs::SHIFT_JIS),
        "ANSI_949" => Some(encoding_rs::EUC_KR),
        "ANSI_950" => Some(encoding_rs::BIG5),
        _ => encoding_rs::Encoding::for_label(label.as_bytes()),
    };
    let big = encoding.and_then(|_| load(big_name)).filter(|f| f.kind == Kind::Bigfont);
    Some(Font::new(base, big, encoding.unwrap_or(encoding_rs::WINDOWS_1252)))
}
