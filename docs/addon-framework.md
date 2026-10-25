# Addon framework

rs-wwidget hosts trusted local desktop components in two modes. A lightweight
canvas mode keeps Cairo/Pango and the single host event loop. An external mode
supervises programs that create their own Wayland surfaces, including actual
Quickshell/QtQuick configurations. Choose the renderer that fits the UI.

| Capability | Canvas providers | External/QML providers |
| --- | --- | --- |
| Logic | Any executable/script with NDJSON stdout | Any executable; its own runtime/API |
| Display | Text, shapes, bars, graphs, transformed PNG/JPEG; arbitrary custom graphics through image revisions | Renderer-owned graphics, layouts, native controls and installed framework modules |
| Mouse | Enter/leave, motion, buttons, scroll; rotated/rounded hit targets | Native framework input |
| Keyboard | Opt-in physical keys, XKB keymap/modifiers/repeat metadata | Native framework keyboard/text/IME behavior |
| Animations | Provider emits changing frames; repeated frames avoid redraw | Framework timelines/timers/animations |
| Position/layer/pin | Host grid dragging, live CLI, persisted overrides, four Wayland layers | Renderer-owned; metadata supplied; QML example honors it |
| State | SDK atomic JSON state or your own storage | Runtime-owned or supplied state-directory metadata |
| Reload | Explicit live reload or bounded file watching | Host supervision/reload, or native runtime live reload |
| Control | CLI / same-user IPC / custom stdin events | Process lifecycle; use the renderer's own window-control API |
| Dependencies | Rust host; your selected script interpreter/libraries | Selected renderer, e.g. Quickshell/Qt |

The external mode is an extension boundary, not an embedded Qt/Web/JavaScript
engine. A normal application does not automatically become a desktop layer
window: the external program must create those surfaces. The supplied QML
example uses Quickshell's `PanelWindow`. The default host widgets need no Qt. Canvas packages can set `namespace` for
per-widget compositor rules; their grid overlay keeps the host namespace. Text
elements support installed font families (including icon fonts), bold and italic.
External windows do not use the host's click targets, drag grid or layout file.

## Create your first component

```sh
mkdir -p ~/.config/rs-wwidget/widgets
rs-wwidget --init-widget ~/.config/rs-wwidget/widgets/counter
# Alternatively:
rs-wwidget --init-widget ~/.config/rs-wwidget/widgets/js-counter --backend javascript
rs-wwidget --init-widget ~/.config/rs-wwidget/widgets/qml-panel --backend qml
```

Each command creates a new package and never overwrites an existing directory.
Add the ID(s) to the main `widgets` list and restart the host once. The Python
and JavaScript starters handle clicks and a custom `reset` event. The Python
counter persists its value. The QML starter demonstrates animation, text input,
clickable controls and a move/pin toggle. Install Node.js or Quickshell separately
only if selecting those backends. The Python starter uses the installed SDK via
`RS_WWIDGET_SDK`; no pip dependencies are needed.

A provider is ordinary code. It can spawn scripts with its language's process
APIs, query services, draw through another library, and map input to actions.
Keep stdout exclusively for frames in canvas mode. The host captures a bounded
8 KiB stderr tail for debugging. It does not run shell strings implicitly.

See [canvas protocol](widget-api-v2.md), [input events](widget-input.md),
[playground](../widgets/playground/README.md), and
[QML example](../widgets/qml-example/README.md).

## Live control

```sh
rs-wwidget widget list
rs-wwidget widget list 32  # Next page, if next_offset is 32
rs-wwidget widget counter status
rs-wwidget widget counter reload
rs-wwidget widget counter lock
rs-wwidget widget counter unlock
rs-wwidget widget counter hide
rs-wwidget widget counter show
rs-wwidget widget counter move 320 160
rs-wwidget widget counter reset
rs-wwidget widget counter send reset '{}'
rs-wwidget widget counter configure '{"width":420,"layer":"top","locked":true}'
```

- `list` returns `{widgets, total, next_offset}` with at most 32 summaries per page;
  logs are omitted from summaries. Use the returned offset for the next page.
- `status` reports process generation, reload state, errors and stderr. External
  visibility is null: the host does not observe that program's windows.
- `reload` validates a package, stops/reaps only its old process group, and starts
  its replacement without stopping other widgets. Static packages reread their
  frame and invalidate image caches. Legacy command definitions can restart with
  their existing settings; new legacy main-config edits require host restart.
- `lock` / `unlock` override drag policy. Interactivity remains active when locked.
- `hide` removes the host surface while retaining provider state/process; `show`
  recreates it. Hiding is not equivalent to disabling an expensive provider.
- `move` uses logical coordinates, snaps to the 16px grid, constrains to the
  selected monitor and saves `layout.ini`.
- `reset` clears only the live lock/visibility overrides, restoring manifest
  policy. It does not erase saved positions or application state.
- `send NAME JSON` queues a `custom` event for a running interactive canvas
  provider. Delivery is queued, not an application-level acknowledgement.
- `configure JSON` updates a package manifest and queues a reload. Keys are TOML
  fields; null removes a field. Unknown/invalid combinations are rejected before
  writing. ID/version/enabled cannot be changed. Formatting/comments are rewritten.
  A reload-validation failure rolls back the manifest. The subsequent runtime
  can still fail (e.g. a missing interpreter); inspect `status`.

Reload/configure acknowledgements mean accepted, not first-frame completion;
query `provider.reloading` / generations for progress. CLI live control targets
currently known widgets, not arbitrary installed IDs. Enabling a new package or
changing the global monitor/theme/widget selection still uses the main config
and a host restart. Lock/visibility overrides persist in `widget-controls.json`
and take precedence over the manifest until `reset` is called. They survive
manifest reloads. These host surface controls apply to canvas/built-in widgets;
external renderers expose their own geometry and input controls.

## File watching

```toml
watch_files = ["main.py", "settings.json"]
```

Watching is opt-in and limited to 64 exact paths. Paths are relative to the
package unless absolute. The host also watches its manifest and static frame
when watching is enabled. Metadata is checked every 500ms in the existing event
loop; there are no watcher threads or recursive directory scans. Disabled
packages are not watched. Atomic file replacement is supported. The watcher
uses modification time and file length, so edits that deliberately preserve both
are not detected. Directory entries are not recursively watched.

Invalid manifests/static JSON preserve the previous configuration and frame;
errors appear in host logs/status. Correct the file to trigger another attempt.
Valid edits that introduce script/runtime errors use the normal provider restart
path. Add image filenames to `watch_files` if static images should reload too.
After size/style/layer changes, the host recreates just the affected surface and
re-constrains its saved position. Keyboard focus may need to be reacquired.
Quickshell has its own QML watcher; the example does not add a second host watcher.

## Metadata, state and scripting

All selected command providers inherit the host environment, plus:

| Variable | Meaning |
| --- | --- |
| `RS_WWIDGET_ID` | Stable package ID |
| `RS_WWIDGET_RENDER_MODE` | `canvas` or `external` |
| `RS_WWIDGET_PROTOCOL` | Manifest protocol hint |
| `RS_WWIDGET_WIDTH`, `RS_WWIDGET_HEIGHT` | Configured logical dimensions |
| `RS_WWIDGET_X`, `RS_WWIDGET_Y` | Initial manifest position or `[64,96]`; not live layout tracking |
| `RS_WWIDGET_NAMESPACE` | Configured compositor namespace; default `rs-wwidget` |
| `RS_WWIDGET_LAYER` | Configured Wayland layer |
| `RS_WWIDGET_LOCKED` | Manifest pin state (`1` or `0`), not live override tracking |
| `RS_WWIDGET_ASSET_DIR` | Package root |
| `RS_WWIDGET_STATE_DIR` | Per-widget state root; created lazily by the provider |
| `RS_WWIDGET_SDK` | Discovered Python SDK directory, when available |
| `RS_WWIDGET_THEME` | JSON object of resolved `#RRGGBB` host palette colors |
| `RS_WWIDGET_EVENTS` | `1` for interactive stdin, `0` otherwise |
| `RS_WWIDGET_CONTROL_SOCKET` | Same-user host control endpoint |

Use `cwd` and `[env]` in the manifest for script-specific settings. Literal
arguments are never expanded by a shell; `{widget_dir}` is substituted for
package command arguments, cwd and env values. Provided host metadata overrides
same-named user env entries. Main-config palette changes require host restart;
package reloads preserve the current host palette.

The Python SDK includes `StateStore(directory=None)`: `load(default)` and
`save(value)` operate on `state.json`, with a 256 KiB limit and atomic mode-0600
replacement. State paths are per ID. `theme()` returns resolved palette colors.
`control(request)` sends one bounded IPC request to the current host; for example:

```python
from rs_wwidget import StateStore, control
store = StateStore()
settings = store.load({"enabled": True})
store.save(settings)
control({"op": "send", "id": "counter", "name": "reset", "data": {}})
```

IPC uses a mode-0600 Unix datagram socket inside the host state directory, with
16 KiB requests, 64 KiB replies and bounded per-loop request processing. The host never waits
for a control client. It is a local same-user interface, not a network service.
SDK/CLI clients time out and clean up their reply endpoints. Invalid requests
receive errors and do not execute shell commands.

## Limits and renderer ownership

Package manifests are capped at 64 KiB and the main configuration at 1 MiB.
Canvas panels support 32..8192 logical pixels per dimension. Each physical SHM
buffer is capped at 256 MiB; reduce dimensions on high-scale outputs if needed.
Frames remain limited to 64 KiB / 256 elements; image files to 32 MiB; decoded
images to 4096x4096 with a 64 MiB LRU cache. Arbitrary graphics can be emitted as
atomically replaced PNG/JPEG files with a changed revision. Animated image file
formats are not decoded; drive animation with successive frames instead.

Canvas input has bounded nonblocking queues, physical keyboard events and
rounded/rotated hit testing. Native text/IME, video/shaders, complex UI layouts,
multiple native surfaces and richer framework integrations belong in an external
renderer such as Quickshell. The host does not supply every Qt module or enforce
its own geometry limits on those external surfaces. Raw canvas keyboard focus
needs layer-shell v4; older compositors continue with pointer-only input.

## Validation and tests

```sh
rs-wwidget --check-widget ~/.config/rs-wwidget/widgets/counter
rs-wwidget --check-frame ~/.config/rs-wwidget/widgets/poster/frame.json
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
./build.sh
cargo test --locked --test ipc -- --ignored
```

The last command exercises actual Unix sockets. Python protocol tests exercise
a tiny Wayland compositor, including content-click routing, drag separation and
live controls. These checks need normal Linux `/proc` and Unix socket access.
Quickshell is optional and is not launched by the host's canvas test suite;
validate QML renderer behavior on a desktop with the selected runtime installed.
Schemas are also installed under `~/.local/share/rs-wwidget/sdk/schema`, and can
be exported with `--print-widget-schema` / `--print-frame-schema`.
The GitHub Actions workflow runs the native, SDK, schema and protocol suite on
Ubuntu. Editor schemas are in `sdk/schema/`; `jsonschema` is an optional test-only
dependency, not a host or SDK runtime dependency.
