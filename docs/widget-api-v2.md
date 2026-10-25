# Widget API v2 — canvas and static packages

API v1 commands and row frames continue to work. v2 adds a clipped, layered
canvas in logical pixels; no browser, GTK, new runtime dependencies, host
threads, or per-widget processes are required for static content.

## Package manifest

```toml
version = 2
id = "photo"
frame = "frame.json"
width = 360
height = 480
decoration = "none"
input = "none"
position = [64, 96]
```

Place this in `widgets/photo/widget.toml` under one of the existing
[discovery roots](widget-api.md#package-discovery-and-installation), then add
`"photo"` to the main `widgets` array. The `id` must match its directory.
Both manifest versions 1 and 2 are accepted; defaults keep old packages working.
Unknown fields, invalid dimensions, and ambiguous sources are errors.

| Field | Default | Meaning |
| --- | --- | --- |
| `command` | absent | Persistent program's argument array; stdout NDJSON as in v1 |
| `frame` | absent | Static JSON file; choose exactly one of `command` or `frame` |
| `width` / `height` | 300 / 178 | Panel dimensions, each 32..2048 logical pixels |
| `decoration` | `"card"` | `card`: themed card/title; `none`: transparent canvas without title or frame |
| `input` | `"title"` | `title`: top 42px drags; `all`: whole panel drags; `none`: all clicks pass through |
| `position` | automatic | Optional `[x,y]` initial logical coordinates; saved layout takes precedence |
| `protocol` | 1 | Requested child protocol hint, 1 or 2; host accepts frames of either version |
| `restart_seconds` | 30 | Child restart delay, 1..86400 |
| `timeout_seconds` | 60 | Child heartbeat timeout, 1..86400 |

Static files load once during configuration validation. They do not spawn a
process, use a poll fd, require heartbeats, or trigger periodic redraws. The
host's normal event loop still runs. Restart after changing a manifest, static
frame, or static image. Disabled packages are not resolved. Relative frame and
image paths resolve against the package directory; absolute paths work too.
Legacy `[[plugins]]` also accepts these fields and resolves paths against the
main config directory. Paths do not expand `~`, variables or shell syntax.

For command packages, add `protocol = 2` and keep writing complete replacement
frames with a newline and flush. Repeating a frame is a heartbeat without a
redraw. Existing timeout, bounded nonblocking reads, process-group cleanup,
restarts and crash handling still apply. Children receive:

- `RS_WWIDGET_PROTOCOL`: the manifest's protocol hint.
- `RS_WWIDGET_WIDTH`, `RS_WWIDGET_HEIGHT`: panel dimensions.
- `RS_WWIDGET_ASSET_DIR`: package directory (config directory for legacy plugins).

This is a display API. `input` controls dragging or click-through; it does not
send buttons or keyboard events to children. All panels stay on the host's
Wayland bottom layer, without keyboard focus or reserved layout space.

## Frame format

```json
{
  "version": 2,
  "elements": [
    {
      "bounds": [0, 0, 360, 480],
      "content": {"type": "image", "path": "poster.png", "fit": "cover"}
    },
    {
      "bounds": [16, 400, 328, 64],
      "opacity": 0.85,
      "content": {"type": "rect", "color": "#282828", "radius": 12}
    },
    {
      "bounds": [24, 414, 312, 40],
      "content": {"type": "text", "text": "Hello desktop", "size": 28, "align": "center"}
    }
  ]
}
```

`title` (up to 128 characters) and v1 `rows` are optional in v2. Decoration and
rows render first, then elements in array order: later elements cover earlier
ones. v1 still requires `title` and `rows` and rejects `elements`. Every element
clips to its `bounds`, and the whole frame clips to its panel. Empty frames
clear the canvas. Frames are full replacements, not patches. Rows use the legacy
300px layout scaled horizontally for a custom panel width; canvas coordinates
remain unscaled logical pixels. Prefer canvas text for custom typography.

`bounds` is `[x, y, width, height]`. Coordinates and dimensions must be finite
with absolute values at most 8192, and width/height must be positive.
`opacity` defaults to 1, accepts 0..1 and applies to the entire element.

Colors accept `#RRGGBB`, `#RRGGBBAA`, `transparent`, or a theme token:
`background`, `foreground`, `muted`, `accent`, `green`, `border`,
`bar_background`, `grid_background`. Color alpha multiplies element opacity.
Text is literal, without markup, in the host's configured font family.

| `content.type` | Required fields | Optional fields (defaults) |
| --- | --- | --- |
| `image` | `path` | `fit`: `contain`, `cover`, `stretch` (default `contain`); `revision`: unsigned integer (0) |
| `text` | `text` | `size`: 1..512 (16), `color` (`foreground`), `align`: `left`/`center`/`right` (`left`), `wrap` (false) |
| `rect` | — | `color` (`accent`), `radius`: 0..2048 (0) |
| `line` | — | `color` (`accent`), `thickness`: 0.1..128 (1) |
| `bar` | `value`: 0..1 | `color` (`accent`), `background` (`bar_background`), `radius`: 0..2048 (0) |
| `graph` | `values`: 1..512 values in 0..1, oldest first | `color` (`accent`), `fill` (false), `thickness`: 0.1..128 (1) |

`contain` centers the full image with transparent margins; `cover` fills and
crops centrally; `stretch` fills the box. PNG transparency is preserved with
premultiplied alpha. JPEG is supported. Images are local still images only;
no URL fetches, SVG, animation or video. A line runs from the bottom-left to
the top-right of its box. For a horizontal separator, use a thin rectangle.
Graphs use a fixed 0..1 vertical range; `fill` selects a solid area instead of
a stroke. Text ellipsizes when it exceeds its box; `wrap` enables word wrapping.

## Bounds and image lifetime

Each frame is at most 65,536 UTF-8 bytes and 256 elements. Each text element
has at most 4096 characters. v1 row limits remain unchanged. Unknown types,
fields, invalid enums, nonfinite numbers and values outside bounds reject the
frame and use the normal plugin failure/restart path. Bad static JSON is a
configuration error. A missing or invalid image logs an error and leaves that
element transparent, while other elements still draw.

The renderer caches decoded images and failures by resolved path + `revision`,
with LRU eviction at 32 entries or 64 MiB of decoded pixel data across the host.
Encoded files are limited to 32 MiB; decoding is bounded to 4096×4096 and 128 MiB
of decoder allocation. Unchanged frames do not read files or decode images.
If a dynamic provider replaces a file at the same path, increment its image
`revision` and emit a changed frame. Prefer writing the replacement atomically.
Cache eviction can cause a later redraw to load the file again.

## Python SDK

The standard-library-only SDK keeps `Row`/`emit` for v1 and adds `Image`, `Text`,
`Rect`, `Line`, `Bar`, `Graph`, `encode_canvas`, and `emit_canvas`.

```python
import os
import time
from rs_wwidget import Image, Text, Rect, Bar, Graph, emit_canvas

width = int(os.environ.get("RS_WWIDGET_WIDTH", "360"))
while True:
    emit_canvas([
        Image("cover.jpg", (0, 0, width, 240), fit="cover"),
        Rect((12, 252, width - 24, 100), color="background", radius=12),
        Text("My panel", (24, 264, width - 48, 30), size=24),
        Bar(0.65, (24, 310, width - 48, 8), color="green", radius=4),
        Graph([0.1, 0.4, 0.2, 0.8], (24, 370, width - 48, 64)),
    ])
    time.sleep(5)
```

To write a static frame without keeping Python running:

```python
from pathlib import Path
from rs_wwidget import Image, encode_canvas
Path("frame.json").write_text(
    encode_canvas([Image("poster.png", (0, 0, 360, 480))]), encoding="utf-8"
)
```

`encode_canvas` validates the display list before writing anything.
`emit_canvas` validates, appends one newline and flushes stdout. Copy the SDK
beside your provider or put `sdk/python` on `PYTHONPATH`.

## Preview and migration

`--check-config` validates static frames without running commands. `--preview`
and `--preview-grid` render actual static frames/images; command packages use
placeholders and never execute during preview. Native host rendering, logical
sizes, drag constraints, overlay damage and saved positions all use each panel's
width and height. Existing built-in rendering and v1 defaults stay the same.

For a ready-to-use static picture, see [poster](../widgets/poster/README.md).
