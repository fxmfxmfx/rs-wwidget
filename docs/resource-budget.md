# Resource budget

The default host remains a single event loop, with no new worker threads, async
runtime, embedded JavaScript/Qt engine or interpreter. Libraries such as Pango
may create their own threads. Optional addon processes must be counted separately.

## Automatic comparison

CI builds the current `main` and this branch with the same Rust toolchain and
runs both with **clock + calendar**, an isolated home, the same font, and a tiny
Wayland server exposing one 1920x1080 scale-1 output. The test runner/compositor
is outside the measured process. `/proc` reports RSS, PSS, threads, child processes
and idle CPU. Three runs use five samples each after a one-second warmup.

```sh
python3 tools/resource_probe.py \
  --baseline /path/to/main/release/rs-wwidget \
  --candidate ./rs-wwidget --output resource-report.json
```

The check rejects new host threads, any children for this built-in-only scenario,
PSS growth over 1024 KiB, and idle CPU growth over one percentage point of a
single core. The complete measurements are printed, not only the pass/fail.
These are regression guardrails, not a promise that one extra MiB is desirable.

This test does not measure a real compositor/GPU, desktop interactions, maximum
image decoding memory, animated addons or every provider process tree. RSS and
PSS vary with fonts, scale, shared library mappings, allocator and distribution.
Do not infer desktop-wide memory from the host's RSS alone. The synthetic test
is an apples-to-apples host comparison, not a complete desktop benchmark.

## Pay for enabled features

- Disabled providers are never started. Watch lists default to empty.
- Watch metadata checks reuse their storage and only run for opted-in packages.
- IPC uses the existing `poll`; receive buffers are used only after readiness,
  not on every redraw or metric tick.
- Image cache capacity (64 MiB) is a limit, not a startup allocation. Decoded
  images and SHM surfaces remain real costs. Fully transparent elements skip
  rendering and image lookup.
- Static posters use no provider process. A 1920x1080 decoded ARGB image costs
  about 7.9 MiB before surface buffers; compressed file size does not predict RAM.
- Canvas scripting adds the selected program's runtime. The native focus timer
  demonstrates an interpreter-free provider with one event loop.
- External renderers (including rs-yambar and Quickshell) keep their own resource
  costs and threads. The yambar launcher replaces itself using `exec`.

Do not enable all showcase packages by default or use a high-frequency animation
as a baseline. Heavy frameworks belong in optional providers rather than host
startup paths. Check `rs-wwidget status` for the host's own RSS/PSS/thread snapshot;
it does not include child-process memory.
