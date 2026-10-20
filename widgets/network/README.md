# Network widget

Shows receive and transmit rates for the configured interface. In
`settings.toml`, `interface = "auto"` prefers the Linux `/proc` default route
and falls back to the first non-loopback interface. Set a device name such as
`"wlan0"` to choose one directly. `poll_seconds` controls sampling (1–60);
`scale_mib_per_second` sets the graph's rate scale. The optional `proc_root`
setting is intended for fixtures and custom proc filesystem mounts.
