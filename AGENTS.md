# Repository Guidelines

## Project Structure

The application is a small C11/GTK 3 Wayland widget process. Core rendering and application startup are in `widgets.c`; reusable logic is split into matching C/header pairs (`metrics`, `layout`, `drag`, and `refresh`). `desktop-widgets.py` provides process control, while `build.sh` and `install.sh` build, install, and manage the application. C checks live in root-level `test_*.c` files and `tests/test_render.c`; Python checks are `test_*.py`. `docs/optimization.md` documents rendering performance work.

## Build, Test, and Development

- `./build.sh` compiles with warnings treated as errors, runs the C and Python checks, and produces `desktop-widgets`.
- `./build.sh --benchmark` runs the rendering comparison in benchmark mode before building the application.
- `./desktop-widgets --preview /tmp/widgets.png` and `--preview-grid /tmp/widgets-grid.png` render previews without a live Wayland session.
- `./install.sh` installs into the current user's `~/.local` directories; use it only when you intend to change your local installation.

The build requires a C compiler, `pkg-config`, GTK 3, gtk-layer-shell, GLib, and Python 3.9 or newer. The README lists distribution-specific dependency commands.

## Code Style and Naming

Use four spaces for Python indentation and follow the existing compact C style. Keep shared C logic in a focused `.c`/`.h` pair, use `snake_case` for functions and variables, and use uppercase names for macros and constants. Keep compiler output clean: `build.sh` uses `-Wall -Wextra -Werror`.

## Testing

Run `./build.sh` after changes to application logic, rendering, process control, or installation behavior. C test files use the `test_*.c` naming pattern; Python tests use `test_*.py` and are discovered by `unittest`. Rendering tests compare output at multiple scales. GUI interaction, especially dragging, still needs a manual check in a Wayland session.

## Commits and Pull Requests

Recent history uses short subjects, often with a Conventional Commit prefix such as `fix:`, `perf:`, or `docs:`. Keep each commit focused and describe the behavior changed. A pull request should explain its user-visible effect, list relevant checks run, and include screenshots for visual changes. Mention any Wayland or MangoWM-specific behavior that reviewers need to verify.

## Configuration and Safety

The installer manages files under `~/.local` and a source line in MangoWM configuration. Do not run installation or removal commands as part of ordinary development checks. Keep generated binaries, temporary previews, and user-specific configuration out of commits.
