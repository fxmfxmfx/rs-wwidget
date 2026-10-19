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
cargo test --locked
python3 -m unittest discover -p 'test_*.py'
if [ "${1:-}" = "--benchmark" ]; then
    ./rs-wwidget --benchmark
fi
