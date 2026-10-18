#!/bin/sh
set -eu
cd "$(dirname "$0")"
exec gh repo create gruvbox-desktop-widgets --public --source=. --remote=origin --push --description 'Lightweight Gruvbox Material desktop widgets for Wayland and MangoWM, with draggable cards and grid snapping'
