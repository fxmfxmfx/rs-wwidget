# Bundled widget packages

Optional packages live beside their executable, manifest, settings, and data
files. They are installed under `~/.local/share/rs-wwidget/widgets/` and remain
inactive until selected by name in the main configuration. For example:

```toml
widgets = ["clock", "network", "temperatures", "music", "weather", "cpu"]
```

This selects the built-in clock and each listed package. The default config
continues to select `clock`, `calendar`, and `system`; bundled packages do not
start by default. Use `rs-wwidget --list-widgets` to see installed names.
Package details, controls, and local settings are described below.

| Name | What it shows | Package-specific notes |
| --- | --- | --- |
| `qml-example` | Real Quickshell/QtQuick external renderer (optional) | [Usage](qml-example/README.md) |
| `playground` | Interactive buttons/slider and keyboard reference | [Usage](playground/README.md) |
| `poster` | Static PNG/JPEG without decoration or a provider | [Setup](poster/README.md) |
| `network` | Interface receive/transmit rates | [Settings](network/README.md) |
| `temperatures` | CPU/GPU temperatures and supported fan speeds | [Settings](temperatures/README.md) |
| `music` | MPRIS player status and playback controls | [Controls](music/README.md) |
| `weather` | Current weather and hourly rain chance | [Location and privacy](weather/README.md) |
| `cpu` | CPU activity, load averages, and uptime | [Details](cpu/README.md) |

## Build and install

Run `./build.sh` to build rs-wwidget, the shared `widgets/widget-provider`, the
native CPU monitor at `widgets/cpu/cpu-monitor`, and the music MPRIS program at
`widgets/music/music`. `./install.sh` copies the built application and package
files into the user data directory. It keeps existing package `settings.toml`
and `.txt` files when reinstalling, so local choices and data survive an
upgrade.

The native build needs a C compiler and `pkg-config`; music also needs the
`gio-2.0` development files. The application uses Rust with direct Wayland,
Cairo and Pango; the project README lists build dependencies. Weather uses
`curl` when that package is enabled. It makes HTTPS requests to ipwho.is for
IP-based location and Open-Meteo for weather. Set both coordinates in its
`settings.toml` to skip the geolocation request. No widget needs `playerctl`.

These packages use Linux interfaces and Wayland-focused hosting. They do not
promise cross-platform operation.

## Measured resources

One development-machine sample of idle native providers (not their host's panel
buffers) measured the following. Music used a private session bus. Weather used
fixture HTTP responses; real `curl` requests add a short-lived process during
refresh. RSS includes shared libraries, so adding each RSS overstates physical
memory shared by multiple widgets. Measurements vary by system and activity.

| Provider | RSS (KiB) | PSS (KiB) | Threads |
| --- | ---: | ---: | ---: |
| cpu | 1,772 | 137 | 1 |
| network | 2,556 | 852 | 1 |
| temperatures | 2,540 | 879 | 1 |
| music | 5,572 | 593 | 4 |
| weather | 2,640 | 849 | 1 |

The host with its default three panels and no optional packages measured
14,788 KiB RSS / 4,041 KiB PSS and two threads. No widget child processes
existed. In the same comparison a pre-plugin executable measured 14,340 KiB
RSS / 3,811 KiB PSS, also two threads; these are individual samples rather
than an exact memory guarantee. Disabled packages allocate no process or
rendering buffers.

## Custom packages

Create a package directory with `widget.toml`, then add its name to the
`widgets` list. The manifest command is an argument array; commands are not
run through a shell. The host substitutes `{widget_dir}` with the package's
absolute directory. See the [package authoring and frame protocol guide](../docs/widget-api.md)
for discovery order, manifest fields, process lifecycle, frame limits, and the
Python SDK.
