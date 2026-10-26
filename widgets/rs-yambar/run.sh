#!/bin/sh
# exec replaces this launcher: there is no persistent wrapper process.
set -eu
package=${RS_WWIDGET_ASSET_DIR:-$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)}
export RS_WWIDGET_ASSET_DIR="$package"
binary=${RS_YAMBAR_BINARY:-rs-yambar}
if [ "$binary" = rs-yambar ] && ! command -v "$binary" >/dev/null 2>&1; then
    if [ -x "${HOME:-}/.local/bin/rs-yambar" ]; then
        binary=$HOME/.local/bin/rs-yambar
    else
        echo 'rs-yambar addon: install rs-yambar, or set RS_YAMBAR_BINARY to its executable' >&2
        exit 127
    fi
fi
config=${RS_YAMBAR_CONFIG:-$package/config.toml}
if [ ! -r "$config" ]; then
    echo "rs-yambar addon: cannot read configuration: $config" >&2
    exit 1
fi
exec "$binary" -c "$config" "$@"
