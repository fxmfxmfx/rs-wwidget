# Widget packages and API v1

For images, custom layouts/sizes, transparent decoration-free panels and static
packages without child processes, see [API v2](widget-api-v2.md).
The v1 protocol below remains supported.

rs-wwidget can display its built-in panels and optional widget packages. A
package is a directory containing a `widget.toml` manifest, an executable
command, and optionally package-specific files such as `settings.toml` or a
text data file. Bundled packages are installed but stay stopped until their
names are selected in the main configuration.

## Enable widgets

Set `widgets` in `~/.config/rs-wwidget/config.toml` (or the corresponding
`$XDG_CONFIG_HOME/rs-wwidget/config.toml`) to the names you want:

```toml
widgets = ["clock", "network", "temperatures", "music", "weather", "cpu"]
```

This list replaces the default built-in selection (`clock`, `calendar`, and
`system`). `clock`, `calendar`, and `system` are built-in panel names; the other
names above select optional packages. Package names are unique, stable IDs used
to save panel positions. Add a package name to enable it; remove it to stop it.
All optional packages are disabled in the default configuration and create no
widget process until selected.

Use `rs-wwidget --list-widgets` to see built-in and installed package names.
`--check-config` validates the configuration and package manifests. Restart the
host after changing the `widgets` list or package manifests.

## Package discovery and installation

For each selected package name, rs-wwidget checks these roots in order and uses
the first matching `<root>/<name>/widget.toml`:

1. `widgets/` beside the explicitly selected config file (`--config PATH`).
2. `$XDG_CONFIG_HOME/rs-wwidget/widgets/` (normally `~/.config/rs-wwidget/widgets/`).
3. `$XDG_DATA_HOME/rs-wwidget/widgets/` (normally `~/.local/share/rs-wwidget/widgets/`).
4. `widgets/` beside the running executable.
5. `../share/rs-wwidget/widgets/` beside the running executable.
6. `widgets/` in the Cargo source tree when running a development build.

An earlier package with an invalid manifest is an error; discovery does not
silently continue to a lower-priority copy. Use `./build.sh` to build the host
and bundled native executables, then `./install.sh` to install the host and
packages under `~/.local/share/rs-wwidget/widgets/`. The installer preserves
existing package `settings.toml` and `.txt` files during reinstall. Keep
user-created package directories and the main configuration when removing the
application.

## Author a package

Create a directory such as `~/.config/rs-wwidget/widgets/hello/` with a
`widget.toml` manifest. The manifest version is `1`; its `id` must match the
directory name. `command` is an argument array executed directly without a
shell. A leading `./` or `../` executable path is resolved relative to the
package directory. The `{widget_dir}` token is replaced with that directory's
absolute path in every argument.

```toml
version = 1
id = "hello"
command = ["python3", "{widget_dir}/main.py"]
height = 178
restart_seconds = 30
timeout_seconds = 60
```

The manifest has no `enabled` field: selecting `"hello"` in the main
configuration enables it. `height` and optional `width` must be 32–8192 pixels; restart and timeout
values must be 1–86,400 seconds. The default widget content width is 300
pixels; v2 supports custom dimensions. Keep package settings and data files beside `widget.toml`; resolve
their paths from `{widget_dir}` or your program's package directory. Commands
inherit the host environment and working directory. Arguments do not expand
`~`, environment variables, or shell operators.

The minimal Python SDK is [`sdk/python/rs_wwidget.py`](../sdk/python/rs_wwidget.py):

```python
import time
from rs_wwidget import Row, emit

while True:
    emit("Hello", [Row("Status", "Ready"), Row("Load", "42%", fraction=0.42)])
    time.sleep(5)
```

Place the SDK beside the script or add its directory to `PYTHONPATH`. Python
widgets are convenient, but keep an interpreter resident for their lifetime;
historical samples measured about 14 MiB RSS for Python versus about 1.7 MiB
for a small native monitor. Native processes are a better fit for frequently
running, resource-sensitive widgets. See [bundled packages](../widgets/README.md)
for package settings and control commands.

## Frame protocol

Widgets are persistent processes. They write newline-delimited UTF-8 JSON to
stdout and flush each complete frame. An unchanged frame is a valid heartbeat.
Keep logs on stderr; the host retains a bounded tail, accessible with
`rs-wwidget widget ID status`. The host sets
`RS_WWIDGET_PROTOCOL=1` in the child environment.

```json
{"version":1,"title":"Example","rows":[{"label":"CPU","value":"42%","fraction":0.42,"history":[0.1,0.4]}]}
```

`version` must be `1`. `title` is required and limited to 128 characters.
`rows` is required and has at most 12 objects. Row `label` and `value` are
strings up to 512 characters; omitted values default to empty strings. An empty
label renders the value across the content width. Optional `fraction` is a
finite number from 0 through 1. Optional `history` has at most 120 finite
numbers from 0 through 1, ordered oldest to newest. The graph uses a fixed 0–1
vertical scale. A frame line is limited to 65,536 bytes, not counting its
newline. Text is rendered literally and truncated when it does not fit.

The host waits for complete newline-terminated frames. A crash, invalid frame,
or timeout stops the process and schedules a restart after
`restart_seconds`. A frame arriving within `timeout_seconds` resets the
heartbeat interval. On shutdown, the host terminates the widget process group and
cleans it up. Widgets are not sandboxed; only install packages and commands
you trust. Version 1 carries display data only. Interactive actions such as
music controls are available through the separate command-line interface
documented in the music package README; they are not clickable panel actions.

The panel uses the host palette, font, opacity, and display scale. Rows are
stacked below the title and rows that do not fit the configured height are
omitted. Preview mode shows placeholders and does not execute widget commands.

## Legacy plugin configuration

The older `[[plugins]]` configuration remains supported for advanced setups.
It can declare a command inline with an `enabled` flag. Prefer a package and
the `widgets` name list for new widgets so settings, data, executable, and
display metadata stay together.

```toml
[[plugins]]
id = "legacy-example"
command = ["/absolute/path/to/widget"]
enabled = false
height = 178
restart_seconds = 30
timeout_seconds = 60
```

Legacy IDs must be unique ASCII letters, digits, `_`, or `-`, and cannot
collide with built-in panel names. Commands are executed directly without a
shell. Legacy entries remain disabled unless `enabled = true`.

## Historical resource samples

The following measurements came from earlier builds and are individual samples,
not guarantees for current package combinations. The host used two threads.

| Earlier configuration | RSS (KiB) | PSS (KiB) | Host threads | Child processes |
| --- | ---: | ---: | ---: | ---: |
| Before external-widget support | 14,760 | 3,863 | 2 | 0 |
| Default plus legacy plugin support | 14,672 | 3,952 | 2 | 0 |
| Default plus disabled legacy plugin definition | 14,752 | 3,909 | 2 | 0 |

One older enabled Python fixture used 14,156 KiB RSS / 8,800 KiB PSS; the host
still had two threads. A native CPU monitor sample used 1,740 KiB RSS / 133 KiB
PSS. Measurements depend on the process, compositor, and machine. Optional
packages are not started unless selected.
