# rs-yambar addon

This package runs the original **rs-yambar** through rs-wwidget's external
renderer API. It keeps the real bar's layout, layer-shell surfaces, Mango tags,
window-title tracking, script modules and mouse/wheel actions. It is not a
canvas reimplementation or a screenshot of the bar.

The supplied config and two scripts come from
[fxmfxmfx/mango-dotfiles](https://github.com/fxmfxmfx/mango-dotfiles), commit
`3716288490428b85db0bdddfdc35d0533be5dee9`. The only script-path changes make them
package-local through `RS_WWIDGET_ASSET_DIR`. The original MIT notice is in
[COPYING](COPYING). Compatibility is checked against
[rs-yambar](https://github.com/fxmfxmfx/rs-yambar), commit
`ba4015500d4fc3965ffb4e6bf07f973f43ef942e`.

## Enable

1. Build/install rs-yambar from source for your distribution. Put its executable
   on `PATH` or at `~/.local/bin/rs-yambar`.
2. Build/install this rs-wwidget branch normally.
3. Add `"rs-yambar"` to the main `widgets` list and restart rs-wwidget.
4. Remove the separate rs-yambar autostart command to avoid two bars. Keep the
   rs-wwidget autostart command.

The bundled bar is 28px tall, with eight workspace tags, an active window title,
keyboard layout, memory, PipeWire volume and clock. It retains `output = "DP-1"`
for the workspace module from the original config: change it to your connector.
Its font stack requires MxPlus IBM VGA and Symbols Nerd Font for matching icons.
Mango modules need `mmsg`; audio/actions need `wpctl`. These are optional external
programs, not dependencies added to the rs-wwidget host.

For edits, first copy the installed package into
`~/.config/rs-wwidget/widgets/rs-yambar/`, then edit its `config.toml`. Reload with:

```sh
rs-wwidget widget rs-yambar reload
rs-wwidget widget rs-yambar status
```

To use your live config unchanged, add this table to the personal package's
`widget.toml`, replacing the absolute paths for your account:

```toml
[env]
RS_YAMBAR_BINARY = "/home/you/.local/bin/rs-yambar"
RS_YAMBAR_CONFIG = "/home/you/.config/rs-yambar/config.toml"
```

No `~` expansion occurs in manifest env values. Using your live config also keeps
its original script paths. A `watch_files` list can opt into automatic reload;
it is disabled by default. The original shell click-command semantics remain.

## Resources and ownership

The small shell launcher uses `exec`, so it leaves **no wrapper process**. The
host supervises the original bar process group, captures bounded diagnostics,
and reloads/stops it. Disabled packages do not run.

rs-yambar remains a separate process with its original libraries, module
threads, scripts and resource costs. This integration does not reduce those
costs or embed the bar in the host's event loop. Its placement, exclusive zone,
input and monitor selection are renderer-owned; host `move/lock/hide/show`
commands are not supported for external renderers. Configure those in rs-yambar.
Host theme metadata does not overwrite the bar's palette.

## Tests without a desktop

```sh
RS_WWIDGET_ASSET_DIR="$PWD/widgets/rs-yambar" sh widgets/rs-yambar/run.sh --validate
python3 -m unittest test_yambar_addon -v
```

Tests check launcher process replacement, paths containing spaces, the Mango
workspace command and keyboard script with a fake `mmsg`. A separate CI job
builds the pinned original rs-yambar and validates the actual bundled config.
This does not prove real Mango/PipeWire/display behavior on every compositor.
