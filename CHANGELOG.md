# Changelog

## Unreleased

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
