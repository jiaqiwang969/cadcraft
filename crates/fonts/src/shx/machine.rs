use cadcraft_geom::Vec2;

use super::{Face, Kind, Reader};

const MAX_STEPS: usize = 2_000_000;
const MAX_POINTS: usize = 1_000_000;
const MAX_DEPTH: usize = 16;

#[derive(Clone, Debug)]
pub(crate) struct Cursor {
    pub at: Vec2,
    pub scale: f64,
    pub tracked_steps: usize,
    stack: Vec<Vec2>,
    pen: bool,
}

impl Default for Cursor {
    fn default() -> Self {
        Self { at: Vec2::ZERO, scale: 1.0, tracked_steps: 0, stack: Vec::new(), pen: true }
    }
}

impl Cursor {
    pub fn active(&self) -> bool {
        !self.stack.is_empty() || (self.scale - 1.0).abs() > 1e-9
    }
}

pub(super) struct Machine {
    pub cursor: Cursor,
    pub strokes: Vec<Vec<Vec2>>,
    pub points: usize,
    pub ink_right: Option<f64>,
    pub ink_top: f64,
    pub ink_bottom: f64,
    pub steps: usize,
    draw: bool,
}

impl Machine {
    pub fn new(cursor: Cursor, draw: bool) -> Self {
        Self { cursor, strokes: Vec::new(), points: 0, ink_right: None, ink_top: 0.0, ink_bottom: 0.0, steps: 0, draw }
    }

    pub fn exhausted(&self) -> bool {
        self.steps >= MAX_STEPS || self.points >= MAX_POINTS
    }

    pub fn charge_points(&mut self, count: usize) -> bool {
        self.points = self.points.saturating_add(count);
        self.points <= MAX_POINTS
    }

    fn tick(&mut self) -> Option<()> {
        if self.exhausted() {
            return None;
        }
        self.steps += 1;
        Some(())
    }

    fn to(&mut self, p: Vec2) -> Option<()> {
        if !p.x.is_finite() || !p.y.is_finite() || p.x.abs().max(p.y.abs()) > 1e9 {
            return None;
        }
        if self.cursor.pen && p != self.cursor.at {
            self.ink_right = Some(self.ink_right.unwrap_or(f64::NEG_INFINITY).max(self.cursor.at.x).max(p.x));
            self.ink_top = self.ink_top.max(self.cursor.at.y).max(p.y);
            self.ink_bottom = self.ink_bottom.min(self.cursor.at.y).min(p.y);
        }
        if self.draw && self.cursor.pen && p != self.cursor.at {
            if self.points + 2 > MAX_POINTS {
                return None;
            }
            self.strokes.push(vec![self.cursor.at, p]);
            self.points += 2;
        }
        self.cursor.at = p;
        Some(())
    }

    fn delta(&mut self, x: f64, y: f64, face: &Face) -> Option<()> {
        self.to(self.cursor.at + Vec2::new(x, y) * (self.cursor.scale / face.above))
    }

    fn arc(&mut self, radius: f64, start: f64, sweep: f64) -> Option<()> {
        let at = self.cursor.at;
        let center = at - Vec2::new(start.cos(), start.sin()) * radius;
        // At most 128 segments per revolution; independent of hostile coordinates.
        let n = (sweep.abs() / (std::f64::consts::TAU / 128.0)).ceil().clamp(1.0, 128.0) as usize;
        for i in 1..=n {
            self.tick()?;
            let angle = start + sweep * i as f64 / n as f64;
            self.to(center + Vec2::new(angle.cos(), angle.sin()) * radius)?;
        }
        Some(())
    }

    fn bulge(&mut self, x: f64, y: f64, bulge: i8, face: &Face) -> Option<()> {
        if bulge == 0 || (x == 0.0 && y == 0.0) {
            return self.delta(x, y, face);
        }
        let b = f64::from(bulge) / 127.0;
        let d = Vec2::new(x, y) * (self.cursor.scale / face.above);
        let start = self.cursor.at;
        let center = start + d * 0.5 + Vec2::new(-d.y, d.x) * ((1.0 - b * b) / (4.0 * b));
        let v = start - center;
        self.arc(v.x.hypot(v.y), v.y.atan2(v.x), 4.0 * b.atan())?;
        self.cursor.at = start + d;
        Some(())
    }

    pub fn glyph(&mut self, face: &Face, code: u16, depth: usize) -> Option<()> {
        if depth >= MAX_DEPTH {
            return None;
        }
        if depth == 0 {
            self.cursor.pen = true;
        }
        let mut r = Reader { bytes: face.program(code)?, pos: 0 };
        let start_steps = self.steps;
        while let Some(op) = r.byte() {
            if self.steps - start_steps >= 32_768 {
                return None;
            }
            if op == 0 {
                return Some(());
            }
            self.command(face, op, &mut r, depth, true)?;
        }
        None // An unterminated program is malformed.
    }

    fn displacements(&mut self, face: &Face, op: u8, r: &mut Reader<'_>, execute: bool) -> Option<()> {
        loop {
            self.tick()?;
            let x = r.byte()? as i8;
            let y = r.byte()? as i8;
            if matches!(op, 9 | 13) && x == 0 && y == 0 {
                break;
            }
            let bulge = if op >= 12 { r.byte()? as i8 } else { 0 };
            if execute {
                self.bulge(f64::from(x), f64::from(y), bulge, face)?;
            }
            if matches!(op, 8 | 12) {
                break;
            }
        }
        Some(())
    }

    fn command(&mut self, face: &Face, op: u8, r: &mut Reader<'_>, depth: usize, execute: bool) -> Option<()> {
        self.tick()?;
        match op {
            0 => {}
            1 | 2 => {
                if execute {
                    self.cursor.pen = op == 1;
                }
            }
            3 | 4 => {
                let n = f64::from(r.byte()?);
                if execute {
                    if n == 0.0 {
                        return None;
                    }
                    self.cursor.scale *= if op == 3 { 1.0 / n } else { n };
                    if !(1e-9..=1e9).contains(&self.cursor.scale) {
                        return None;
                    }
                }
            }
            5 => {
                if execute {
                    if self.cursor.stack.len() >= 16 {
                        return None;
                    }
                    self.cursor.stack.push(self.cursor.at);
                }
            }
            6 => {
                if execute {
                    self.cursor.at = self.cursor.stack.pop()?;
                }
            }
            7 => {
                let first = r.byte()?;
                let code = if face.kind == Kind::Bigfont { u16::from_le_bytes([first, r.byte()?]) } else { u16::from(first) };
                if execute {
                    self.glyph(face, code, depth + 1)?;
                }
            }
            8 | 9 | 12 | 13 => self.displacements(face, op, r, execute)?,
            10 | 11 => {
                let (start_offset, end_offset, radius) = if op == 11 {
                    let a = f64::from(r.byte()?) / 256.0;
                    let b = f64::from(r.byte()?) / 256.0;
                    let hi = r.byte()?;
                    (a, b, f64::from(u16::from_be_bytes([hi, r.byte()?])))
                } else {
                    (0.0, 0.0, f64::from(r.byte()?))
                };
                let spec = i16::from(r.byte()? as i8);
                let sign = if spec < 0 { -1.0 } else { 1.0 };
                let magnitude = spec.unsigned_abs();
                let start_octant = (magnitude >> 4) & 7;
                let count = magnitude & 7;
                let count = if count == 0 { 8.0 } else { f64::from(count) };
                let start = f64::from(start_octant) + sign * start_offset;
                let sweep = sign * (count - start_offset - if end_offset == 0.0 { 0.0 } else { 1.0 - end_offset });
                if execute {
                    self.arc(radius * self.cursor.scale / face.above, start * std::f64::consts::FRAC_PI_4, sweep * std::f64::consts::FRAC_PI_4)?;
                }
            }
            14 => {
                // Horizontal text: consume the WHOLE next instruction without executing it.
                // In particular a skipped vector scale must not change subsequent glyphs.
                if depth >= MAX_DEPTH {
                    return None;
                }
                let next = r.byte()?;
                self.command(face, next, r, depth + 1, false)?;
            }
            15 => return None,
            _ => {
                const DIR: [(f64, f64); 16] = [
                    (1.0, 0.0),
                    (1.0, 0.5),
                    (1.0, 1.0),
                    (0.5, 1.0),
                    (0.0, 1.0),
                    (-0.5, 1.0),
                    (-1.0, 1.0),
                    (-1.0, 0.5),
                    (-1.0, 0.0),
                    (-1.0, -0.5),
                    (-1.0, -1.0),
                    (-0.5, -1.0),
                    (0.0, -1.0),
                    (0.5, -1.0),
                    (1.0, -1.0),
                    (1.0, -0.5),
                ];
                let (x, y) = DIR.get(usize::from(op & 15)).copied()?;
                let len = f64::from(op >> 4);
                if execute {
                    self.delta(x * len, y * len, face)?;
                }
            }
        }
        Some(())
    }
}
