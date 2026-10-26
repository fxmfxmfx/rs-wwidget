# Changelog

## Unreleased

- Add live same-user widget IPC: inspect, reload, configure, move, lock/unlock,
  hide/show/reset and application-defined events. Persist drag/visibility policy.
- Add bounded file watching and validate-before-reload semantics; isolate process
  replacement, clear static image caches and update surfaces without host restart.
- Support arbitrary surface-owning external renderers, including a real QML/
  Quickshell example, with shared theme/geometry/state metadata and process supervision.
- Add Python/JavaScript/QML package scaffolds, atomic provider state storage,
  bounded stderr diagnostics, package schemas and Ubuntu protocol CI.
- Raise canvas dimensions to 8192 logical pixels with a 256 MiB per-buffer cap.

- Add opt-in bidirectional stdin events for interactive providers: pointer,
  transformed element hit testing, scroll and on-demand physical keyboard input.
- Bound/coalesce nonblocking input queues and isolate broken/slow consumers.
  Add the playground provider, event-reader SDK and installed SDK assets.
- Add per-package locking, Wayland layers, working directories and environment
  overrides while preserving the existing v1 and noninteractive defaults.

- Add per-element rotation, pivot and rounded clipping to canvas v2, image
  alignment/sampling controls and bold text, with matching Python SDK helpers.
- Add native poster creation and rotation commands that resize the surface to
  preserve tilted corners, and a standalone JSON frame validation command.
- Publish an optional editor JSON Schema. Reject duplicate protocol fields and
  avoid reading disabled legacy static frames.

- Add backward-compatible canvas API v2 with layered images, text, rounded
  rectangles, lines, bars and graphs, RGBA/theme colors and element opacity.
- Support custom panel sizes, decoration and input modes, initial positions and
  size-aware dragging/damage. Add the `poster` package for a click-through static
  PNG/JPEG without a provider process, with a bounded decoded-image cache.
- Extend the Python SDK with canvas helpers, static-frame generation and input
  validation. Render static packages in previews and preserve their frame/image
  files when reinstalling. Document the v2 protocol and poster setup.

- Add widget packages enabled by name in the existing `widgets` list. Executable
  arguments, dimensions and restart policies belong to each package's manifest;
  source-specific choices live in the package's settings file.
- Add native read-only CPU, network, temperature/fan, MPRIS music and weather
  providers. Weather supports cached approximate IP location and explicit
  coordinates. Disabled packages create no child processes or rendering buffers.
- Add the versioned JSON widget API, a small optional Python SDK and native
  examples. The compatibility CPU launcher replaces Python with a native binary.
- Install bundled packages alongside the application and preserve their local
  settings. Remove retired experimental interactive/manual-data packages during
  upgrades; they are no longer bundled or accepted as provider modes.
- Track the live instance through its flock owner, process start time and
  executable. Support restarts between checkout and installed binaries, wait for
  complete lock release, and preserve logs when the owner cannot be inspected.
- Use pidfds for shutdown and reject stale status records that do not identify
  the actual lock holder. Cover restart, package lifecycle, MPRIS, installation,
  provider parsing and rendering with automated checks.

See [README](README.md), [widget API](docs/widget-api.md),
[bundled packages](widgets/README.md) and [architecture notes](docs/optimization.md).

## Addon showcases and resource regression checks

- Native Rust focus timer, bounded persistent vector sketchpad, and original rs-yambar external addon with the user’s Mango config/scripts.
- Normalized stroked/filled polyline primitive across Rust renderer, Python SDK and editor schema. Captured-frame PNG renderer for desktop-free inspection.
- Readiness-driven IPC, allocation-free watcher metadata refresh, and transparent-element render skipping.
- CI comparison of two built-in widgets against main, including PSS/RSS, threads, children and idle CPU; pinned upstream rs-yambar config validation.

- Cancel pointer/keyboard focus before replacing a live provider’s surface, preventing held gestures from surviving hide/pin/output changes.
