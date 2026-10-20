#!/usr/bin/env python3
"""Opt-in external-plugin smoke test in an isolated headless MangoWM session.

Run after building rs-wwidget and ``cargo build --example test_pointer``.
Requires mango and grim. It never connects to the user's active Wayland socket.
An optional first argument is a pre-plugin rs-wwidget binary for a baseline
RSS/PSS/thread comparison.
"""
import configparser
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
POINTER = ROOT / 'target/debug/examples/test_pointer'
sys.path.insert(0, str(ROOT / 'tests'))
from wayland_smoke import memory, png_pixel, wait_for  # noqa: E402

BINARY = ROOT / 'rs-wwidget'


def task_children(pid):
    path = Path(f'/proc/{pid}/task/{pid}/children')
    return path.read_text().split()


def sample_process(binary, env, config, label):
    process = subprocess.Popen([str(binary), '--config', str(config), '--run'], env=env,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                               start_new_session=True)
    try:
        time.sleep(1.1)
        assert process.poll() is None, f'{label} host exited early'
        result = memory(process.pid)
        children = task_children(process.pid)
        print(f'{label}: {json.dumps(result)}, child_tasks={children}', flush=True)
        assert result['threads'] <= 2, f'{label} host has more than two threads: {result}'
        assert not children, f'{label} default/disabled host started children: {children}'
        return result
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=5)


def is_alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False


def main():
    assert BINARY.exists() and POINTER.exists(), 'build rs-wwidget and test_pointer first'
    for tool in ('mango', 'grim'):
        assert shutil.which(tool), f'{tool} is required'

    with tempfile.TemporaryDirectory(prefix='rs-wwidget-plugin-') as temp:
        root = Path(temp)
        runtime = root / 'runtime'
        runtime.mkdir(mode=0o700)
        home = root / 'home'
        home.mkdir()
        compositor_config = root / 'mango.conf'
        compositor_config.write_text('')
        sdk_path = str(ROOT / 'sdk/python')
        pythonpath = os.pathsep.join(filter(None, [sdk_path, os.environ.get('PYTHONPATH', '')]))
        env = {**os.environ, 'HOME': str(home), 'XDG_RUNTIME_DIR': str(runtime),
               'XDG_STATE_HOME': str(home / '.state'),
               'XDG_CONFIG_HOME': str(home / '.config'),
               'XDG_CACHE_HOME': str(home / '.cache'), 'PYTHONPATH': pythonpath,
               'DBUS_SESSION_BUS_ADDRESS': 'unix:path=' + str(root / 'no-session-bus'),
               'WLR_BACKENDS': 'headless', 'WLR_RENDERER': 'pixman',
               'WLR_HEADLESS_OUTPUTS': '2', 'WLR_LIBINPUT_NO_DEVICES': '1',
               'WAYLAND_DISPLAY': 'wayland-0'}
        children = []
        host = None
        pointer = None
        child_pid = None
        with (root / 'compositor.log').open('w') as log:
            compositor = subprocess.Popen(['mango', '-c', str(compositor_config)], env=env,
                                          stdout=log, stderr=log, start_new_session=True)
            children.append(compositor)
            try:
                wait_for(lambda: (runtime / 'wayland-0').exists())

                def cli(binary, config, *args, check=True):
                    return subprocess.run([str(binary), '--config', str(config), *args], env=env,
                                          check=check, capture_output=True, text=True, timeout=10)

                empty_cfg = root / 'empty.toml'
                empty_cfg.write_text('')
                monitors = cli(BINARY, empty_cfg, '--list-monitors').stdout.strip().splitlines()
                assert len(monitors) == 2, monitors
                target = monitors[-1].split('\t')[0]
                width, height = map(int, monitors[-1].split('\t')[1].split()[0].split('x'))

                disabled_marker = root / 'disabled-ran'
                enabled_marker = root / 'preview-ran'
                fixture = root / 'fixture.py'
                fixture.write_text(
                    'import os, pathlib, sys, time\n'
                    'from rs_wwidget import Row, emit\n'
                    'pathlib.Path(sys.argv[1]).open("a").write(str(os.getpid()) + "\\n")\n'
                    'while True:\n'
                    '    emit("Live plugin", [Row("PLUGIN_FRAME", "VISIBLE_42", 0.42, [0.2, 0.42])])\n'
                    '    time.sleep(1)\n')
                (root / 'disabled.toml').write_text(
                    f'monitor = "{target}"\n'
                    '[[plugins]]\nid = "disabled_probe"\n'
                    f'command = ["python3", "{fixture}", "{disabled_marker}"]\n')
                default_cfg = root / 'default.toml'
                default_cfg.write_text(f'monitor = "{target}"\n')

                baseline_path = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else None
                if baseline_path:
                    baseline = sample_process(baseline_path, env, default_cfg, 'baseline')
                    current_default = sample_process(BINARY, env, default_cfg, 'new-default')
                    print('memory delta KiB: ' + json.dumps({
                        key: current_default[key] - baseline[key] for key in ('rss_kib', 'pss_kib')
                    }), flush=True)
                else:
                    current_default = sample_process(BINARY, env, default_cfg, 'new-default')
                disabled = sample_process(BINARY, env, root / 'disabled.toml', 'disabled-plugin')
                assert not disabled_marker.exists(), 'disabled plugin command was executed'

                # A configured enabled process must remain inert during config checks and previews.
                preview_cfg = root / 'preview.toml'
                preview_cfg.write_text(
                    f'monitor = "{target}"\nwidgets = []\n'
                    '[[plugins]]\nid = "preview_probe"\nenabled = true\nheight = 178\n'
                    'restart_seconds = 1\ntimeout_seconds = 5\n'
                    f'command = ["python3", "{fixture}", "{enabled_marker}"]\n')
                cli(BINARY, preview_cfg, '--check-config')
                cli(BINARY, preview_cfg, '--preview', str(root / 'preview.png'))
                assert not enabled_marker.exists(), 'check-config or preview launched plugin'

                package = root / 'widgets/sample'
                package.mkdir(parents=True)

                def write_package(marker):
                    (package / 'widget.toml').write_text(
                        'version = 1\nid = "sample"\nheight = 178\n'
                        'restart_seconds = 1\ntimeout_seconds = 5\n'
                        f'command = ["python3", "{fixture}", "{marker}"]\n')

                write_package(root / 'pids')
                plugin_cfg = root / 'plugin.toml'
                plugin_cfg.write_text(
                    f'monitor = "{target}"\nwidgets = ["sample"]\nbackground_opacity = 1.0\n'
                    '[colors]\nbackground = "#234567"\n')
                host = subprocess.Popen([str(BINARY), '--config', str(plugin_cfg), '--run'], env=env,
                                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                        start_new_session=True)
                children.append(host)
                pid_file = root / 'pids'
                wait_for(lambda: pid_file.read_text() if pid_file.exists() else None)
                child_pid = int(pid_file.read_text().splitlines()[-1])
                def capture_live():
                    subprocess.run(['grim', '-o', target, str(root / 'live.png')], env=env,
                                   check=True, capture_output=True)
                    panel = png_pixel(root / 'live.png', 48 + 260, 64 + 80)
                    background = png_pixel(root / 'live.png', 10, 10)
                    content = png_pixel(root / 'live.png', 48 + 50, 64 + 75)
                    return (panel, background, content) if panel != background and content != panel else False

                pixel, outside, content = wait_for(capture_live)
                assert pixel != outside, f'plugin panel is not visible against the output: {pixel}'
                assert content != pixel, 'plugin frame content was not rendered'

                before = len(pid_file.read_text().splitlines())
                duplicate = cli(BINARY, plugin_cfg, '--run', check=False)
                assert duplicate.returncode == 0, duplicate.stderr
                cli(BINARY, plugin_cfg, '--list-monitors')
                time.sleep(1.1)
                assert len(pid_file.read_text().splitlines()) == before, 'duplicate host or monitor query launched another plugin'
                host_children = task_children(host.pid)
                assert host_children == [str(child_pid)], f'expected one plugin child {child_pid}, got {host_children}'
                host_resources = memory(host.pid)
                assert host_resources['threads'] <= 2, host_resources
                print('enabled-plugin host: ' + json.dumps(host_resources), flush=True)
                print('enabled-plugin child: ' + json.dumps(memory(child_pid)), flush=True)

                pointer = subprocess.Popen([str(POINTER), target, str(width), str(height)], env=env,
                                           stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                           stderr=subprocess.PIPE, text=True)
                children.append(pointer)
                assert pointer.stdout.readline().strip() == 'ready'

                def send(*commands):
                    import select
                    for command in commands:
                        pointer.stdin.write(command + '\n')
                        pointer.stdin.flush()
                        ready, _, _ = select.select([pointer.stdout], [], [], 5)
                        assert ready, f'pointer command timed out: {command}'
                        assert pointer.stdout.readline().strip() == 'done', command

                send('move 58 84', 'press', 'move 378 148', 'release')
                layout_path = home / '.state/rs-wwidget/layout.ini'
                saved = wait_for(lambda: layout_path.read_text() if layout_path.exists() else None)
                layout = configparser.ConfigParser()
                layout.read_string(saved)
                assert int(layout['sample']['x']) == 368, saved
                assert int(layout['sample']['y']) == 128, saved

                # Adding another named package preserves the saved position by ID.
                reordered_cfg = root / 'reordered.toml'
                reordered_cfg.write_text(
                    f'monitor = "{target}"\nwidgets = ["cpu", "sample"]\nbackground_opacity = 1.0\n'
                    '[colors]\nbackground = "#234567"\n')
                write_package(root / 'pids2')
                host.terminate()
                host.wait(timeout=5)
                host = subprocess.Popen([str(BINARY), '--config', str(reordered_cfg), '--run'], env=env,
                                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                        start_new_session=True)
                children.append(host)
                pid_file2 = root / 'pids2'
                wait_for(lambda: pid_file2.read_text() if pid_file2.exists() else None)
                def capture_restored():
                    subprocess.run(['grim', '-o', target, str(root / 'restored.png')], env=env,
                                   check=True, capture_output=True)
                    panel = png_pixel(root / 'restored.png', 368 + 260, 128 + 80)
                    content = png_pixel(root / 'restored.png', 368 + 50, 128 + 75)
                    return (panel, content) if panel != outside and content != panel else False

                restored_panel, restored_content = wait_for(capture_restored)
                assert restored_panel != outside and restored_content != restored_panel, \
                    'plugin position was not restored by ID after reordering'

                restored_pid = int(pid_file2.read_text().splitlines()[-1])
                host.terminate()
                host.wait(timeout=5)
                host = None
                wait_for(lambda: not is_alive(restored_pid))
                assert not is_alive(child_pid), 'plugin child survived host shutdown/restart'
                pointer.stdin.close()
                pointer.wait(timeout=5)
                pointer = None
                print('PASS: opt-in process lifecycle, default/disabled resource use, CLI non-launch, '
                      'live render, single instance, drag persistence by ID, process-group cleanup', flush=True)
            finally:
                if host and host.poll() is None:
                    host.send_signal(signal.SIGTERM)
                    try:
                        host.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        host.kill()
                        host.wait()
                if pointer and pointer.poll() is None:
                    pointer.terminate()
                    try:
                        pointer.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        pointer.kill()
                        pointer.wait()
                for child in reversed(children):
                    if child.poll() is None:
                        child.terminate()
                        try:
                            child.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            child.kill()
                            child.wait()
                for marker in (root / 'pids', root / 'pids2'):
                    if not marker.exists():
                        continue
                    for value in marker.read_text().splitlines():
                        try:
                            pid = int(value)
                            os.kill(pid, signal.SIGTERM)
                        except (ValueError, ProcessLookupError):
                            continue
                        deadline = time.monotonic() + 1
                        while is_alive(pid) and time.monotonic() < deadline:
                            time.sleep(.02)
                        if is_alive(pid):
                            try:
                                os.kill(pid, signal.SIGKILL)
                            except ProcessLookupError:
                                pass
                if compositor.poll() is None:
                    compositor.terminate()
                    try:
                        compositor.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        compositor.kill()
                        compositor.wait()


if __name__ == '__main__':
    main()
