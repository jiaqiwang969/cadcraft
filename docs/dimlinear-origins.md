# Explicit source points for DIMLINEAR

The JSON command accepts two explicit source points in addition to the existing
point-coordinate and selected-object forms:

```json
{"origins":[{"handle":"100","snap":"end"},{"handle":"103","snap":"mid"}],"rotation":0,"at":[30,28]}
```

Each source names a real entity in the current drawing space and a supported
`start`, `end`, `mid` or `center` snap. The snap must resolve on that entity.
Exactly two origins and DIMASSOC=2 are required. Combining origins with `object`,
`p1` or `p2`, or supplying an unavailable target, fails before creating a
dimension. The rotation, text and placement options retain their existing
meaning. The command resolves positions from the specified objects; it does not
look for replacement objects at coincident coordinates.

Regression cases cover a parent span from an arc endpoint to another line,
two independently editable silhouette midpoints, coincident distractor edges,
undo, rejected ambiguous/invalid inputs, and standard DXF associations after
private metadata is removed. Independent application-level AutoCAD checks are
separate from these command and serialization tests.
