# Engineering drawing integration

The engineering drawing branch retains the complete local implementation based on
CADCraft 0.15.1 (`660e15c9f494ce647e87de10adbb01a8786e0901`). Its pre-integration
source checkpoint is `1b55a0c60395db8afc3e6d2f8cc141b2cdc00fba`.

The merged repair is `ab66d6808313d3b67e519e7ee556c591f1a40d8a` from
[jiaqiwang969/cadcraft](https://github.com/jiaqiwang969/cadcraft/tree/fix/dwg-dimensions-shx-leaders).
It adds cached dimension block placement, explicit runtime SHX font lookup and
text/tolerance layout, and correct rendering of mixed-width leaders with duplicate
vertices. All repair tests are retained. Font assets are not included.

The existing explicit dimension origins, saved spline endpoint associations,
analytic quadratic spline construction, exact associated hatch boundaries and
native dimension text background support remain present. The integration adapts
the text-background lookup to the repair's `(block_name, insertion_offset)` map;
background lookup uses the name, while geometry placement retains the offset.
A combined DXF/DWG round-trip test exercises both behaviors together.

Five equivalent boolean simplifications in the DXF, render and UI crates allow
strict Rust 1.95 Clippy checks to proceed without suppressing `nonminimal_bool`.
The required nextest version remains 0.9.150; no version-check override is used.
Real drawing artifacts, native AutoCAD outputs and engineering Project records
are intentionally kept outside this source repository.

The dimension MTEXT producer now expresses DIMTFAC as the actual height ratio,
and emits DIMTOLJ alignment for the entire nominal/tolerance row. This composes
with the repair's explicit-height stack layout without compensating twice or
leaving the nominal value at MTEXT's default bottom alignment. Regression tests
compare nominal and tolerance glyph bounds for all three alignments, with stacks
both taller and shorter than the nominal text, and check limit-value heights.

The existing non-Unicode path tests remain active. The read-path test validates
the rejected path before filesystem access; the CLI test also checks the explicit
error and absence of a lossy replacement filename on filesystems that reject such
names. The integration artifact is versioned `0.15.1-aixcam.2`, satisfying the
repository's existing version policy without weakening its version validator.

Validation: `cargo xtask ci` passes all seven steps on Rust 1.95.0 and nextest
0.9.150 (format, strict Clippy, tests, translation report, assets, layers, wasm).
The native test run passes 3,065 tests. The repository's existing 28 ignored tests
remain unchanged; they include known format limitations and are not represented
as passing. The 28 focused integration tests also pass. No exclusion or lint
allowance was added to obtain these results.

## Compiled review export (`0.15.1-aixcam.3`)

The next integration starts from merge `1590d59e591442d7cbe8502d5610fb5b0dfbe70d`
and retains all of the repairs and tests above. Its shared `reviewplot` command
and CLI `review` entry compile the versioned print recipe into the engine and
require a hash-verified external SHX pack. See
[Engineering review export](engineering-review-export.md) for the profile,
deployment manifest, output receipt and regression scope. The font bytes and
private rendering references remain outside the public repository.

## Signed deviations — 0.15.1-aixcam.4

Based on compiled-review commit `6fc6ccc8d39c4f014f6ed4a4c635132dac82fc69`, retaining the DWG block/arrow, SHX, leader and compiled review fixes. A negative upper deviation now keeps its own minus sign instead of receiving an additional plus sign (`34+-0.04/-0.12` becomes `34-0.04/-0.12`). Original synthetic regressions also cover two positive deviations at four decimals, zero upper deviation and symmetric deviations. This changes formatting, not the stored dimensional requirements or geometry. The review export profile and font validation remain unchanged; qualification against the private accepted reference is recorded outside the repository.

## DXF route fidelity — 0.15.1-aixcam.5

Based on `7e201903ffe638c41cfaddaffb49b488fba8cfd6` (signed deviations and SVG source handles), retaining every earlier repair. A curve- or spline-fit 2D POLYLINE (PEDIT Fit or Spline) now keeps its fit: the reader keeps the fit flags (group 70 bits 2 and 4), the curve type (group 75) and every vertex in file order with its flags and curve-fit tangent, the spline frame vertices included; the writer writes the polyline back as POLYLINE/VERTEX/SEQEND in the same order while its drawn vertices still match the record, and as a plain polyline once they were edited, so a stale frame is never written. Moving, rotating, scaling or mirroring transforms the record with the polyline. The DWG bridge adds the curve type that acadrust's DXF writer (0.6.6) leaves out, from acadrust's own document. A DXF of 2007 or later keeps the code page the drawing was read with, as AutoCAD does, instead of declaring ANSI_1252 (its text stays UTF-8). Original synthetic regressions cover both fits at every DXF version in ASCII and binary, the edited and transformed cases, the DWG round trip, hostile fit data and the bridge fix-up.
