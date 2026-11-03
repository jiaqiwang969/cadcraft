//! Runtime SHX stroke fonts. No font data is distributed with CADCraft.
//!
//! The vector language follows Autodesk's public Shape Descriptions / Special Codes
//! Reference. Scale and the position stack belong to a text run, not to an individual
//! glyph: some drafting fonts use otherwise invisible glyphs to lay out tolerances.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

use cadcraft_geom::Vec2;

use crate::Shaped;

mod lookup;
mod machine;
pub use lookup::{find, register};
pub(crate) use machine::Cursor;

const MAX_FILE_BYTES: usize = 16 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Shapes,
    Bigfont,
}

/// A validated directory of glyph programs. Programs are interpreted on demand.
#[derive(Debug)]
pub struct Face {
    bytes: Arc<[u8]>,
    glyphs: BTreeMap<u16, Range<usize>>,
    kind: Kind,
    above: f64,
    below: f64,
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn byte(&mut self) -> Option<u8> {
        let b = *self.bytes.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }
    fn word(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes([self.byte()?, self.byte()?]))
    }
    fn dword(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes([self.byte()?, self.byte()?, self.byte()?, self.byte()?]))
    }
}

impl Face {
    /// Read the legacy shapes / Big Font containers. Unsupported containers and extended
    /// Big Fonts return None so the caller can use its normal missing-font fallback.
    pub fn parse(bytes: Arc<[u8]>) -> Option<Arc<Self>> {
        if bytes.len() > MAX_FILE_BYTES {
            return None;
        }
        let kind = if bytes.starts_with(b"AutoCAD-86 shapes 1.0\r\n\x1a") || bytes.starts_with(b"AutoCAD-86 shapes 1.1\r\n\x1a") {
            Kind::Shapes
        } else if bytes.starts_with(b"AutoCAD-86 bigfont 1.0\r\n\x1a") {
            Kind::Bigfont
        } else {
            return None;
        };
        let mut r = Reader { pos: bytes.iter().position(|b| *b == 26)? + 1, bytes: &bytes };
        let mut glyphs = BTreeMap::new();
        match kind {
            Kind::Shapes => {
                r.word()?; // First and last shape numbers; the directory is authoritative.
                r.word()?;
                let count = usize::from(r.word()?);
                let mut offset = r.pos.checked_add(count.checked_mul(4)?)?;
                bytes.get(..offset)?;
                for _ in 0..count {
                    let code = r.word()?;
                    let len = usize::from(r.word()?);
                    let end = offset.checked_add(len)?;
                    bytes.get(offset..end)?;
                    if len > 0 && glyphs.insert(code, offset..end).is_some() {
                        return None;
                    }
                    offset = end;
                }
            }
            Kind::Bigfont => {
                if r.word()? != 8 {
                    return None;
                }
                let count = usize::from(r.word()?);
                let ranges = usize::from(r.word()?);
                r.pos = r.pos.checked_add(ranges.checked_mul(4)?)?;
                let table_end = r.pos.checked_add(count.checked_mul(8)?)?;
                bytes.get(..table_end)?;
                for _ in 0..count {
                    let code = r.word()?;
                    let len = usize::from(r.word()?);
                    let offset = usize::try_from(r.dword()?).ok()?;
                    // Big Font hash tables contain vacant slots, including code zero.
                    if len == 0 {
                        continue;
                    }
                    if offset < table_end {
                        return None;
                    }
                    let end = offset.checked_add(len)?;
                    bytes.get(offset..end)?;
                    if glyphs.insert(code, offset..end).is_some() {
                        return None;
                    }
                }
            }
        }
        let metadata = bytes.get(glyphs.get(&0)?.clone())?;
        let start = metadata.iter().position(|b| *b == 0)? + 1;
        let above = f64::from(*metadata.get(start)?);
        let below = f64::from(*metadata.get(start + 1)?);
        if above == 0.0 || (kind == Kind::Bigfont && below == 0.0) {
            return None;
        }
        // Every record has a NUL-terminated name and a nonempty program.
        for range in glyphs.values_mut() {
            let data = bytes.get(range.clone())?;
            range.start += data.iter().position(|b| *b == 0)? + 1;
            if range.start >= range.end {
                return None;
            }
        }
        Some(Arc::new(Self { bytes, glyphs, kind, above, below }))
    }

    fn program(&self, code: u16) -> Option<&[u8]> {
        self.bytes.get(self.glyphs.get(&code)?.clone())
    }
}

/// A base font paired with an optional legacy Big Font and its explicitly selected encoding.
/// A Big Font contains byte codes, not Unicode; guessing the code page can draw the wrong
/// character. Hosts should use the drawing's code page when constructing this pair.
#[derive(Clone, Debug)]
pub struct Font {
    pub base: Arc<Face>,
    pub big: Option<Arc<Face>>,
    encoding: &'static encoding_rs::Encoding,
}

impl Font {
    pub fn new(base: Arc<Face>, big: Option<Arc<Face>>, encoding: &'static encoding_rs::Encoding) -> Self {
        Self { base, big, encoding }
    }

    fn glyph(&self, c: char) -> Option<(&Face, u16)> {
        // Big Font codes are the original two-byte values. Unicode private-use characters
        // must round trip through that encoding just like ordinary CJK characters.
        if let Some(big) = &self.big {
            let mut utf8 = [0; 4];
            let (bytes, _, errors) = self.encoding.encode(c.encode_utf8(&mut utf8));
            if !errors && let [hi, lo] = bytes.as_ref() {
                let code = u16::from_be_bytes([*hi, *lo]);
                if big.program(code).is_some() {
                    return Some((big, code));
                }
            }
        }
        // The three %% drafting symbols occupy 256..258 in legacy shape fonts.
        let code = match c {
            '°' => 256,
            '±' => 257,
            '⌀' | 'Ø' => 258,
            _ => u16::try_from(u32::from(c)).ok()?,
        };
        self.base.program(code).map(|_| (self.base.as_ref(), code))
    }

    pub fn descent_ratio(&self) -> f64 {
        self.base.below / self.base.above
    }

    /// True while a control sequence has changed scale or saved a position. MTEXT must not
    /// split such a sequence at its padding spaces and reset the glyph interpreter.
    pub(crate) fn track(&self, cursor: &mut Cursor, c: char) -> bool {
        if cursor.tracked_steps >= 2_000_000 {
            return cursor.active();
        }
        if let Some((face, code)) = self.glyph(c) {
            let mut machine = machine::Machine::new(cursor.clone(), false);
            let ok = machine.glyph(face, code, 0).is_some();
            let steps = cursor.tracked_steps.saturating_add(machine.steps);
            if ok {
                *cursor = machine.cursor;
            }
            cursor.tracked_steps = steps;
        }
        cursor.active()
    }

    pub fn shape(&self, text: &str, height: f64, width: f64, oblique: f64) -> Shaped {
        self.run(text, height, width, oblique, true).0
    }

    pub fn width(&self, text: &str, height: f64, width: f64) -> f64 {
        self.run(text, height, width, 0.0, false).0.width
    }

    /// Baseline pen position after a run, including its final letter spacing. This is used
    /// when concatenating MTEXT runs, whereas justification uses the visible right edge.
    pub fn advance(&self, text: &str, height: f64, width: f64) -> f64 {
        self.run(text, height, width, 0.0, false).1
    }

    /// Vertical extent including the baseline and font-controlled superscripts.
    pub fn height(&self, text: &str, height: f64) -> f64 {
        self.run(text, height, 1.0, 0.0, false).2
    }

    fn run(&self, text: &str, height: f64, width: f64, oblique: f64, draw: bool) -> (Shaped, f64, f64) {
        let h = if height.is_finite() && height > 0.0 { height.min(1e12) } else { 1.0 };
        let wf = if width.is_finite() && width.abs() > 1e-6 { width.clamp(-1e6, 1e6) } else { 1.0 };
        let shear = if oblique.is_finite() { oblique.clamp(-1.48, 1.48).tan() } else { 0.0 };
        let mut machine = machine::Machine::new(Cursor::default(), draw);
        let mut spans = Vec::new();
        let mut fallback = Shaped::default();
        let end = text.char_indices().nth(200_000).map_or(text.len(), |(i, _)| i);
        let bounded = text.get(..end).unwrap_or("");
        for (c, under, over) in crate::decode_controls(bounded) {
            let before = machine.cursor.clone();
            let start = before.at.x;
            let points = machine.points;
            let strokes = machine.strokes.len();
            let ink_right = machine.ink_right;
            let ink_y = (machine.ink_bottom, machine.ink_top);
            let ok = self.glyph(c).is_some_and(|(face, code)| machine.glyph(face, code, 0).is_some());
            if !ok {
                // Malformed glyphs must not leave a half-drawn glyph or corrupt later text.
                machine.cursor = before;
                machine.points = points;
                machine.strokes.truncate(strokes);
                machine.ink_right = ink_right;
                (machine.ink_bottom, machine.ink_top) = ink_y;
                let size = machine.cursor.scale;
                let mut sh = crate::shape_stroke(&c.to_string(), size, 1.0, 0.0);
                let count = sh.strokes.iter().chain(sh.glyphs.iter().flatten()).map(Vec::len).sum();
                if !machine.charge_points(count) {
                    break;
                }
                sh.map(|p| p + machine.cursor.at);
                for point in sh.strokes.iter().flatten().chain(sh.glyphs.iter().flatten().flatten()) {
                    machine.ink_bottom = machine.ink_bottom.min(point.y);
                    machine.ink_top = machine.ink_top.max(point.y);
                }
                machine.cursor.at.x += sh.width;
                machine.ink_right = Some(machine.ink_right.unwrap_or(f64::NEG_INFINITY).max(machine.cursor.at.x));
                if draw {
                    fallback.extend(sh);
                }
            }
            if draw {
                spans.push((start, machine.cursor.at.x, under, over));
            }
            if machine.exhausted() {
                break;
            }
        }
        let mut out = fallback;
        out.strokes.extend(machine.strokes);
        let extent = machine.ink_right.unwrap_or(machine.cursor.at.x);
        out.width = extent.max(0.0) * h * wf;
        if draw {
            out.strokes.extend(crate::decorations(&spans, 1.0));
        }
        out.map(|p| Vec2::new((p.x * wf + p.y * shear) * h, p.y * h));
        (out, machine.cursor.at.x.max(0.0) * h * wf, (machine.ink_top.max(1.0) - machine.ink_bottom) * h)
    }
}

#[cfg(test)]
mod tests;
