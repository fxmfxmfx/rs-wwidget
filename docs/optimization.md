# rs-wwidget architecture and measurements

## Changes

The application has been fully ported to Rust. It uses `wayland-client` and the
wlr-layer-shell protocol instead of GTK/GDK and `gtk-layer-shell`. Cairo/Pango
remain in use to preserve rasterization and font matching. Neither a Python
controller nor a separate polling process is needed. Python remains a tool for
installation and integration checks.

One event loop handles Wayland events, metric polling, and the drag recovery
timer. While waiting for events, it sleeps in `poll` until the next polling or
pointer recovery deadline. There is no async runtime or application worker thread.

The installed Pango 1.57.1 creates an internal `[pango] fontconfig` thread, so
the process has **2 threads** including the main thread. The thread is created
by [Pango's Fontconfig backend](https://github.com/GNOME/pango/blob/main/pango/pangofc-fontmap.c)
for font matching; disabling Fontconfig's periodic checks does not remove it.

## Buffers and redraws

- Each panel has its own SHM surface. Buffers are allocated as needed, with no
  more than two retained per surface at once.
- A buffer is not overwritten until the compositor sends `wl_buffer.release`.
  No permanent full-screen buffers are allocated.
- The grid is created only during a drag and freed when the drag ends. The last
  position is tracked separately for each of its buffers; both the old and new
  areas, including the border, are redrawn. This preserves correct double buffering.
- Movement within a single 16 px cell does not trigger a redraw.
- Metrics are polled every 5 seconds; the clock and calendar update only when
  their displayed time or date changes. Unchanged system metrics are not redrawn.
- `/proc/stat` is read into a bounded buffer containing the first aggregate line,
  without loading per-CPU lines. The `/proc/meminfo` buffer is reused.
- Full panel images are not cached; two font descriptions are cached.

Monitor scale is applied through `wl_surface.set_buffer_scale` and Cairo's
device scale. Sizes and positions remain in logical pixels. SHM memory grows
with the square of the scale; during a drag, grid buffers sized to the selected
display are added.

## Appearance and behavior checks

`cargo test` compares Rust output with the independent saved C renderer in
`tests/reference/`. It checks three datasets, three panels, 1×/2× scale, the
full grid, and a sequence of partial drag frames. Decoded RGBA pixels are
compared for exact equality.

`tests/wayland_smoke.py` starts a separate MangoWM instance with two headless
outputs. It checks connector selection, the error for a missing monitor, the
single-instance lock, SHM surface creation, dragging with coordinate persistence,
cancellation, recovery after 30 seconds, and clean shutdown. It does not use the
current user session.

## Local measurements from 2026-10-19

MangoWM with the headless backend and pixman, two 1280×720 outputs, 1× scale.
Measurements were taken 5.5 seconds after launching each version. The C binary
was built from the repository's original state with `-O2`; Rust was built in
release mode with LTO. Values were read externally from `/proc/PID/smaps_rollup`
and `/proc/PID/task` in the same environment.

| Version | RSS, KiB | PSS, KiB | Threads |
| --- | ---: | ---: | ---: |
| C / GTK | 26 208 | 7 896 | 6 |
| Rust / direct Wayland | 14 696 | 3 544 | 2 |

In this measurement, RSS was about 44% lower and PSS about 55% lower.
Absolute values depend on libraries, fonts, scale, shared pages, and the
compositor; this is a short local measurement, not a universal guarantee.

`./rs-wwidget --benchmark` measures CPU time for headless rendering. It does not
replace measuring the full running process. To compare live application CPU
usage, use `status.json` for the same PID:

```text
100 × (cpu_usec_2 − cpu_usec_1) / (elapsed_usec_2 − elapsed_usec_1)
```

## Language selection and drag visibility

Widget translations are static string tables selected once when the renderer is
created. English and Russian use the same geometry, font descriptions, metric
sampler and redraw cadence. CLI output, diagnostics and documentation remain
English regardless of `language`.

A drag previously cleared the original panel while its preview surface was still
waiting for the layer-shell configure handshake. The reverse gap occurred when
the preview was destroyed before the original panel's restoration was committed.
The source now stays visible until the overlay's first frame callback, and the
preview stays alive until the restored source commits its buffer. Late callbacks
from cancelled overlays are ignored. This adds one callback per drag, with no
new idle wakeups, threads, image caches or SHM buffers.

Five paired headless rendering runs with the same Russian labels, font and palette
measured median CPU times of 26.02 ms before and 25.58 ms after the changes for
20 simulated minutes. This short local comparison shows no rendering slowdown;
it does not establish a universal timing guarantee.

`test_wayland_handoff.py` also controls the configure and frame-callback delays
with a small test compositor implemented using Python's standard library. It
checks the source buffer's actual alpha and the order of restoration and preview
destruction without connecting to the user's desktop. This test is included in
`./build.sh` through normal Python unittest discovery.
