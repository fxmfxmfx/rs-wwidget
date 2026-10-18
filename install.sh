#!/bin/sh
set -eu
WIDGET_PROJECT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
python3 - "$WIDGET_PROJECT_DIR" <<'PY'
from pathlib import Path
import os
import shlex
import shutil
import sys
import tempfile

source = Path(sys.argv[1])
home = Path.home()
share = home / '.local/share/desktop-widgets'
controller = home / '.local/bin/desktop-widgets'
applications = home / '.local/share/applications'
if not (source / 'desktop-widgets').is_file():
    sys.exit('Run ./build.sh before ./install.sh')
for directory in (share, controller.parent, applications):
    directory.mkdir(parents=True, exist_ok=True)

def install_file(source, target):
    descriptor, name = tempfile.mkstemp(prefix='.install-', dir=target.parent)
    os.close(descriptor)
    try:
        shutil.copyfile(source, name)
        os.chmod(name, 0o755)
        os.replace(name, target)
    finally:
        Path(name).unlink(missing_ok=True)

install_file(source / 'desktop-widgets', share / 'desktop-widgets')
install_file(source / 'desktop-widgets.py', controller)
(share / 'mango.conf').write_text(
    'exec_once = ' + shlex.quote(str(controller)) + ' start\n'
    'layer_rule = no_animation:1, no_blur:1, layer_name:desktop-widgets\n')
# Desktop Entry Exec has its own quoting rules (it does not use a shell).
quoted = str(controller)
for char in ('\\', '"', '`', '$'):
    quoted = quoted.replace(char, '\\' + char)
quoted = quoted.replace('%', '%%')
(applications / 'desktop-widgets.desktop').write_text(
    '[Desktop Entry]\nType=Application\nName=Desktop Widgets\n'
    'Name[ru]=Виджеты — показать / скрыть\n'
    'Comment=Gruvbox clock, calendar and system widgets for Wayland\n'
    f'Exec="{quoted}" toggle\n'
    'Icon=preferences-desktop-theme\nTerminal=false\nCategories=Utility;\nStartupNotify=false\n')
print('Installed. Start from your desktop terminal:')
print(shlex.quote(str(controller)) + ' start')
print('Optional MangoWM autostart — add this line to ~/.config/mango/config.conf:')
print('source = ' + str(share / 'mango.conf'))
PY
