# Poster

A static PNG/JPEG on the desktop, with rotation, rounded corners, opacity,
image fitting and optional dragging. No title, frame, panel background, child
process or refresh timer. The default is click-through. Transparent PNG pixels
stay transparent, and ordinary windows remain above the poster.

## Create a poster from your image

After building and installing:

```sh
mkdir -p ~/.config/rs-wwidget/widgets
rs-wwidget --create-poster ~/Pictures/photo.jpg \
  ~/.config/rs-wwidget/widgets/my-poster \
  --width 360 --rotation -12 --radius 16 --filter best
```

Add `"my-poster"` to the `widgets` list in
`~/.config/rs-wwidget/config.toml`, then run `rs-wwidget restart`.
The command copies the image into a new self-contained package. Its directory
name becomes the widget ID. The parent directory must already exist; existing
packages are never overwritten. Input images are validated using the host's
bounded PNG/JPEG decoder before files are created. No Python is required.

| Option | Default | Meaning |
| --- | --- | --- |
| `--width`, `--height` | Width 360; height from image aspect | Unrotated image box, 32..8192 logical pixels; one dimension preserves aspect, two specify a box |
| `--rotation` | 0 | Clockwise angle in degrees, -360..360; negative tilts counterclockwise |
| `--fit` | `contain` | `contain` shows the whole image, `cover` crops to fill, `stretch` distorts to fill |
| `--opacity` | 1 | Whole-image opacity, 0..1, multiplied by PNG alpha |
| `--radius` | 0 | Rounded local image-box clipping, 0..2048 pixels |
| `--input` | `none` | `none` passes through clicks; `all` drags anywhere; `title` drags from the top 42px |
| `--filter` | `bilinear` | `nearest` for pixel art, `bilinear` for general scaling, `best` for Cairo's highest-quality filter |

The surface automatically grows to contain the rotated image box, so tilted
corners are not cut off. If that would exceed 8192px on either axis, reduce the
image box dimensions. The source image is copied without re-encoding.

## Rotate an existing poster

```sh
rs-wwidget --rotate-poster ~/.config/rs-wwidget/widgets/my-poster 25
rs-wwidget restart
```

The angle is absolute, not added to the previous rotation. `0` returns it to
upright. This command centers the image, rotates about its center, and updates
both the frame and manifest dimensions. It preserves image data, opacity,
rounding, image fitting/position, sampling, input mode and manifest positions.
It rewrites TOML formatting/comments. A manifest write failure rolls the frame
back. Rotation is supported for static, decoration-free v2 packages with exactly
one image and no rows; use the general canvas API for custom compositions.

Use `rs-wwidget widget my-poster reload` after rotating/editing an enabled poster,
or enable `watch_files = ["frame.json", "poster.png"]` in its manifest. Without
watching/live reload, restart after changes. New package IDs still need selection
in the main widgets list and a host restart. Saved positions in `layout.ini` take precedence over manifest
`position = [64, 96]` and are constrained to the selected monitor's logical size.
To move a click-through poster, set `input = "all"` in `widget.toml`, restart,
drag, then set `input = "none"` and restart again. Transparent margins of the
rectangular surface are part of the drag area when input is enabled.

## Bundled sample and manual editing

The bundled `poster` contains a working typographic card. Enable it by adding
`"poster"` to `widgets`. To customize it, first make a personal copy:

```sh
mkdir -p ~/.config/rs-wwidget/widgets
cp -R ~/.local/share/rs-wwidget/widgets/poster ~/.config/rs-wwidget/widgets/
cp /absolute/path/to/your-image.png ~/.config/rs-wwidget/widgets/poster/poster.png
```

JPEG also works: change `content.path` in `frame.json` to its filename. Paths
may be absolute, or relative to the package directory. Multiple posters use
separate package directories and matching manifest IDs.

`rotation`, `origin` and `clip_radius` belong to the element, alongside `bounds`
and `opacity`. `position` and `filter` belong inside image `content`:

```json
{
  "bounds": [24, 24, 360, 480],
  "rotation": -5,
  "origin": [0.5, 0.5],
  "clip_radius": 16,
  "content": {
    "type": "image", "path": "poster.png", "fit": "cover",
    "position": [0.5, 0], "filter": "best"
  }
}
```

When editing directly, leave room in the panel for transformed corners; manual
JSON rotation does not resize the surface. Prefer `--rotate-poster` for this.
The rotation command requires the frame file to stay inside the package.

## Validate and preview without a desktop

```sh
rs-wwidget --check-frame ~/.config/rs-wwidget/widgets/my-poster/frame.json
rs-wwidget --check-config
rs-wwidget --preview /tmp/widgets.png
```

See [API v2](../../docs/widget-api-v2.md) for composition and the
[JSON Schema](../../sdk/schema/frame-v2.schema.json) for editor assistance.


Live pinning and placement also work:

```sh
rs-wwidget widget my-poster lock
rs-wwidget widget my-poster move 320 160
rs-wwidget widget my-poster hide
rs-wwidget widget my-poster show
```

These controls persist without changing the image. Use `unlock` or `reset` to
restore drag behavior. See the [framework guide](../../docs/addon-framework.md).
