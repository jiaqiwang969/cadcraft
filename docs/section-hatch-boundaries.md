# Analytic section boundaries

The `hatch` JSON command accepts a `handles` set containing one simple,
endpoint-connected XY loop of whole LINE and ARC objects. The loop must have
2–512 unique edges. Source order and edge direction may differ from traversal.
The command preserves circular bulges and all source handles; it does not
create a duplicate polyline or bridge gaps using HPGAPTOL. Unsupported, open,
branched, disconnected or self-intersecting whole-edge sets return an error.

This path supports source-bound section views where the exact material boundary
has already been constructed. It complements the existing selected closed-object
and picked-point paths. The latter continues to trace general regions using its
existing tessellation and gap policy. No general curve-trimming claim is made.

Associative refill first tries the same whole-edge analytic loop. Other cases
continue through the existing general tracer. With HPGAPTOL zero, opening a
boundary detaches the hatch while retaining its last shape. Native DXF/DWG
stores standard source handles and persistent reactors; independent target-CAD
editing must be verified separately from these local engine tests.

The numerical endpoint join bound is 1e-8 drawing units. Coordinates and radii
are finite and bounded to 1e9; this is a computational profile, not a physical
tolerance. The caller retains the distinction between true material edges and
artificial display cuts; both can delimit the displayed material region without
making a display cut a physical part edge.
