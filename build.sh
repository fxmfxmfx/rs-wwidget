#!/bin/sh
set -eu
cd "$(dirname "$0")"
case "${1:-}" in
    ""|--benchmark) ;;
    *) echo "Usage: ./build.sh [--benchmark]" >&2; exit 2 ;;
esac
cargo build --release --locked
WIDGET_BUILD_FILE=$(mktemp ./rs-wwidget-build.XXXXXX)
trap 'rm -f "$WIDGET_BUILD_FILE"' EXIT HUP INT TERM
cp target/release/rs-wwidget "$WIDGET_BUILD_FILE"
chmod 755 "$WIDGET_BUILD_FILE"
mv -f "$WIDGET_BUILD_FILE" rs-wwidget
WIDGET_BUILD_FILE=$(mktemp ./examples/widgets/cpu-monitor-build.XXXXXX)
"${CC:-cc}" -std=c11 -O2 -Wall -Wextra -Werror examples/widgets/cpu.c -o "$WIDGET_BUILD_FILE"
chmod 755 "$WIDGET_BUILD_FILE"
mv -f "$WIDGET_BUILD_FILE" examples/widgets/cpu-monitor
WIDGET_BUILD_FILE=$(mktemp ./widgets/cpu/cpu-monitor-build.XXXXXX)
cp examples/widgets/cpu-monitor "$WIDGET_BUILD_FILE"
chmod 755 "$WIDGET_BUILD_FILE"
mv -f "$WIDGET_BUILD_FILE" widgets/cpu/cpu-monitor
WIDGET_BUILD_FILE=$(mktemp ./widgets/widget-provider-build.XXXXXX)
cp target/release/widget-provider "$WIDGET_BUILD_FILE"
chmod 755 "$WIDGET_BUILD_FILE"
mv -f "$WIDGET_BUILD_FILE" widgets/widget-provider
WIDGET_BUILD_FILE=$(mktemp ./widgets/music/music-build.XXXXXX)
"${CC:-cc}" -std=c11 -O2 -Wall -Wextra -Werror widgets/music/music.c $(pkg-config --cflags --libs gio-2.0) -o "$WIDGET_BUILD_FILE"
chmod 755 "$WIDGET_BUILD_FILE"
mv -f "$WIDGET_BUILD_FILE" widgets/music/music
cargo test --locked
python3 -m unittest discover -p 'test_*.py'
if [ "${1:-}" = "--benchmark" ]; then
    ./rs-wwidget --benchmark
fi
