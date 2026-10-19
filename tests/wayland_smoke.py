#!/usr/bin/env python3
"""Opt-in live protocol test in an isolated, screenless MangoWM session.

Run after ./build.sh and cargo build --example test_pointer.
Requires mango and grim. Never connects to the user's active Wayland socket.
Set RS_WWIDGET_LEGACY to a saved C binary for a same-session memory comparison.
"""
import configparser
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import time
import zlib

ROOT = Path(__file__).resolve().parent.parent
BINARY = ROOT / 'rs-wwidget'
POINTER = ROOT / 'target/debug/examples/test_pointer'


def wait_for(predicate, seconds=5):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        try:
            value = predicate()
            if value:
                return value
        except (FileNotFoundError, json.JSONDecodeError, KeyError):
            pass
        time.sleep(.03)
    raise AssertionError('timed out waiting for compositor/client state')


def memory(pid):
    fields = {}
    for line in Path(f'/proc/{pid}/smaps_rollup').read_text().splitlines():
        parts = line.split()
        if parts and parts[0] in ('Rss:', 'Pss:'):
            fields[parts[0][:-1].lower() + '_kib'] = int(parts[1])
    fields['thread_names'] = [(t / 'comm').read_text().strip() for t in Path(f'/proc/{pid}/task').iterdir()]
    fields['threads'] = len(fields['thread_names'])
    return fields


def png_pixel(path, x, y):
    """Read one 8-bit RGB/RGBA pixel from a non-interlaced PNG."""
    data = Path(path).read_bytes()
    assert data[:8] == b'\x89PNG\r\n\x1a\n', 'invalid PNG signature'
    offset = 8
    compressed = bytearray()
    width = height = color_type = bit_depth = None
    while offset < len(data):
        length = struct.unpack_from('>I', data, offset)[0]
        kind = data[offset + 4:offset + 8]
        chunk = data[offset + 8:offset + 8 + length]
        offset += length + 12
        if kind == b'IHDR':
            width, height, bit_depth, color_type, compression, filtering, interlace = struct.unpack(
                '>IIBBBBB', chunk)
            assert (bit_depth, compression, filtering, interlace) == (8, 0, 0, 0)
            assert color_type in (2, 6), f'unsupported PNG color type {color_type}'
        elif kind == b'IDAT':
            compressed.extend(chunk)
        elif kind == b'IEND':
            break
    assert width is not None and 0 <= x < width and 0 <= y < height
    channels = 4 if color_type == 6 else 3
    stride = width * channels
    raw = zlib.decompress(compressed)
    rows = []
    previous = bytearray(stride)
    pos = 0
    for _ in range(height):
        filter_type = raw[pos]
        scan = bytearray(raw[pos + 1:pos + 1 + stride])
        pos += stride + 1
        for i in range(stride):
            left = scan[i - channels] if i >= channels else 0
            above = previous[i]
            upper_left = previous[i - channels] if i >= channels else 0
            if filter_type == 1:
                scan[i] = (scan[i] + left) & 0xff
            elif filter_type == 2:
                scan[i] = (scan[i] + above) & 0xff
            elif filter_type == 3:
                scan[i] = (scan[i] + ((left + above) // 2)) & 0xff
            elif filter_type == 4:
                p = left + above - upper_left
                pa, pb, pc = abs(p - left), abs(p - above), abs(p - upper_left)
                predictor = left if pa <= pb and pa <= pc else above if pb <= pc else upper_left
                scan[i] = (scan[i] + predictor) & 0xff
            else:
                assert filter_type == 0, f'unsupported PNG filter {filter_type}'
        rows.append(scan)
        previous = scan
    pixel = rows[y][x * channels:(x + 1) * channels]
    return tuple(pixel) + ((255,) if channels == 3 else ())


def main():
    assert BINARY.exists() and POINTER.exists(), 'build rs-wwidget and test_pointer first'
    for tool in ('mango', 'grim'):
        assert shutil.which(tool), f'{tool} is required'
    with tempfile.TemporaryDirectory(prefix='rs-wwidget-wayland-') as temp:
        root = Path(temp)
        runtime = root / 'runtime'
        runtime.mkdir(mode=0o700)
        home = root / 'home'
        home.mkdir()
        compositor_config = root / 'mango.conf'
        compositor_config.write_text('')
        env = {**os.environ, 'HOME': str(home), 'XDG_RUNTIME_DIR': str(runtime),
               'XDG_STATE_HOME': str(home / '.state'),
               'XDG_CONFIG_HOME': str(home / '.config'),
               'XDG_CACHE_HOME': str(home / '.cache'),
               'DBUS_SESSION_BUS_ADDRESS': 'unix:path=' + str(root / 'no-session-bus'),
               'WLR_BACKENDS': 'headless', 'WLR_RENDERER': 'pixman',
               'WLR_HEADLESS_OUTPUTS': '2', 'WLR_LIBINPUT_NO_DEVICES': '1',
               'WAYLAND_DISPLAY': 'wayland-0'}
        children = []
        client_pid = None
        with (root / 'compositor.log').open('w') as log:
            compositor = subprocess.Popen(['mango', '-c', str(compositor_config)], env=env,
                                          stdout=log, stderr=log)
            children.append(compositor)
            try:
                wait_for(lambda: (runtime / 'wayland-0').exists())

                def cli(*args, check=True):
                    return subprocess.run([str(BINARY), *args], env=env, check=check,
                                          capture_output=True, text=True, timeout=10)

                monitors = cli('--list-monitors').stdout.strip().splitlines()
                assert len(monitors) == 2, monitors
                target = monitors[-1].split('\t')[0]
                w, h = map(int, monitors[-1].split('\t')[1].split()[0].split('x'))
                cfg = root / 'config.toml'
                cfg.write_text(f'monitor = "{target}"\nbackground_opacity = 1.0\n'
                               '[colors]\nbackground = "#234567"\n')
                invalid = root / 'invalid.toml'
                invalid.write_text('monitor = "nonexistent-output"\n')
                failure = cli('--config', str(invalid), '--run', check=False)
                assert failure.returncode != 0 and 'unavailable' in failure.stderr

                import sys
                legacy = sys.argv[1] if len(sys.argv) > 1 else os.environ.get('RS_WWIDGET_LEGACY')
                if legacy:
                    with (root / 'legacy.log').open('w') as log:
                        old = subprocess.Popen([legacy], env=env, stdout=log, stderr=log)
                        children.append(old)
                        time.sleep(5.5)
                        assert old.poll() is None, 'legacy client exited'
                        print('legacy:', json.dumps(memory(old.pid)), flush=True)
                        old.terminate()
                        old.wait(timeout=5)

                cli('--config', str(cfg), 'start')
                status_path = home / '.state/rs-wwidget/status.json'

                def status():
                    return json.loads(status_path.read_text())

                info = wait_for(status)
                client_pid = info['pid']
                time.sleep(5.5)
                info = status()
                assert info['monitor'] == target, info
                print('rust:', json.dumps(memory(client_pid)), flush=True)
                assert info['threads'] <= 2, info
                cli('--config', str(cfg), '--run')  # single-instance lock
                assert status()['pid'] == client_pid
                x = int((max(0, w - 300 - 40) + 8) // 16 * 16)
                subprocess.run(['grim', '-o', target, str(root / 'live.png')],
                               env=env, check=True, capture_output=True)
                panel_sample = x + 260, 144
                expected_panel_pixel = (0x23, 0x45, 0x67, 0xff)
                assert png_pixel(root / 'live.png', *panel_sample) == expected_panel_pixel, \
                    'baseline panel sample must be opaque and distinguishable from the background'

                pointer = subprocess.Popen([str(POINTER), target, str(w), str(h)], env=env,
                                           stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                           stderr=subprocess.PIPE, text=True)
                children.append(pointer)
                assert pointer.stdout.readline().strip() == 'ready'

                def send(*commands):
                    for command in commands:
                        pointer.stdin.write(command + '\n')
                        pointer.stdin.flush()
                        import select
                        ready, _, _ = select.select([pointer.stdout], [], [], 5)
                        assert ready, f'pointer command timed out: {command}'
                        assert pointer.stdout.readline().strip() == 'done', command

                send(f'move {x+10} 84', 'press')
                wait_for(lambda: status()['drag_active'])
                drag_start = root / 'drag-start.png'
                subprocess.run(['grim', '-o', target, str(drag_start)], env=env, check=True,
                               capture_output=True)
                assert png_pixel(drag_start, *panel_sample) != png_pixel(drag_start, 10, 10), \
                    'drag start must keep the old panel area covered by the panel or overlay'
                send(f'move {x-310} 148', 'release')
                wait_for(lambda: not status()['drag_active'])
                layout_path = home / '.state/rs-wwidget/layout.ini'
                saved = wait_for(lambda: layout_path.read_text())
                layout = configparser.ConfigParser()
                layout.read_string(saved)
                assert int(layout['clock']['x']) == x-320, saved
                assert int(layout['clock']['y']) == 128, saved
                moved_panel_sample = x - 320 + 260, 128 + 80
                send(f'move {x-310} 148', 'press')
                wait_for(lambda: status()['drag_active'])
                send('move 100 100', 'right', 'release')
                wait_for(lambda: not status()['drag_active'])
                assert layout_path.read_text() == saved, 'cancel must preserve layout'
                # Non-left input over a draggable header must not start dragging
                # or place the transparent grid above the panel.
                session_log = home / '.state/rs-wwidget/session.log'
                drag_begins = session_log.read_text().count('drag begin:')
                send(f'move {x-320+10} {128+20}', 'middle')
                send('middle_press')
                assert not status()['drag_active'], 'middle press must not start a drag'
                held = root / 'middle-held.png'
                subprocess.run(['grim', '-o', target, str(held)], env=env, check=True,
                               capture_output=True)
                assert png_pixel(held, *moved_panel_sample) == expected_panel_pixel, \
                    'middle press must not cover the panel with the drag grid'
                send('middle_release', 'scroll')
                assert not status()['drag_active'], 'middle release or scroll must not start a drag'
                assert session_log.read_text().count('drag begin:') == drag_begins, \
                    'middle click and scroll must not log a drag begin'
                # The application must also recover when a release never arrives.
                send(f'move {x-310} 148', 'press')
                wait_for(lambda: status()['drag_active'])
                wait_for(lambda: not status()['drag_active'], seconds=33)
                send('release')
                assert layout_path.read_text() == saved
                pointer.stdin.close()
                pointer.wait(timeout=5)
                cli('stop')
                assert not status_path.exists()
                client_pid = None
                print('PASS: named output, main + Pango thread, single instance, live SHM render, '
                      'drag commit/cancel/idle recovery, clean stop', flush=True)
            finally:
                if client_pid:
                    try:
                        os.kill(client_pid, 15)
                    except ProcessLookupError:
                        pass
                for child in reversed(children):
                    if child.poll() is None:
                        child.terminate()
                        try:
                            child.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            child.kill()
                            child.wait()


if __name__ == '__main__':
    main()
