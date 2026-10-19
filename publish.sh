#!/bin/sh
set -eu
cd "$(dirname "$0")"
exec gh repo create rs-wwidget --public --source=. --remote=origin --push --description 'Single-threaded Rust Gruvbox Material desktop widgets for Wayland and MangoWM, with draggable cards and grid snapping'
