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
share = home / '.local/share/rs-wwidget'
controller = home / '.local/bin/rs-wwidget'
applications = home / '.local/share/applications'
if not (source / 'rs-wwidget').is_file():
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

install_file(source / 'rs-wwidget', controller)
config_root = Path(os.environ.get('XDG_CONFIG_HOME') or home / '.config')
if not config_root.is_absolute():
    config_root = home / '.config'
config = config_root / 'rs-wwidget/config.toml'
config.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
if not config.exists():
    with config.open('x') as output:
        output.write((source / 'config.example.toml').read_text())
    config.chmod(0o600)
(share / 'mango.conf').write_text(
    'exec_once = ' + shlex.quote(str(controller)) + ' start\n'
    'layerrule=noanim:1,noblur:1,noshadow:1,layer_name:rs-wwidget\n')
# Desktop Entry Exec has its own quoting rules (it does not use a shell).
quoted = str(controller)
for char in ('\\', '"', '`', '$'):
    quoted = quoted.replace(char, '\\' + char)
quoted = quoted.replace('%', '%%')
(applications / 'rs-wwidget.desktop').write_text(
    '[Desktop Entry]\nType=Application\nName=rs-wwidget\n'
    'Comment=Rust Gruvbox clock, calendar and system widgets for Wayland\n'
    f'Exec="{quoted}" toggle\n'
    'Icon=preferences-desktop-theme\nTerminal=false\nCategories=Utility;\nStartupNotify=false\n')
print('Installed. Start from your desktop terminal:')
print(shlex.quote(str(controller)) + ' start')
print('Optional MangoWM autostart — add this line to ~/.config/mango/config.conf:')
print('source = ' + str(share / 'mango.conf'))
PY
