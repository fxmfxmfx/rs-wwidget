#!/bin/sh
set -eu
cd "$(dirname "$0")"
WIDGET_TEST_DIR=$(mktemp -d)
WIDGET_BUILD_FILE=$(mktemp ./desktop-widgets-build.XXXXXX)
trap 'rm -rf "$WIDGET_TEST_DIR"; rm -f "$WIDGET_BUILD_FILE"' EXIT HUP INT TERM
cc -std=c11 -O2 -Wall -Wextra -Werror test_metrics.c metrics.c -o "$WIDGET_TEST_DIR/metrics"
"$WIDGET_TEST_DIR/metrics"
cc -std=c11 -O2 -Wall -Wextra -Werror test_layout.c layout.c $(pkg-config --cflags --libs glib-2.0) -lm -o "$WIDGET_TEST_DIR/layout"
"$WIDGET_TEST_DIR/layout"
cc -std=c11 -O2 -Wall -Wextra -Werror test_drag.c drag.c layout.c $(pkg-config --cflags --libs glib-2.0) -lm -o "$WIDGET_TEST_DIR/drag"
"$WIDGET_TEST_DIR/drag"
python3 -m unittest discover -p 'test_*.py'
cc -std=c11 -O2 -Wall -Wextra -Werror widgets.c metrics.c layout.c drag.c -o "$WIDGET_BUILD_FILE" $(pkg-config --cflags --libs gtk+-3.0 gtk-layer-shell-0) -lm
chmod 755 "$WIDGET_BUILD_FILE"
mv -f "$WIDGET_BUILD_FILE" desktop-widgets
