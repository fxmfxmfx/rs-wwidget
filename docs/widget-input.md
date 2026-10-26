# Interactive widgets: scripts and bidirectional events

Any executable can be a widget provider: Rust, Python, JavaScript, shell, or a
program that renders its own images. It owns its logic, subprocesses and actions.
The host owns Wayland surfaces, display lists, dragging and process lifetime.
Providers are trusted local programs, with the user's ordinary permissions.
No shell is implicitly inserted into manifest commands.

## Manifest

```toml
version = 2
id = "controls"
protocol = 2
command = ["python3", "{widget_dir}/main.py"]
width = 360
height = 280
input = "interactive"
keyboard = true
drag_height = 42
locked = false
layer = "bottom"
cwd = "{widget_dir}"

[env]
MY_SETTINGS = "{widget_dir}/settings.json"
```

Only `input = "interactive"` opens a provider stdin event pipe. It requires
`protocol = 2` and a command source. Static packages do not consume input.
`keyboard` defaults to false; explicitly enabling it requires interactive input
and layer-shell v4 for on-demand keyboard focus. The compositor controls focus;
the host never requests exclusive focus or installs global shortcuts. Older
layer-shell versions continue with pointer input only.

`drag_height` reserves a top strip for left-button dragging: default 42 for
cards, 0 for decoration-free panels. Content buttons never drag the panel.
`locked = true` disables dragging. Locked interactive panels keep receiving
content input; locked title/all drag-only panels become click-through.
`layer` accepts `background`, `bottom` (default), `top`, `overlay`.
The drag overlay remains on the host's top layer. Panel positions are saved by ID.

`cwd` optionally selects the process working directory; relative package paths
resolve against its directory. `env` adds/overrides environment variables.
Package cwd and env values support `{widget_dir}`. Reserved `RS_WWIDGET_*`
metadata overrides user env values. Disabled widgets create no process/pipes.

## Wire protocol

Provider **stdout** stays newline-delimited v1/v2 display frames. Provider
**stdin** receives newline-delimited UTF-8 JSON events, each with `version: 2`.
Flush complete frames and send an unchanged heartbeat within `timeout_seconds`,
even when the user is idle. Logs belong on stderr, not stdout.

Initial event after each process start:

```json
{"version":2,"type":"init","width":360,"height":280,"keyboard":true}
```

The `init` event also includes the resolved `theme` palette. The host sets `RS_WWIDGET_EVENTS=1` for interactive providers, `0` otherwise,
plus the existing geometry/protocol/asset-directory variables.
`RS_WWIDGET_STATE_DIR` names a per-widget persistent-state directory under
`$XDG_STATE_HOME/rs-wwidget/widgets/ID` (the host does not create it automatically).

Pointer events:

```json
{"version":2,"type":"pointer","kind":"button","x":42,"y":60,
 "target":"toggle","local":[18,4],"button":272,"pressed":true,
 "axis":null,"value":null,"drag_handle":false}
```

| Field | Meaning |
| --- | --- |
| `kind` | `enter`, `leave`, `motion`, `button`, `scroll` |
| `x`, `y` | Panel-local logical pixels; can be outside the panel during an implicit pointer grab |
| `target` | Topmost hit element ID, or null |
| `local` | Pointer in that element's unrotated local coordinates, or null |
| `button` / `pressed` | Linux button code and press/release, only for `button`; left=272, right=273, middle=274 |
| `axis` / `value` | `vertical`/`horizontal` and Wayland axis distance for `scroll`; not normalized wheel ticks |
| `drag_handle` | Pointer is in the reserved host drag strip; no content target is returned there |

Set `id` on a canvas element to make it a target. IDs are unique across a frame,
1..64 ASCII letters, digits, `_` or `-`. Later elements are on top. Labels or
other elements without IDs do not block an underlying target. Hit testing
accounts for rotation, origin, rounded clipping and zero opacity; image pixel
alpha is not a hit mask. A leave event has no target. No events are sent while
the host is dragging. The host consumes left-button dragging in its strip.
For sliders, track the pressed button and keep using panel coordinates when the
pointer leaves the element's bounds.

Keyboard events (only for `keyboard = true` and compositor focus):

- `keyboard_focus`: `focused` boolean and current pressed `keys` (Linux evdev).
- `key`: evdev `key`, `pressed`, compositor millisecond `time`.
- `modifiers`: XKB `depressed`, `latched`, `locked`, `group` masks.
- `keymap`: `format = "xkb_v1"`, UTF-8 `keymap` text, at most 128 KiB before JSON encoding.
- `repeat_info`: compositor repeat `rate` and `delay`; rate 0 disables repeat.

On focus, keymap/repeat/modifier metadata precedes the focus event. Restarted
focused providers receive fresh metadata and a focus event with an empty key
list; reset local pressed-state tracking. Keycodes are physical, not text or
keysyms. Use the supplied XKB keymap with a keyboard library for layout-aware
text. The host does not synthesize repeat, text-input/IME or global hotkeys.

## Nonblocking input and failures

The host never waits for a provider to read stdin. Writes are nonblocking and
integrated with the existing poll loop. The pending input queue is bounded to
128 events / 256 KiB; consecutive unsent motion events coalesce to the latest
one. Already-partially-written events are preserved. Button/key/focus events
are never silently dropped: overflow or a broken stdin pipe stops/restarts that
provider through the normal failure path. Other widgets keep running. Queue
state is cleared on restart. Old v1 providers still get null stdin.

## Python example

```python
from rs_wwidget import Rect, Text, emit_canvas, events

count = 0

def draw():
    emit_canvas([
        Rect((24, 56, 200, 48), color="accent", radius=8, id="add"),
        Text(f"Count: {count}", (24, 66, 200, 28), align="center")
    ], title="Counter")

draw()
for event in events(timeout=5):
    if (event and event["type"] == "pointer" and event["kind"] == "button"
            and event["pressed"] and event["button"] == 272 and event["target"] == "add"):
        count += 1
    draw()  # Also a heartbeat after each idle timeout.
```

`events()` yields event dictionaries, and `None` on idle timeouts. It terminates
on EOF. For custom loops use `EventStream().poll(timeout)` and check `.closed`.
The reader handles fragmented/multiple buffered lines without relying on text
stream buffering. It is single-consumer and adds no threads/dependencies.
All primitive SDK helpers accept `id=`. Invalid/duplicate IDs fail before output.
The SDK is installed at `~/.local/share/rs-wwidget/sdk/python/rs_wwidget.py`.

For a full interactive example, enable [playground](../widgets/playground/README.md).
For arbitrary graphics, render PNGs with your own library and send image elements
with an incremented `revision`, keeping updates atomic. This can support custom
charts, diagrams and animated scenes without expanding the host's primitive API.


For live lock/show/move/reload/configure commands, custom IPC events, file watching,
state storage and non-canvas/native renderer packages, see the
[framework guide](addon-framework.md). A custom event is
`{"version":2,"type":"custom","name":"reset","data":{}}`.

Before hiding, pinning/resetting or otherwise replacing a focused surface, the host sends pointer `leave` and keyboard focus-loss events to the still-running provider. Providers should cancel held controls/gestures on these events. This does not depend on the compositor sending a late event for the destroyed surface.
