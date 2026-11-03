use super::common::{DimText, add_dim, assoc_enabled, circle_of, dim, hex_param, linear_kind, pick_at, with_assoc};
use super::{
    Accept, AssocSnap, DimAssoc, DimKind, Dimension, EngineError, EntityKind, Handle, Input, Interactive, Prim, Prompt, Result, Segment, Session,
    Step, Value, Vec2, bad, json, point_req, points_param, str_param, v3,
};

pub(super) fn run_linear(s: &mut Session, p: &Value) -> Result<Value> {
    let object = hex_param(p, "object");
    let (a, b, explicit) = if let Some(origins) = p.get("origins") {
        if ["object", "p1", "p2"].iter().any(|key| p.get(*key).is_some()) {
            return Err(bad("dimlinear", "`origins` cannot be combined with `object`, `p1` or `p2`"));
        }
        if s.doc()?.header.i64("DIMASSOC", 2) != 2 {
            return Err(bad("dimlinear", "explicit `origins` require DIMASSOC=2"));
        }
        let origins = origins.as_array().ok_or_else(|| bad("dimlinear", "`origins` must contain exactly two object references"))?;
        let [first, second] = origins.as_slice() else {
            return Err(bad("dimlinear", "`origins` must contain exactly two object references"));
        };
        let (a, first) = explicit_origin(s, first, "p13")?;
        let (b, second) = explicit_origin(s, second, "p14")?;
        (a, b, Some(vec![first, second]))
    } else {
        let (a, b) = match object {
            Some(h) => object_extent(s, h).ok_or_else(|| bad("dimlinear", "`object` must be a line, arc, circle or polyline segment"))?,
            None => (point_req("dimlinear", p, "p1")?, point_req("dimlinear", p, "p2")?),
        };
        (a, b, None)
    };
    let at = point_req("dimlinear", p, "at")?;
    let rot = p.get("rotation").and_then(Value::as_f64).map(f64::to_radians);
    let k = dim(s, linear_kind(a, b, at, rot, false), at, a, b, Vec2::ZERO, str_param(p, "text").unwrap_or(""));
    let k = if let Some(assoc) = explicit { with_assoc(k, assoc) } else { with_object_assoc(s, k, object) };
    Ok(json!({ "handle": add_dim(s, k)?.hex() }))
}

/// Resolve the caller's chosen object and snap, never an object found nearby.
/// Invalid references fail before adding the dimension or changing the drawing.
fn explicit_origin(s: &Session, value: &Value, point: &str) -> Result<(Vec2, DimAssoc)> {
    let handle = hex_param(value, "handle").ok_or_else(|| bad("dimlinear", "each origin requires a valid `handle`"))?;
    let snap = match value.get("snap").and_then(Value::as_str) {
        Some("start") => AssocSnap::Start,
        Some("end") => AssocSnap::End,
        Some("mid") => AssocSnap::Mid,
        Some("center") => AssocSnap::Center,
        _ => return Err(bad("dimlinear", "origin `snap` must be start, end, mid or center")),
    };
    let doc = s.doc()?;
    if doc.space(&s.space()).and_then(|store| store.get(handle)).is_none() {
        return Err(bad("dimlinear", "origin target is not in the current drawing space"));
    }
    let assoc = DimAssoc { point: point.into(), handle, snap };
    let at = crate::assoc::snap_point(doc, &assoc)
        .filter(|p| p.is_finite())
        .ok_or_else(|| bad("dimlinear", "origin snap cannot be resolved on the selected object"))?;
    Ok((at, assoc))
}

/// Picking an object is stronger evidence than finding another object at the
/// same coordinates. Preserve that selection in both command and UI paths.
fn with_object_assoc(s: &Session, k: EntityKind, object: Option<Handle>) -> EntityKind {
    let Some(h) = object.filter(|_| assoc_enabled(s)) else { return k };
    let Some(e) = s.doc().ok().and_then(|d| d.entity(h)) else { return k };
    let snaps = match e.kind {
        EntityKind::Line(_) | EntityKind::Arc(_) => [AssocSnap::Start, AssocSnap::End],
        EntityKind::Circle(_) => [AssocSnap::OnCircle { angle: std::f64::consts::PI }, AssocSnap::OnCircle { angle: 0.0 }],
        EntityKind::LwPolyline(_) => [AssocSnap::Vertex { index: 0 }, AssocSnap::Vertex { index: 1 }],
        _ => return k,
    };
    with_assoc(k, ["p13", "p14"].into_iter().zip(snaps).map(|(point, snap)| DimAssoc { point: point.into(), handle: h, snap }).collect())
}

pub(super) fn run_aligned(s: &mut Session, p: &Value) -> Result<Value> {
    let a = point_req("dimaligned", p, "p1")?;
    let b = point_req("dimaligned", p, "p2")?;
    let at = point_req("dimaligned", p, "at")?;
    let k = dim(s, DimKind::Aligned, at, a, b, Vec2::ZERO, str_param(p, "text").unwrap_or(""));
    Ok(json!({ "handle": add_dim(s, k)?.hex() }))
}

pub(super) fn run_ordinate(s: &mut Session, p: &Value) -> Result<Value> {
    let f = point_req("dimordinate", p, "feature")?;
    let l = point_req("dimordinate", p, "leader")?;
    let xtype = p.get("xtype").and_then(Value::as_bool).unwrap_or((l - f).y.abs() > (l - f).x.abs());
    let k = dim(s, DimKind::Ordinate { x_type: xtype }, Vec2::ZERO, f, l, Vec2::ZERO, "");
    Ok(json!({ "handle": add_dim(s, k)?.hex() }))
}

/// The last linear/aligned dimension (for CONTINUE/BASELINE).
fn last_linear(s: &Session) -> Option<Dimension> {
    let h = s.last_dim?;
    match &s.doc().ok()?.entity(h)?.kind {
        EntityKind::Dimension(d) if matches!(d.kind, DimKind::Linear { .. } | DimKind::Aligned) => Some(d.clone()),
        _ => None,
    }
}

/// Next dimension in a chain from `prev` to a new second origin.
fn chained(s: &Session, prev: &Dimension, next: Vec2, baseline: bool) -> EntityKind {
    // DIMDLI × the overall scale of the dimension being continued: its overrides (SETVAR DIM*)
    // included, and DIMSCALE 0 falling back to the drawing's DIMSCALE as when it is drawn.
    let spacing = s
        .doc()
        .ok()
        .map(|d| {
            let st = d.dim_style(&prev.style).cloned().unwrap_or_default().with_overrides(&prev.overrides);
            st.baseline_spacing * st.effective_scale(d.header.f64("DIMSCALE", 1.0))
        })
        .filter(|v| v.is_finite())
        .unwrap_or(0.38);
    let dir = match prev.kind {
        DimKind::Linear { rotation } => Vec2::from_angle(rotation),
        _ => (prev.p14.xy() - prev.p13.xy()).normalized(),
    };
    let first = if baseline { prev.p13.xy() } else { prev.p14.xy() };
    let mut defpt = prev.defpt.xy();
    if baseline {
        // Offset the dimension line away from the measured points.
        let n = dir.perp();
        let side = if (defpt - prev.p13.xy()).dot(n) >= 0.0 { 1.0 } else { -1.0 };
        defpt += n * (spacing * side);
    }
    EntityKind::Dimension(Dimension {
        p13: v3(first),
        p14: v3(next),
        defpt: v3(defpt),
        text: String::new(),
        block: None,
        assoc: Vec::new(),
        marks: Default::default(),
        user_text_pos: false,
        ..prev.clone()
    })
}

/// Most dimensions one DIMCONTINUE/DIMBASELINE call adds.
pub(super) const MAX_CHAIN: usize = 100_000;

fn chain_run(s: &mut Session, p: &Value, baseline: bool) -> Result<Value> {
    let id = if baseline { "dimbaseline" } else { "dimcontinue" };
    let pts = p.get("points").and_then(Value::as_array).ok_or_else(|| bad(id, "`points` is required"))?;
    if pts.len() > MAX_CHAIN {
        return Err(bad(id, format!("at most {MAX_CHAIN} `points` per call")));
    }
    let pts = points_param(p, "points").ok_or_else(|| bad(id, "`points` must be a list of points"))?;
    // One snap index for the whole chain: looking up each new point's object by scanning the
    // drawing made long chains quadratic (the added dimensions are no snap targets).
    let snaps = if assoc_enabled(s) { Some(crate::assoc::SnapIndex::new(s.doc()?, &s.space())) } else { None };
    let mut out = Vec::new();
    for q in pts {
        let prev = last_linear(s).ok_or_else(|| bad(id, "no linear dimension to continue"))?;
        let mut k = chained(s, &prev, q, baseline);
        if let (Some(ix), EntityKind::Dimension(dm)) = (&snaps, &mut k) {
            dm.assoc = ix.auto_assoc(&[("p13", dm.p13.xy()), ("p14", dm.p14.xy())]);
        }
        let h = s.add_entity(k)?;
        s.last_dim = Some(h);
        out.push(h.hex());
    }
    Ok(json!({ "handles": out }))
}

pub(super) fn run_continue(s: &mut Session, p: &Value) -> Result<Value> {
    chain_run(s, p, false)
}

pub(super) fn run_baseline(s: &mut Session, p: &Value) -> Result<Value> {
    chain_run(s, p, true)
}

/// The two points a picked object is dimensioned between (DIMLINEAR "select object").
fn object_extent(s: &Session, h: Handle) -> Option<(Vec2, Vec2)> {
    let e = s.doc().ok()?.entity(h)?;
    if let Some((c, r)) = circle_of(s, h).filter(|_| matches!(e.kind, EntityKind::Circle(_))) {
        return Some((c - Vec2::X * r, c + Vec2::X * r));
    }
    e.kind.prims().into_iter().find_map(|pr| match pr {
        Prim::Seg(sg @ (Segment::Line(_) | Segment::Arc { .. })) => Some((sg.start(), sg.end())),
        _ => None,
    })
}

pub(super) struct LinearM {
    aligned: bool,
    pts: Vec<Vec2>,
    object: Option<Handle>,
    rotation: Option<f64>,
    text: DimText,
    selecting: bool,
    /// Asking for the dimension line angle (Rotated).
    rotating: bool,
}

impl LinearM {
    pub(super) fn new(aligned: bool) -> Self {
        LinearM { aligned, pts: Vec::new(), object: None, rotation: None, text: DimText::default(), selecting: false, rotating: false }
    }
}

impl Interactive for LinearM {
    fn name(&self) -> &'static str {
        if self.aligned { "DIMALIGNED" } else { "DIMLINEAR" }
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        if self.rotating {
            return Prompt::new("Specify angle of dimension line", Accept::NUMBER);
        }
        if let Some(p) = self.text.prompt() {
            return p;
        }
        if self.selecting {
            return Prompt::new("Select object to dimension", Accept::POINT);
        }
        match self.pts.len() {
            0 => Prompt::new("Specify first extension line origin", Accept::POINT).default("select object"),
            1 => Prompt::new("Specify second extension line origin", Accept::POINT).base_opt(self.pts.first().copied()),
            // Once the orientation is forced, only the text options remain.
            _ if self.aligned || self.rotation.is_some() => {
                Prompt::new("Specify dimension line location", Accept::POINT).kw(&["Mtext", "Text", "Angle"])
            }
            _ => Prompt::new("Specify dimension line location", Accept::POINT).kw(&["Mtext", "Text", "Angle", "Horizontal", "Vertical", "Rotated"]),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if self.rotating {
            if let Input::Text(t) = &i {
                let Some(r) = s.angle_settings().direction(t) else {
                    s.echo("Requires a valid angle.");
                    return Ok(Step::Continue);
                };
                self.rotation = Some(r);
            }
            self.rotating = false;
            return Ok(Step::Continue);
        }
        if self.text.answer(s, &i) {
            return Ok(Step::Continue);
        }
        if self.selecting {
            if let Input::Point(p) = i {
                let Some(h) = pick_at(s, p) else {
                    s.echo("*Invalid selection*");
                    return Ok(Step::Continue);
                };
                match object_extent(s, h) {
                    Some((a, b)) => {
                        self.pts = vec![a, b];
                        self.object = Some(h);
                        self.selecting = false;
                    }
                    None => {
                        s.echo("Object cannot be dimensioned this way.");
                    }
                }
            }
            return Ok(Step::Continue);
        }
        match (self.pts.len(), i) {
            (0, Input::Enter) => {
                self.selecting = true;
                Ok(Step::Continue)
            }
            (n, Input::Point(p)) if n < 2 => {
                self.pts.push(p);
                Ok(Step::Continue)
            }
            (_, Input::Point(at)) => {
                let (Some(&a), Some(&b)) = (self.pts.first(), self.pts.get(1)) else { return Ok(Step::Continue) };
                let k = self.text.apply(dim(s, linear_kind(a, b, at, self.rotation, self.aligned), at, a, b, Vec2::ZERO, ""));
                let k = with_object_assoc(s, k, self.object);
                add_dim(s, k)?;
                if let Some(EntityKind::Dimension(d)) = s.last_dim.and_then(|h| s.doc().ok()?.entity(h).map(|e| e.kind.clone())) {
                    let st = s.doc()?.dim_style(&d.style).cloned().unwrap_or_default();
                    let g = cadcraft_render::dimension_geometry(&d, &st, 1.0);
                    s.echo(format!("Dimension text = {}", g.value));
                }
                Ok(Step::Done)
            }
            (_, Input::Keyword(k)) => {
                match k.as_str() {
                    "Horizontal" => self.rotation = Some(0.0),
                    "Vertical" => self.rotation = Some(std::f64::consts::FRAC_PI_2),
                    "Rotated" => self.rotating = true,
                    other => {
                        self.text.keyword(other);
                    }
                }
                Ok(Step::Continue)
            }
            (_, Input::Enter) => Ok(Step::Cancel),
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, s: &Session, c: Vec2) -> Vec<EntityKind> {
        match self.pts.as_slice() {
            [a] => vec![crate::cmd::helpers::line(*a, c)],
            [a, b] => vec![self.text.apply(dim(s, linear_kind(*a, *b, c, self.rotation, self.aligned), c, *a, *b, Vec2::ZERO, ""))],
            _ => Vec::new(),
        }
    }
}

#[derive(Default)]
pub(super) struct OrdinateM {
    feature: Option<Vec2>,
    /// Xdatum / Ydatum: the forced ordinate type (`true` = X datum).
    xtype: Option<bool>,
    text: DimText,
}

impl OrdinateM {
    fn kind(&self, s: &Session, f: Vec2, l: Vec2) -> EntityKind {
        let x = self.xtype.unwrap_or((l - f).y.abs() > (l - f).x.abs());
        self.text.apply(dim(s, DimKind::Ordinate { x_type: x }, Vec2::ZERO, f, l, Vec2::ZERO, ""))
    }
}

impl Interactive for OrdinateM {
    fn name(&self) -> &'static str {
        "DIMORDINATE"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        if let Some(p) = self.text.prompt() {
            return p;
        }
        match (self.feature, self.xtype) {
            (None, _) => Prompt::new("Specify feature location", Accept::POINT),
            (Some(f), None) => Prompt::new("Specify leader endpoint", Accept::POINT).kw(&["Xdatum", "Ydatum", "Mtext", "Text", "Angle"]).base(f),
            (Some(f), Some(_)) => Prompt::new("Specify leader endpoint", Accept::POINT).kw(&["Mtext", "Text", "Angle"]).base(f),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if self.text.answer(s, &i) {
            return Ok(Step::Continue);
        }
        match (self.feature, i) {
            (None, Input::Point(p)) => {
                self.feature = Some(p);
                Ok(Step::Continue)
            }
            (Some(_), Input::Keyword(k)) => {
                match k.as_str() {
                    "Xdatum" => self.xtype = Some(true),
                    "Ydatum" => self.xtype = Some(false),
                    other => {
                        self.text.keyword(other);
                    }
                }
                Ok(Step::Continue)
            }
            (Some(f), Input::Point(l)) => {
                add_dim(s, self.kind(s, f, l))?;
                Ok(Step::Done)
            }
            (_, Input::Enter) => Ok(Step::Cancel),
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, s: &Session, c: Vec2) -> Vec<EntityKind> {
        self.feature.map(|f| vec![self.kind(s, f, c)]).unwrap_or_default()
    }
}

pub(super) struct ChainM {
    baseline: bool,
    created: Vec<Handle>,
}

impl ChainM {
    pub(super) fn new(_s: &Session, baseline: bool) -> Self {
        ChainM { baseline, created: Vec::new() }
    }
}

impl Interactive for ChainM {
    fn name(&self) -> &'static str {
        if self.baseline { "DIMBASELINE" } else { "DIMCONTINUE" }
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        if last_linear(s).is_none() {
            s.echo("No linear dimension to continue. Create one with DIMLINEAR first.");
            return Ok(Step::Done);
        }
        Ok(Step::Continue)
    }
    fn prompt(&self, s: &Session) -> Prompt {
        Prompt::new("Specify a second extension line origin", Accept::POINT)
            .kw(&["Select", "Undo"])
            .default("Select")
            .base_opt(last_linear(s).map(|d| if self.baseline { d.p13.xy() } else { d.p14.xy() }))
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match i {
            Input::Point(p) => {
                let prev = last_linear(s).ok_or_else(|| EngineError::Other("no linear dimension".into()))?;
                let k = chained(s, &prev, p, self.baseline);
                self.created.push(add_dim(s, k)?);
                Ok(Step::Continue)
            }
            Input::Keyword(k) if k == "Undo" => {
                if let Some(h) = self.created.pop() {
                    s.doc_mut()?.remove_entity(h);
                    s.last_dim = self.created.last().copied();
                }
                Ok(Step::Continue)
            }
            Input::Enter => Ok(Step::Done),
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, s: &Session, c: Vec2) -> Vec<EntityKind> {
        last_linear(s).map(|d| vec![chained(s, &d, c, self.baseline)]).unwrap_or_default()
    }
}
