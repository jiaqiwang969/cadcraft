use super::{Deserialize, PolyVertex, Serialize, Vec3};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub a: Vec3,
    pub b: Vec3,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub p: Vec3,
    #[serde(default)]
    pub angle: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Circle {
    pub center: Vec3,
    pub radius: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Arc {
    pub center: Vec3,
    pub radius: f64,
    /// Radians, CCW.
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ellipse {
    pub center: Vec3,
    pub major: Vec3,
    pub ratio: f64,
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LwPolyline {
    pub vertices: Vec<PolyVertex>,
    pub closed: bool,
    #[serde(default)]
    pub const_width: f64,
    #[serde(default)]
    pub elevation: f64,
    /// Polyline linetype generation (PLINEGEN).
    #[serde(default)]
    pub plinegen: bool,
    /// The curve or spline fit of a polyline read from a 2D POLYLINE (PEDIT Fit or Spline), kept
    /// so the file keeps it; `None` for a plain polyline. See [`LwPolyline::fit_record`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fit: Option<PolylineFit>,
}

impl LwPolyline {
    /// The fit record while it still describes this polyline: `None` once the vertices were
    /// edited (or there is none), so a stale spline frame is never written back.
    pub fn fit_record(&self) -> Option<&PolylineFit> {
        self.fit.as_ref().filter(|f| f.matches(&self.vertices))
    }
}

/// The smoothing of a 2D polyline as the POLYLINE entity of a DXF or DWG file stores it (DXF
/// Reference, POLYLINE and VERTEX): the curve- or spline-fit flags (POLYLINE group 70 bits 2 and
/// 4), the curve type (group 75) and every vertex in file order with its flags, the spline frame
/// (control) vertices included. The polyline draws the vertices that are not frame vertices; the
/// record describes it only while those equal [`LwPolyline::vertices`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PolylineFit {
    /// POLYLINE group 70 bits 2 (curve-fit vertices added) and 4 (spline-fit vertices added).
    pub flags: u8,
    /// POLYLINE group 75: 0 none, 5 quadratic B-spline, 6 cubic B-spline, 8 Bezier.
    #[serde(default)]
    pub curve_type: i16,
    /// Every vertex in file order.
    pub sequence: Vec<FitVertex>,
}

/// A vertex of a fitted polyline with its VERTEX group 70 flags (1 extra vertex created by curve
/// fitting, 2 curve-fit tangent defined, 8 spline vertex created by spline fitting, 16 spline
/// frame control point) and its curve-fit tangent direction (group 50, radians).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FitVertex {
    pub v: PolyVertex,
    pub flags: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tangent: Option<f64>,
}

impl PolylineFit {
    /// VERTEX group 70 bit 16: a spline frame control point, not drawn.
    pub const FRAME: u8 = 16;

    /// The vertices the polyline draws: every one but the spline frame, in file order.
    pub fn drawn(&self) -> impl Iterator<Item = &PolyVertex> + '_ {
        self.sequence.iter().filter(|x| x.flags & Self::FRAME == 0).map(|x| &x.v)
    }

    /// Whether the record still describes `vertices`: the drawn vertices are exactly these (a
    /// transform moves both; any other edit leaves them apart).
    pub fn matches(&self, vertices: &[PolyVertex]) -> bool {
        self.drawn().eq(vertices.iter())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Polyline3d {
    pub points: Vec<Vec3>,
    pub closed: bool,
}

/// SOLID / TRACE: up to four corners (DXF order 1,2,4,3 when drawn).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Solid {
    pub corners: [Vec3; 4],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Face3d {
    pub corners: [Vec3; 4],
    #[serde(default)]
    pub hidden_edges: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RayLine {
    pub base: Vec3,
    pub dir: Vec3,
}
