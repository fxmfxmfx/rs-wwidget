# Poster

A static PNG/JPEG on the desktop: no title, frame, panel background, child
process, timer, or pointer interception. Transparent PNG pixels stay transparent.
Windows remain above it on the normal rs-wwidget bottom layer.

After building and installing, make a personal copy so package updates cannot
replace your image or configuration:

```sh
mkdir -p ~/.config/rs-wwidget/widgets
cp -R ~/.local/share/rs-wwidget/widgets/poster ~/.config/rs-wwidget/widgets/
cp /absolute/path/to/your-image.png ~/.config/rs-wwidget/widgets/poster/poster.png
```

Add `"poster"` to the `widgets` list in `~/.config/rs-wwidget/config.toml`, then
run `rs-wwidget restart`. The included typographic card is a working sample.
JPEG works too: change `content.path` in `frame.json` to the JPEG filename.
Absolute image paths also work; relative paths start at the package directory.
No downloads, shell expansion, or animated images are involved.

In `widget.toml`, change `width`, `height` (32..2048 logical pixels), and
`position = [64, 96]`. Positions snap to the 16px grid and are constrained to
the selected output. Previously saved positions in `layout.ini` take precedence.
To move it with the mouse, set `input = "all"`, restart, drag it, then set
`input = "none"` and restart again. `input = "title"` makes only the top 42px
a drag handle, without adding visible decoration.

In `frame.json`, keep the element's `[x, y, width, height]` in sync with your
chosen panel size. `contain` preserves the full picture with transparent margins;
`cover` fills and crops; `stretch` fills without preserving aspect ratio.
An element's optional `opacity` is 0..1. For several posters, copy the package
under new directory names, change each manifest's `id`, and enable those IDs.
Static files are loaded once; restart after editing them or changing an image.

Headless check and preview (does not launch widget commands):

```sh
rs-wwidget --check-config
rs-wwidget --preview /tmp/widgets.png
```

See [API v2](../../docs/widget-api-v2.md) for layering text, shapes and charts.
