<img width="337" height="687" alt="image" src="https://github.com/user-attachments/assets/6d5160ff-ea69-4b98-965d-3cab66c34e12" />
<img width="600" height="61" alt="image" src="https://github.com/user-attachments/assets/a48fc88c-e0b9-4ead-93d3-4813d8c3be8f" />

# rs-wwidget

Desktop clock, calendar, and system metrics in the Gruvbox Material palette.
Written in Rust, the application uses the Wayland layer-shell protocol directly.
Cairo/Pango preserve the previous C version's rendering: dimensions, opacity,
widget labels, and the **MxPlus IBM VGA 8x16** font. Install the font separately;
Pango selects a fallback if it is unavailable.

- One process and one event loop, with no application worker threads or GTK.
  Pango creates one font-matching thread; the verified environment had 2 threads total.
- CPU, memory, and home filesystem metrics are polled every 5 seconds.
- Only changed panels are redrawn. The clock updates when the minute changes;
  the calendar updates when the date changes.
- Drag each panel by its title bar; positions snap to a 16 px grid.
- Clicks pass through panels outside their title bars. The grid exists only while dragging.

## Build and install

Requires Linux, Rust **1.92+** with Cargo, `pkg-config`, and Cairo and Pango
development headers. Building the native examples requires a C compiler and
GIO development headers (`gio-2.0` via `pkg-config`); checks also require Python 3 and
`desktop-file-validate`. Running requires a Wayland compositor with wlr-layer-shell
support, such as MangoWM. GTK and gtk-layer-shell are no longer required.

Arch Linux / CachyOS:

```sh
sudo pacman -S --needed rust base-devel pkgconf cairo pango python desktop-file-utils
```

Debian / Ubuntu (install Rust 1.92+ separately, for example with rustup):

```sh
sudo apt install build-essential pkg-config libcairo2-dev libpango1.0-dev libglib2.0-dev python3 desktop-file-utils
```

From the project directory:

```sh
./build.sh
./install.sh
~/.local/bin/rs-wwidget start
```

`build.sh` builds an optimized binary and runs Rust checks and install/control
checks in a temporary home directory. The `rs-wwidget` binary is created in the
project root. For development, use `cargo run -- --run`; release builds use less
memory. Installation without sudo places the binary in `~/.local/bin`, the
desktop entry in `~/.local/share/applications`, and the MangoWM include file in
`~/.local/share/rs-wwidget/mango.conf`. The running version is replaced atomically;
run `rs-wwidget restart` after an update.

If a restart reports `Process exited before creating widgets` and the session
log is empty, another instance may still hold the widget lock. This can happen
when the running process was launched from a checkout while the restart command
uses the installed binary; current process detection may not recognize the
checkout executable. Confirm that the old process has exited before retrying.

## Color, language, and monitor configuration

The path is `$XDG_CONFIG_HOME/rs-wwidget/config.toml`, defaulting to
`~/.config/rs-wwidget/config.toml`. The installer creates it from
[config.example.toml](config.example.toml), preserving any existing user file.
The same defaults apply when no configuration file exists.

[examples/config.toml](examples/config.toml) contains the current desktop preset: clock and system panels on `DP-1`, an opaque dark background, and warm yellow accents. Change `monitor` to your connector name before using it.

```toml
monitor = "auto"
widgets = ["clock", "calendar", "system"]
language = "en"
background_opacity = 0.96
font_family = "MxPlus IBM VGA 8x16"

[colors]
background = "#282828"
foreground = "#d4be98"
muted = "#a89984"
accent = "#e78a4e"
green = "#a9b665"
border = "#504945"
bar_background = "#3c3836"
grid_background = "#1b1b1b"
```

`widgets` lists enabled built-in and installed widget names.
Omitting it enables all three; `widgets = ["clock", "system"]` hides the calendar,
and `widgets = []` hides every panel. Unknown names and duplicates are errors.
The list does not change panel order or saved positions. Restart after editing.
Previews and rendering benchmarks also respect this list.

## Optional and custom widgets

Enable optional widgets by adding their names to the same list:

```toml
widgets = ["clock", "system", "network", "temperatures", "music", "weather"]
```

| Name | Content |
| --- | --- |
| `poster` | Static PNG/JPEG desktop poster, no provider process |
| `cpu` | CPU utilization and two-minute history, load, uptime |
| `network` | RX/TX speed, active interface and traffic history |
| `temperatures` | CPU/GPU maximum temperatures and fan speeds |
| `music` | MPRIS artist, track and playback state |
| `weather` | IP-based approximate location, weather and rain probability |

The [bundled packages](widgets/README.md) are also examples for authors. Each
package owns its `widget.toml` metadata and optional `settings.toml`; the main
configuration only needs names. Use `rs-wwidget --list-widgets` to list available
packages. Installation places them in `~/.local/share/rs-wwidget/widgets` and
preserves their settings and text data on updates. Development uses `widgets/`
next to the executable. See the [widget API](docs/widget-api.md) for custom
packages, the JSON protocol and the optional [Python SDK](sdk/python/rs_wwidget.py).

The shipped providers are native programs. Weather additionally requires `curl`
and queries ipwho.is and Open-Meteo only while enabled; explicit coordinates can
override IP location in its settings. Music uses GIO/MPRIS without `playerctl`.
Python remains supported for user plugins, with its interpreter's memory cost.

Optional widgets are disabled by default. Disabled packages create no processes,
runtime buffers, surfaces or scheduled work and are not scanned on the default path.
The host keeps its single event loop and adds no threads or runtime dependencies.
Enabled plugins pay for their own process and panel buffers. Host RSS/PSS in
`status.json` does not include child processes; count those separately.

Panels support text, label/value rows, progress bars and small history graphs.
[API v2](docs/widget-api-v2.md) adds free-layout text, PNG/JPEG images, rounded
rectangles, lines, bars and graphs, custom sizes, decoration/input modes and
static JSON packages without child processes. Elements support rotation about a
custom pivot, rounded clipping, image positioning/sampling, and bold text.
Create a complete image package with `--create-poster IMAGE PACKAGE_DIR --rotation -12`,
change its angle with `--rotate-poster PACKAGE_DIR 25`, and validate standalone
JSON with `--check-frame FILE`. See the [poster setup](widgets/poster/README.md)
and [editor schema](sdk/schema/frame-v2.schema.json).
The host handles themes, scaling, dragging and saved positions by plugin ID.
Slow or broken plugins are stopped and retried without blocking the event loop.
Config checking, monitor listing, previews and benchmarks never launch plugins;
previews show placeholder panels. Restart after editing the configuration.
Legacy `[[plugins]]` definitions remain supported for existing configurations.

## Additional appearance settings

Set only the options you need. Colors use the `#RRGGBB` format; unknown options
and invalid values are errors. `language` accepts `"en"` or `"ru"` and defaults
to `"en"`; it changes widget text only. CLI output and logs are always in English.
`background_opacity` sets opacity for panel backgrounds only: `0.0` is fully
transparent and `1.0` is opaque. It defaults to `0.96`, as in the previous
version. Text, borders, and metric bars remain opaque. The drag grid retains its
existing opacity.
`font_family` selects the font family; label and clock sizes stay the same. The
[themes/rs-yambar.toml](themes/rs-yambar.toml) preset matches rs-yambar's dark
palette with a light yellow accent and the `MxPlus IBM VGA 8x16 MidTilde` font.

```sh
rs-wwidget --list-monitors
rs-wwidget --check-config
rs-wwidget --config /path/to/config.toml start
```

To select a specific display, use a connector name from the list, for example
`monitor = "DP-1"`. `auto` selects the first announced monitor; Wayland has no
universal way to identify the primary display. If an explicitly selected
monitor is unavailable at startup, the application reports an error. If the
active display is disconnected, the application moves to an available display
and returns when the selected one reconnects. Panel positions are constrained
to the selected monitor's logical dimensions, accounting for its scale. Restart
the application after changing the configuration.

## Layer and MangoWM rules

Panels use **`BOTTOM`**; the drag grid uses **`TOP`**.
Both use the namespace **`rs-wwidget`**, called `layer_name` in MangoWM.
The application does not request keyboard focus or reserve layout space.

```ini
layerrule=noanim:1,noblur:1,noshadow:1,layer_name:rs-wwidget
```

For autostart, add a line with the absolute path to `~/.config/mango/config.conf`:

```ini
source = /home/USERNAME/.local/share/rs-wwidget/mango.conf
```

This included file already contains the autostart entry and layer rule; do not
duplicate them. MangoWM 0.17 uses the `layerrule` and `noanim` syntax shown above.
If rules are stored in `~/.config/mango/conf.d/60-rules.conf`, ensure the rule
uses the actual namespace **`rs-wwidget`**. After changing it, run
`mmsg dispatch reload_config` and `rs-wwidget restart` to recreate the surfaces.

## Process control and state

```sh
rs-wwidget start
rs-wwidget stop
rs-wwidget restart
rs-wwidget toggle
rs-wwidget status
rs-wwidget remove
```

Running with no arguments or with `--run` stays in the foreground.
The `start` command detaches the process and writes errors to `session.log`.
An instance lock prevents duplicate launches. `remove` deletes installed files
and state while preserving the user's `config.toml`.

The lock records its owner's PID, process start time and executable path.
Process control verifies the actual held flock, so the installed command can
restart a checkout-launched process even after an atomic binary update or loss
of `status.json`. It uses Linux pidfds to keep a reused PID from receiving the
shutdown signal. Shutdown waits for the lock to be released before replacement.
When a process is outside the caller's visible process session, the controller
reports that explicitly and keeps the previous log; run the command from the
same desktop terminal. This also applies to `desktop-theme` integrations that
apply colors and then call `rs-wwidget restart`.

Panel positions are stored in `$XDG_STATE_HOME/rs-wwidget/layout.ini` (default:
`~/.local/state/rs-wwidget/layout.ini`); the log and `status.json` are alongside
it. Status includes RSS/PSS, thread count, monitor, drag activity, and accumulated
CPU time. Measurements update after the first 5 seconds, then once per minute.
A second click cancels a drag; after 30 seconds without movement, recovery handles
a missed button release.

When switching from the C version, stop it first:
`~/.local/bin/desktop-widgets stop`, then remove the old autostart line from MangoWM.
Then start `rs-wwidget`. If the new `layout.ini` does not exist, positions are
read from the previous `desktop-widgets` directory. Old installed files are not
removed automatically.

## Checks and resource usage

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
./build.sh --benchmark
./rs-wwidget --preview /tmp/widgets.png
./rs-wwidget --preview /tmp/wallpaper-widgets.png /path/to/wallpaper.jpg
./rs-wwidget --preview-grid /tmp/grid.png
```

Preview does not require Wayland. PNG and JPEG wallpapers are supported.
Checks compare pixels with the saved C renderer at 1× and 2× scale, including a
sequence of partial grid frames. That reference renderer is test-only. The host
is Rust; the separate native CPU and music providers are written in C.

An additional check exercises real protocols in a separate headless MangoWM
(requires `mango` and `grim`):

```sh
cargo build --example test_pointer
python3 tests/wayland_smoke.py
```

It checks two monitors, dragging, cancellation, 30-second recovery, thread count,
and clean shutdown. It does not use the current graphical session. For architecture
and measurements, see [docs/optimization.md](docs/optimization.md).
