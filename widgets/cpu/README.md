# CPU widget

The native `cpu-monitor` reads `/proc/stat` for aggregate CPU use and
`/proc/loadavg` and `/proc/uptime` for system load and uptime. It samples once
per second, shows `Sampling…` until it has two CPU samples, and retains up to
two minutes of graph history. CPU usage is aggregate across cores; it is not a
per-core breakdown. Missing or reset counters display as unavailable.
