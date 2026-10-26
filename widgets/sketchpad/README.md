# Vector sketchpad — interactive Python addon

Enable `"sketchpad"` in the main `widgets` list. Requires Python 3 with no pip
packages. Drag the top 42px to move the widget; draw on the paper below.

- Left-button strokes use normalized polyline points, without raster images.
- COLOR cycles the palette; scroll over paper changes pen width.
- UNDO / Backspace removes a stroke; CLEAR / C clears all.
- `send undo '{}'` and `send clear '{}'` also work through live control.
- Committed strokes persist in the SDK's per-widget `state.json`; release,
  leaving the surface or losing keyboard focus commits the current stroke.
- At most 24 strokes of 24 points are kept. Long strokes split at that boundary;
  the oldest segments expire. Frames stay within the host's 64 KiB limit.
- State writes happen on commit/control actions, not every mouse movement.
  No animation timer, watcher, image cache or extra provider thread is used.

The Python interpreter is an optional additional process. Paused/idle heartbeats
are every 5 seconds; unchanged frames do not trigger host redraws.
