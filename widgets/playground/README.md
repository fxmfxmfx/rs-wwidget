# Interactive playground

Add `"playground"` to `widgets` and restart. Requires Python 3. The installer
ships the standard-library SDK beside the package tree; development uses the
same SDK in the checkout. No third-party Python modules are needed.

- Click ON/OFF or RESET.
- Drag the slider, or scroll over the content to change its value.
- When focused, use physical left/right arrow keys and Space.
- Drag the title strip to move the panel; content clicks do not move it.

Keyboard is explicitly enabled with `keyboard = true` and uses layer-shell v4
on-demand focus. The compositor decides whether to focus a bottom-layer surface.
Set `keyboard = false` if you only need mouse interaction, including on older
compositors. Raw keycodes use Linux evdev numbering; this sample does not decode
text, IME, keymaps or repeat events.

See [interactive API](../../docs/widget-input.md) for event transport, hit testing,
backpressure and writing your own provider. Disabled playground packages do not
start Python. To test the sample without Wayland, pipe version-2 events to its
stdin and inspect the emitted frames on stdout.
