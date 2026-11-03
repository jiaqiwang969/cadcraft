# Native DWG dimension text background

The DWG bridge uses an R2000 DXF transport. A dimension style's `DIMTFILL`
(69) and `DIMTFILLCLR` (70) were kept in CADCRAFT `DIMVARS` xdata there,
even when the requested DWG format supported native backgrounds. CADCraft
could recover its own setting, while other CAD readers received the native
default of zero.

The bridge now promotes a bounded, unambiguous pair of these values into the
unmodified acadrust document model before writing R2007 or newer DWG. The
current style's header values follow the same setting. No geometry,
dimension measurement, object reference or text override is changed.

For R2000/R2004 output the original extension remains; this does not claim
native background support in those formats. Native values in R2007+ input
take precedence over old extensions. Other applications' xdata, duplicates,
malformed lists and out-of-range values do not enable a background.

The anonymous dimension display block also carries an MTEXT mask for
`DIMTFILL=1` in R2007+ output. A style alone does not repair cached graphics
on first open. The native mask uses fill-on plus drawing-background flags,
with its margin derived from DIMGAP/text height. Ordinary MTEXT and user
blocks are unchanged. Reopening and resaving retains the dimension mask.
The existing explicit-color (`DIMTFILL=2`) SOLID representation is unchanged.

The native mask groups follow the public [MTEXT DXF reference](https://help.autodesk.com/cloudhelp/2020/ENU/AutoCAD-DXF/files/GUID-5E5DB93B-F8D3-4433-ADF7-E92E250D2BAB.htm)
and the [Autodesk explanation of the background bits](https://blog.autodesk.io/change-the-background-mask-property-of-acdbmtext-object-programmatically-using-objectarx/).

`crates/dwg/tests/dim_text_fill.rs` uses original synthetic DXF inputs and
reads native DWG style fields directly through acadrust. This is a local
regression check, not an independent cross-CAD acceptance. Independent CAD
results stay outside this repository with the owning engineering Project.

Format/behavior references: Autodesk's published DXF DIMSTYLE table reference,
the public ODA DWG specification section 5.4.1, and Autodesk's DIMTFILL system
variable reference (0 none, 1 drawing background, 2 explicit fill color).
