# Focus timer — native Rust canvas addon

Enable `"focus"` in the main `widgets` list after building/installing. The timer
uses the bundled native `widget-provider`, with no Python/Node/Qt runtime, worker
thread or independent renderer. It emits v2 text, buttons and a progress bar;
rs-wwidget handles drawing, input, dragging and pinning.

- Click START/PAUSE or press Space; RESET or R restores the duration.
- Buttons, left/right arrow keys and scrolling adjust the duration by a minute.
  Adjusting pauses and resets the countdown.
- Edit `settings.toml` (`seconds = 1500`) and reload for a new default.
- Monotonic time preserves fractional seconds across pauses; zero stops ticking.
- No sound, notification, network request or state file is created. Reload starts
  a fresh session. Running emits once per second; paused emits 5s heartbeats.

```sh
rs-wwidget widget focus send set '{"seconds":300}'
rs-wwidget widget focus send start '{}'
rs-wwidget widget focus send pause '{}'
rs-wwidget widget focus send reset '{}'
rs-wwidget widget focus lock
```

Provider input is bounded and polled in one thread. Fragmented lines and buffered
multiple events are handled without blocking the process on a partial line.
Source: `src/widget_plugins/focus.rs` and `events.rs`. The native provider is an
optional child process; its memory is additional to the host.
