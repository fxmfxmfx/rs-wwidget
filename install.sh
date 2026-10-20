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
if not all((source / name).is_file() for name in (
        'widgets/widget-provider', 'widgets/music/music', 'widgets/cpu/cpu-monitor')):
    sys.exit('Run ./build.sh to build the bundled widgets before ./install.sh')
for directory in (share, controller.parent, applications):
    directory.mkdir(parents=True, exist_ok=True)

def install_file(source, target, mode=0o755):
    descriptor, name = tempfile.mkstemp(prefix='.install-', dir=target.parent)
    os.close(descriptor)
    try:
        shutil.copyfile(source, name)
        os.chmod(name, mode)
        os.replace(name, target)
    finally:
        Path(name).unlink(missing_ok=True)

install_file(source / 'rs-wwidget', controller)
for asset in sorted((source / 'widgets').rglob('*')):
    if not asset.is_file() or '-build.' in asset.name:
        continue
    target = share / 'widgets' / asset.relative_to(source / 'widgets')
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists() and (asset.name == 'settings.toml' or asset.suffix == '.txt'):
        continue
    install_file(asset, target, 0o755 if os.access(asset, os.X_OK) else 0o644)
# Retired bundled packages are removed on upgrade; user packages under config stay separate.
for retired in ('agenda', 'summary', 'timer'):
    target = share / 'widgets' / retired
    if target.is_symlink():
        target.unlink()
    elif target.is_dir():
        shutil.rmtree(target)
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
