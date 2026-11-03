# Compiled engineering review export

`mechanical-review-v1` is a fixed, versioned presentation profile in the Rust
command engine. The CLI and MCP use the same `reviewplot` implementation. This
keeps a verified print recipe and its required fonts from being accidentally
replaced by the general `convert` defaults.

```sh
cadcraft-cli review drawing.dwg review.png
cadcraft-cli review drawing.dwg review-8k.png --quality 8k
cadcraft-cli review drawing.dwg review.png --font-dir /private/verified-fonts
```

The default is a **2560 × 1440 PNG**. `8k` produces **7680 × 4320**. Both use model
space, drawing extents, fit and center, zero margins, landscape, builtin
`monochrome.ctb`, lineweights enabled and a 0.70 mm default lineweight. The paper
dimensions are 365.76 × 650.24 mm, with resolution 100 or 300 respectively.
Transparency is disabled. The profile operates on a render copy; it does not
change the drawing, its dimensions, the session's lineweight, or the source file.
Output must have a new `.png` path. Unknown options and unsupported quality names
are errors, so a typo cannot silently select a different recipe. Native output uses
exclusive hard-link publication, so concurrent writers cannot replace an existing
review; filesystems without hard-link support report an error.

The response includes the profile identifier, settings, font hashes, output byte
count and output SHA-256. It is a rendering receipt, not engineering acceptance
of the part or a claim that a drawing has been fully reconstructed.

## Font pack

Fonts remain user-supplied runtime assets. There are no third-party SHX files,
customer drawings or customer reference images in this repository or embedded
in the executable. Distribute fonts only under their applicable permissions.

The application looks for `Fonts/cadcraft-fonts.json` beside the executable,
or uses the explicitly supplied directory. No `CADCRAFT_SHX_PATH` environment
variable is required. A local deployment can place its already verified font
pack next to the program and use the first command without additional switches.

The manifest has this format; replace the illustrative digest with the actual
SHA-256 of the supplied font:

```json
{
  "schema": "cadcraft.font-pack/1",
  "fonts": [
    { "name": "example.shx", "sha256": "<64 hexadecimal characters>" }
  ]
}
```

Seal a pack only after choosing the intended font files. A matching hash proves
that the selected bytes have not changed; it does not prove their provenance or
licence. Files must be simple `.shx` names, with no subdirectories or path escapes.
The limits are 64 entries, 16 MiB per font, 64 MiB total and a 64 KiB manifest.

The engine checks the manifest, every asset hash, the SHX parser, required base
and Big Font roles, and the drawing's code page before producing any image.
It checks the bytes again on each export, even when a font is already cached.
Missing, altered, unsupported or mismatched fonts cause an error instead of
fallback. A nonempty `CADCRAFT_SHX_CODEPAGE` override is rejected. The shared
font cache accepts the validated pack atomically and accounts for replacements.

This profile currently supports the builtin stroke font and legacy horizontal
SHX/Big Font styles. It conservatively checks all declared text styles, including
unused styles. Vertical text and TTF styles are outside its qualified scope and
produce an error. Input read warnings, skipped records and a drawing-local
`monochrome.ctb` that shadows the builtin are also errors. General conversion and
plot commands remain available with their own broader behavior.

## MCP and command engine

Open the drawing, then invoke the shared command:

```json
{"name":"execute","arguments":{"command":"reviewplot","params":{"path":"/private/review.png","quality":"preview","fontDirectory":"/private/verified-fonts"}}}
```

Omitting `path` returns base64 PNG data with the same receipt. The command is
available through the normal command registry, so scripts, the desktop command
line and MCP do not carry separate copies of the print recipe. In a browser,
filesystem font-pack loading is unavailable; builtin-font drawings can still
use the profile through the host's plot/delivery hooks.

## Regression scope

Public tests use original synthetic geometry and font instruction streams. They
cover a changed cached font, missing Big Font, malformed font data, path escapes,
duplicate names, unsupported modes, source-read failures, repeated registration,
CLI mistakes and source preservation. A real PNG from the compiled command is
compared against the existing explicit plot path on a public synthetic sample.

Private acceptance additionally binds each customer's source DWG, font pack,
approved preview and full-resolution reference by SHA-256. Compare actual PNG
bytes after building the exact version, and keep that result beside its runtime
manifest. Private reference data must not be copied into public regression
fixtures. An image match qualifies only that rendering case, not geometric
reconstruction, manufacturing intent or all DWG files.
