"""Deterministic protocol regression for the drag overlay visibility handoff.

This starts rs-wwidget against a tiny in-process Wayland server. It deliberately
holds the overlay configure and first frame callback to control the handoff.
Only Python's standard library and the built rs-wwidget binary are required.
"""
import array
import mmap
import os
import json
from pathlib import Path
import select
import socket
import struct
import subprocess
import tempfile
import threading
import time
import traceback
import unittest


ROOT = Path(__file__).resolve().parent
TIMEOUT = 5.0


def specs(items):
    """Build compact (name, argument-type string) protocol tables."""
    result = []
    for entry in items.split(';'):
        parts = entry.split()
        if parts:
            result.append((parts[0], parts[1] if len(parts) > 1 else ''))
    return result


# Entries retain protocol opcode order, including operations this test ignores.
INTERFACES = {
    'wl_display': (specs('sync n; get_registry n'), specs('error ous; delete_id u')),
    'wl_registry': (specs('bind un'), specs('global usu; global_remove u')),
    'wl_callback': (specs(''), specs('done u')),
    'wl_compositor': (specs('create_surface n; create_region n'), specs('')),
    'wl_surface': (specs('destroy; attach oii; damage iiii; frame n; set_opaque_region o; '
                         'set_input_region o; commit; set_buffer_transform i; set_buffer_scale i; '
                         'damage_buffer iiii; offset ii'),
                   specs('enter o; leave o; preferred_buffer_scale i; preferred_buffer_transform u')),
    'wl_region': (specs('destroy; add iiii; subtract iiii'), specs('')),
    'wl_shm': (specs('create_pool nhi; release'), specs('format u')),
    'wl_shm_pool': (specs('create_buffer niiiiu; destroy; resize i'), specs('')),
    'wl_buffer': (specs('destroy'), specs('release')),
    'wl_output': (specs('release'), specs('geometry iiiiissi; mode uiii; done; scale i; name s; description s')),
    'wl_seat': (specs('get_pointer n; get_keyboard n; get_touch n; release'),
                specs('capabilities u; name s')),
    'wl_keyboard': (specs('release'), specs('keymap uhu; enter uoa; leave uo; key uuuu; modifiers uuuuu; repeat_info ii')),
    'wl_pointer': (specs('set_cursor uoii; release'),
                   specs('enter uoff; leave uo; motion uff; button uuuu; axis uuf; frame; '
                         'axis_source u; axis_stop uu; axis_discrete ui; axis_value120 ui; '
                         'axis_relative_direction uu')),
    'zwlr_layer_shell_v1': (specs('get_layer_surface noous; destroy'), specs('')),
    'zwlr_layer_surface_v1': (specs('set_size uu; set_anchor u; set_exclusive_zone i; '
                                    'set_margin iiii; set_keyboard_interactivity u; get_popup o; '
                                    'ack_configure u; destroy; set_layer u; set_exclusive_edge u'),
                              specs('configure uuu; closed')),
}


def pack_string(value):
    data = value.encode() + b'\0'
    return struct.pack('=I', len(data)) + data + bytes((-len(data)) % 4)


def pack_message(object_id, opcode, payload=b''):
    size = 8 + len(payload)
    return struct.pack('=II', object_id, (size << 16) | opcode) + payload


class FakeWayland:
    def __init__(self, runtime, keyboard=False):
        self.has_keyboard = keyboard
        self.keyboard_id = None
        self.runtime = Path(runtime)
        self.path = self.runtime / 'wayland-0'
        self.listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.listener.bind(str(self.path))
        self.listener.listen(1)
        self.listener.settimeout(TIMEOUT)
        self.conn = None
        self.thread = None
        self.lock = threading.RLock()
        self.cv = threading.Condition(self.lock)
        self.stop = False
        self.error = None
        self.rx = bytearray()
        self.rx_fds = []
        self.objects = {1: 'wl_display'}
        self.global_ids = {'wl_compositor': 10, 'wl_shm': 11, 'wl_output': 12,
                           'wl_seat': 13, 'zwlr_layer_shell_v1': 14}
        self.surfaces = {}
        self.layers = {}
        self.pools = {}
        self.buffers = {}
        self.pending_attach = {}
        self.commits = []
        self.requests = []
        self.sequence = 0
        self.overlay_destroy_sequence = None
        self.frame_callbacks = []
        self.source_id = None
        self.source_ids = []
        self.overlay_id = None
        self.configured_layers = set()
        self.overlay_first_frame_seen = False
        self.overlay_destroy_seen = False
        self.source_transparency_commits = []
        self.source_restored_commit = None
        self.registry_id = None
        self.output_id = None
        self.seat_id = None
        self.pointer_id = None
        self.shm_id = None
        self.compositor_id = None
        self.shell_id = None

    def start(self):
        self.thread = threading.Thread(target=self._serve, daemon=True)
        self.thread.start()
        return self

    def close(self):
        self.stop = True
        if self.conn:
            try:
                self.conn.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            self.conn.close()
        self.listener.close()
        if self.thread:
            self.thread.join(timeout=1)
        for fd, _, _ in self.pools.values():
            try:
                os.close(fd)
            except OSError:
                pass

    def wait_for(self, predicate, message):
        deadline = time.monotonic() + TIMEOUT
        with self.cv:
            while not predicate():
                if self.error:
                    raise AssertionError(f'fake Wayland server failed: {self.error}')
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise AssertionError(message)
                self.cv.wait(remaining)

    def _send(self, object_id, interface, event_name, args=()):
        events = INTERFACES[interface][1]
        opcode = next(i for i, entry in enumerate(events) if entry[0] == event_name)
        payload = bytearray()
        descriptors = []
        for typ, value in zip(events[opcode][1], args):
            if typ in ('u', 'i', 'f', 'o', 'n'):
                payload += struct.pack('=I', value & 0xffffffff)
            elif typ == 'h':
                descriptors.append(value)
            elif typ == 's':
                payload += pack_string(value)
            elif typ == 'a':
                payload += struct.pack('=I', len(value)) + value + bytes((-len(value)) % 4)
            else:
                raise AssertionError(f'unsupported event arg {interface}.{event_name}: {typ}')
        with self.lock:
            message = pack_message(object_id, opcode, payload)
            if descriptors:
                sent = self.conn.sendmsg([message], [(socket.SOL_SOCKET, socket.SCM_RIGHTS, array.array('i', descriptors).tobytes())])
                if sent < len(message): self.conn.sendall(message[sent:])
            else:
                self.conn.sendall(message)

    def _global(self, name, interface, version):
        self._send(self.registry_id, 'wl_registry', 'global',
                   (name, interface, version))

    def _initial_globals(self):
        for interface, name in self.global_ids.items():
            self._global(name, interface, 4 if interface in ('wl_compositor', 'wl_output',
                                                              'zwlr_layer_shell_v1') else
                         5 if interface == 'wl_seat' else 1)

    @staticmethod
    def _decode_string(data, at):
        length = struct.unpack_from('=I', data, at)[0]
        at += 4
        value = bytes(data[at:at + length - 1]).decode()
        return value, at + ((length + 3) & ~3)

    def _decode(self, interface, opcode, payload, fds):
        name, signature = INTERFACES[interface][0][opcode]
        if interface == 'wl_registry' and name == 'bind':
            global_name = struct.unpack_from('=I', payload, 0)[0]
            bound_interface, at = self._decode_string(payload, 4)
            version, new_id = struct.unpack_from('=II', payload, at)
            return name, [global_name, bound_interface, version, new_id]
        result = []
        at = 0
        fd_index = 0
        for typ in signature:
            if typ == 'h':
                result.append(fds[fd_index])
                fd_index += 1
            elif typ in ('u', 'i', 'f', 'o', 'n'):
                result.append(struct.unpack_from('=I', payload, at)[0])
                at += 4
            elif typ == 's':
                value, at = self._decode_string(payload, at)
                result.append(value)
            elif typ == 'a':
                length = struct.unpack_from('=I', payload, at)[0]
                at += 4
                result.append(bytes(payload[at:at + length]))
                at += (length + 3) & ~3
            else:
                raise AssertionError(f'unsupported request arg {interface}.{name}: {typ}')
        return name, result

    def _serve(self):
        try:
            self.conn, _ = self.listener.accept()
            self.conn.setblocking(False)
            while not self.stop:
                readable, _, _ = select.select([self.conn], [], [], 0.05)
                if not readable:
                    continue
                data, ancillary, _, _ = self.conn.recvmsg(65536, socket.CMSG_SPACE(16 * 4))
                if not data:
                    return
                self.rx.extend(data)
                for level, kind, raw in ancillary:
                    if level == socket.SOL_SOCKET and kind == socket.SCM_RIGHTS:
                        fds = array.array('i')
                        fds.frombytes(raw[:len(raw) - len(raw) % fds.itemsize])
                        self.rx_fds.extend(fds.tolist())
                while len(self.rx) >= 8:
                    object_id, word = struct.unpack_from('=II', self.rx)
                    size, opcode = word >> 16, word & 0xffff
                    if size < 8 or len(self.rx) < size:
                        break
                    packet = bytes(self.rx[8:size])
                    del self.rx[:size]
                    iface = self.objects.get(object_id)
                    self._request(object_id, iface, opcode, packet, self.rx_fds)
        except (OSError, AssertionError, IndexError, KeyError, ValueError) as exc:
            if not self.stop:
                with self.cv:
                    self.error = traceback.format_exc()
                    self.cv.notify_all()

    def _request(self, obj, iface, opcode, payload, fds):
        name, args = self._decode(iface, opcode, payload, fds)
        self.requests.append((iface, name, args))
        fd_count = INTERFACES[iface][0][opcode][1].count('h')
        if fd_count:
            del self.rx_fds[:fd_count]
        with self.cv:
            if iface == 'wl_display' and name == 'get_registry':
                self.registry_id = args[0]
                self.objects[self.registry_id] = 'wl_registry'
                self._initial_globals()
            elif iface == 'wl_display' and name == 'sync':
                cb = args[0]
                self.objects[cb] = 'wl_callback'
                self._send(cb, 'wl_callback', 'done', (0,))
            elif iface == 'wl_registry' and name == 'bind':
                global_name, interface, version, new_id = args
                self.objects[new_id] = interface
                if interface == 'wl_compositor':
                    self.compositor_id = new_id
                elif interface == 'wl_shm':
                    self.shm_id = new_id
                    self._send(new_id, 'wl_shm', 'format', (0,))
                elif interface == 'wl_output':
                    self.output_id = new_id
                    self._send(new_id, 'wl_output', 'geometry', (0, 0, 600, 340, 0, 'test', 'fake', 0))
                    self._send(new_id, 'wl_output', 'mode', (1, 1920, 1080, 60000))
                    if version >= 2:
                        self._send(new_id, 'wl_output', 'scale', (1,))
                        self._send(new_id, 'wl_output', 'done', ())
                    if version >= 4:
                        self._send(new_id, 'wl_output', 'name', ('FAKE-1',))
                elif interface == 'wl_seat':
                    self.seat_id = new_id
                    self._send(new_id, 'wl_seat', 'capabilities', (3 if self.has_keyboard else 1,))
                elif interface == 'zwlr_layer_shell_v1':
                    self.shell_id = new_id
            elif iface == 'wl_seat' and name == 'get_pointer':
                self.pointer_id = args[0]
                self.objects[self.pointer_id] = 'wl_pointer'
            elif iface == 'wl_seat' and name == 'get_keyboard':
                self.keyboard_id = args[0]
                self.objects[self.keyboard_id] = 'wl_keyboard'
            elif iface == 'zwlr_layer_shell_v1' and name == 'get_layer_surface':
                layer_id, surface_id, output_id, layer, namespace = args
                self.objects[layer_id] = 'zwlr_layer_surface_v1'
                self.layers[layer_id] = {'surface': surface_id, 'namespace': namespace,
                                         'layer': layer}
                self.surfaces.setdefault(surface_id, {})['layer'] = layer_id
                if layer == 1:
                    self.source_ids.append(surface_id)
                    if self.source_id is None:
                        self.source_id = surface_id
                elif layer == 2:
                    self.overlay_id = surface_id
                    self.overlay_layer_id = layer_id
            elif iface == 'wl_surface' and name == 'commit':
                surface = self.surfaces.setdefault(obj, {})
                layer = surface.get('layer')
                if layer and layer not in self.configured_layers:
                    self._configure(layer, held=(self.layers[layer]['layer'] == 2))
                elif layer:
                    self._surface_commit(obj, surface)
            elif iface == 'wl_surface' and name == 'attach':
                buffer_id, x, y = args
                self.pending_attach[obj] = buffer_id
            elif iface == 'wl_surface' and name == 'frame':
                callback = args[0]
                self.objects[callback] = 'wl_callback'
                self.frame_callbacks.append((obj, callback))
                if obj == self.overlay_id:
                    self.overlay_first_frame_seen = True
            elif iface == 'wl_compositor' and name == 'create_surface':
                surface_id = args[0]
                self.objects[surface_id] = 'wl_surface'
                self.surfaces[surface_id] = {}
            elif iface == 'wl_compositor' and name == 'create_region':
                self.objects[args[0]] = 'wl_region'
            elif iface == 'wl_shm' and name == 'create_pool':
                pool_id, fd, size = args
                self.objects[pool_id] = 'wl_shm_pool'
                self.pools[pool_id] = (fd, size, mmap.mmap(fd, size, access=mmap.ACCESS_READ))
            elif iface == 'wl_shm_pool' and name == 'create_buffer':
                bid, offset, width, height, stride, fmt = args
                self.objects[bid] = 'wl_buffer'
                self.buffers[bid] = (obj, offset, width, height, stride)
            elif iface in ('wl_shm_pool', 'wl_buffer', 'wl_region', 'zwlr_layer_surface_v1') and name == 'destroy':
                if iface == 'zwlr_layer_surface_v1':
                    info = self.layers.get(obj)
                    if info and info['layer'] == 2:
                        self.overlay_destroy_seen = True
                        self.sequence += 1
                        self.overlay_destroy_sequence = self.sequence
                self.objects.pop(obj, None)
            elif iface == 'zwlr_layer_surface_v1' and name == 'ack_configure':
                pass
            elif iface == 'wl_pointer' and name == '':
                pass
            self.cv.notify_all()

    def _configure(self, layer_id, held):
        if held:
            return
        self.configured_layers.add(layer_id)
        self._send(layer_id, 'zwlr_layer_surface_v1', 'configure',
                   (layer_id + 1000, 300, 160))

    def release_overlay_configure(self):
        with self.cv:
            layer = self.overlay_layer_id
            self.configured_layers.add(layer)
            self._send(layer, 'zwlr_layer_surface_v1', 'configure', (layer + 1000, 1920, 1080))
            self.cv.notify_all()

    def release_overlay_frame(self):
        with self.cv:
            matches = [(surface, cb) for surface, cb in self.frame_callbacks
                       if surface == self.overlay_id]
            if not matches:
                raise AssertionError('overlay has not requested its first frame callback')
            surface, cb = matches[-1]
            self._send(cb, 'wl_callback', 'done', (0,))
            self.cv.notify_all()

    def enter_and_press(self, x=10, y=40):
        with self.cv:
            if not self.pointer_id or not self.source_id:
                raise AssertionError('pointer or source surface is not ready')
            self._send(self.pointer_id, 'wl_pointer', 'enter',
                       (500, self.source_id, x * 256, y * 256))
            self.cv.notify_all()
        time.sleep(0.05)
        with self.cv:
            self._send(self.pointer_id, 'wl_pointer', 'button', (501, 1, 0x110, 1))
            self._send(self.pointer_id, 'wl_pointer', 'frame', ())
            self.cv.notify_all()

    def release_left(self):
        with self.cv:
            self.source_restored_commit = None
            self._send(self.pointer_id, 'wl_pointer', 'button', (502, 2, 0x110, 0))
            self._send(self.pointer_id, 'wl_pointer', 'frame', ())
            self.cv.notify_all()

    def _surface_commit(self, surface_id, surface):
        buffer_id = self.pending_attach.pop(surface_id, None)
        if buffer_id is None:
            return
        pool_id, offset, width, height, stride = self.buffers[buffer_id]
        _, _, mapped = self.pools[pool_id]
        visible = any(mapped[offset + row * stride + col + 3] for row in range(height)
                      for col in range(0, width * 4, 4))
        entry = {'surface': surface_id, 'buffer': buffer_id, 'visible': visible}
        self.sequence += 1
        entry['sequence'] = self.sequence
        self.commits.append(entry)
        if surface_id == self.source_id and not visible:
            self.source_transparency_commits.append(entry)
        if surface_id == self.source_id and visible and self.overlay_id is not None:
            self.source_restored_commit = entry
        self._send(buffer_id, 'wl_buffer', 'release', ())
        self.cv.notify_all()


class HandoffTests(unittest.TestCase):
    def test_drag_visibility_waits_for_preview_and_restores_before_destroy(self):
        binary = Path(os.environ.get('RS_WWIDGET_TEST_BINARY', ROOT / 'rs-wwidget'))
        if not binary.exists():
            self.skipTest('build rs-wwidget first')
        with tempfile.TemporaryDirectory(prefix='wwidget-handoff-') as temp:
            root = Path(temp)
            runtime = root / 'runtime'
            runtime.mkdir(mode=0o700)
            home = root / 'home'
            home.mkdir()
            env = {**os.environ, 'HOME': str(home), 'XDG_CONFIG_HOME': str(home / '.config'),
                   'XDG_STATE_HOME': str(home / '.state'), 'XDG_CACHE_HOME': str(home / '.cache'),
                   'XDG_RUNTIME_DIR': str(runtime), 'WAYLAND_DISPLAY': 'wayland-0'}
            cfg = root / 'config.toml'
            cfg.write_text('monitor = "FAKE-1"\n')
            server = FakeWayland(runtime).start()
            process = subprocess.Popen([str(binary), '--config', str(cfg), '--run'], env=env,
                                       stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
            try:
                server.wait_for(lambda: len(server.source_ids) >= 3 and
                                all(any(c['surface'] == sid for c in server.commits)
                                    for sid in server.source_ids),
                                'initial panel surfaces did not render')
                server.enter_and_press(x=10, y=10)
                server.wait_for(lambda: server.overlay_id is not None,
                                f'drag did not create overlay (pointer={server.pointer_id}, '
                                f'source={server.source_id}, proc={process.poll()}, '
                                f'requests={server.requests[-10:]}, '
                                f'stderr={process.stderr.read() if process.poll() is not None else "running"})')
                # The compositor withholds configure: no transparent source handoff is safe.
                time.sleep(0.15)
                self.assertFalse(server.source_transparency_commits,
                                 'source committed transparent before overlay configure')
                server.release_overlay_configure()
                server.wait_for(lambda: server.overlay_first_frame_seen and
                                any(c['surface'] == server.overlay_id for c in server.commits),
                                'overlay did not commit its preview and request a first-frame callback')
                time.sleep(0.15)
                self.assertFalse(server.source_transparency_commits,
                                 'source cleared before preview frame callback')
                server.release_overlay_frame()
                server.wait_for(lambda: bool(server.source_transparency_commits),
                                'source was not hidden after preview became visible')
                server.release_left()
                server.wait_for(lambda: server.source_restored_commit is not None,
                                'source was not restored on drag end')
                server.wait_for(lambda: server.overlay_destroy_seen,
                                'overlay was not retired after source restoration')
                self.assertLess(server.source_restored_commit['sequence'],
                                server.overlay_destroy_sequence,
                                'overlay destroy request preceded restored source commit')
            finally:
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=2)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
                server.close()
                if process.stderr:
                    process.stderr.close()


class InteractiveRoutingTests(unittest.TestCase):
    def test_content_click_reaches_provider_and_does_not_begin_drag(self):
        binary = Path(os.environ.get('RS_WWIDGET_TEST_BINARY', ROOT / 'rs-wwidget'))
        if not binary.exists(): self.skipTest('build rs-wwidget first')
        with tempfile.TemporaryDirectory(prefix='wwidget-click-') as temp:
            root = Path(temp); runtime = root/'runtime'; runtime.mkdir(mode=0o700)
            home = root/'home'; home.mkdir(); marker = root/'clicked'
            script = root/'provider.py'
            script.write_text(
                'import json,sys\nfrom pathlib import Path\n'
                'for line in sys.stdin:\n'
                ' event=json.loads(line)\n'
                ' if event.get("type")=="pointer" and event.get("pressed") and event.get("target")=="button":\n'
                f'  Path({str(marker)!r}).write_text("clicked")\n'
                ' print(json.dumps({"version":2,"elements":[{"bounds":[0,0,160,100],"id":"button","content":{"type":"rect"}}]}),flush=True)\n')
            cfg=root/'config.toml'
            cfg.write_text('widgets=[]\n[[plugins]]\nid="click-test"\nenabled=true\nprotocol=2\n'
                           'input="interactive"\ndecoration="none"\nheight=100\nwidth=160\n'
                           f'command=["python3",{json.dumps(str(script))}]\n')
            env={**os.environ,'HOME':str(home),'XDG_CONFIG_HOME':str(home/'.config'),
                 'XDG_STATE_HOME':str(home/'.state'),'XDG_RUNTIME_DIR':str(runtime),
                 'WAYLAND_DISPLAY':'wayland-0'}
            server=FakeWayland(runtime).start()
            process=subprocess.Popen([str(binary),'--config',str(cfg),'--run'],env=env,
                                     stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
            try:
                server.wait_for(lambda: server.source_id is not None and bool(server.commits), 'panel not ready')
                time.sleep(.1)
                server.enter_and_press(x=40,y=60)
                server.wait_for(marker.exists,'click did not reach provider')
                self.assertIsNone(server.overlay_id,'content click incorrectly began dragging')
                server.release_left()
                for command, field, expected in [("lock", "locked", True), ("hide", "visible", False), ("show", "visible", True)]:
                    result = subprocess.run([str(binary), 'widget', 'click-test', command], env=env,
                                            capture_output=True, text=True, timeout=5)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(json.loads(result.stdout)[field], expected)
            finally:
                process.terminate()
                try: process.wait(timeout=2)
                except subprocess.TimeoutExpired: process.kill();process.wait()
                server.close();process.stderr.close()


class KeyboardRoutingTests(unittest.TestCase):
    def test_keymap_focus_key_and_focus_loss_reach_only_opted_in_provider(self):
        binary=Path(os.environ.get('RS_WWIDGET_TEST_BINARY', ROOT/'rs-wwidget'))
        if not binary.exists(): self.skipTest('build rs-wwidget first')
        with tempfile.TemporaryDirectory(prefix='wwidget-keys-') as folder:
            root=Path(folder);runtime=root/'runtime';runtime.mkdir(mode=0o700)
            home=root/'home';home.mkdir();marker=root/'pressed';lost=root/'lost-focus'
            script=root/'provider.py'
            script.write_text('import json,sys\nfrom pathlib import Path\nseen=[]\ndef save(path,value):\n p=Path(path);tmp=p.with_suffix(".tmp");tmp.write_text(json.dumps(value));tmp.replace(p)\n'
                'for line in sys.stdin:\n'
                ' e=json.loads(line);seen.append(e.get("kind", e["type"]))\n'
                ' if e["type"]=="key" and e["pressed"]:\n'
                f'  save({str(marker)!r},{{"key":e["key"],"seen":seen}})\n'
                ' if e["type"]=="keyboard_focus" and not e["focused"]:\n'
                f'  save({str(lost)!r},seen)\n'
                ' print(json.dumps({"version":2,"title":str(len(seen))}),flush=True)\n')
            cfg=root/'config.toml'
            cfg.write_text('widgets=[]\n[[plugins]]\nid="keys-test"\nenabled=true\nprotocol=2\ninput="interactive"\nkeyboard=true\n'
                           f'command=["python3",{json.dumps(str(script))}]\n')
            env={**os.environ,'HOME':str(home),'XDG_CONFIG_HOME':str(home/'.config'),
                 'XDG_STATE_HOME':str(home/'.state'),'XDG_RUNTIME_DIR':str(runtime),'WAYLAND_DISPLAY':'wayland-0'}
            server=FakeWayland(runtime, keyboard=True).start()
            process=subprocess.Popen([str(binary),'--config',str(cfg),'--run'],env=env,
                                     stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
            try:
                server.wait_for(lambda: server.keyboard_id is not None and server.source_id is not None and bool(server.commits),'keyboard/surface not ready')
                fd=os.memfd_create('test-keymap');keymap=b'test-xkb-map\0';os.write(fd,keymap);os.lseek(fd,0,os.SEEK_SET)
                try:
                    with server.cv:
                        server._send(server.keyboard_id,'wl_keyboard','keymap',(1,fd,len(keymap)))
                        server._send(server.keyboard_id,'wl_keyboard','repeat_info',(25,400))
                        server._send(server.keyboard_id,'wl_keyboard','enter',(600,server.source_id,b''))
                        server._send(server.keyboard_id,'wl_keyboard','key',(601,123,57,1))
                        server.cv.notify_all()
                finally: os.close(fd)
                server.wait_for(marker.exists,'key event did not reach provider')
                data=json.loads(marker.read_text());self.assertEqual(data['key'],57)
                self.assertLess(data['seen'].index('keymap'),data['seen'].index('keyboard_focus'))
                self.assertLess(data['seen'].index('keyboard_focus'),data['seen'].index('key'))
                with server.cv:
                    server._send(server.keyboard_id,'wl_keyboard','leave',(602,server.source_id));server.cv.notify_all()
                server.wait_for(lost.exists,'focus loss did not reach provider')
                with server.cv:
                    server._send(server.keyboard_id,'wl_keyboard','key',(603,124,58,1));server.cv.notify_all()
                time.sleep(.1)
                self.assertEqual(json.loads(marker.read_text())['key'],57)
                # Host controls recreate/destroy surfaces while the script stays
                # alive. Input cancellation must not depend on a compositor Leave.
                lost.unlink()
                with server.cv:
                    server._send(server.pointer_id,'wl_pointer','enter',(604,server.source_id,40*256,60*256))
                    server._send(server.keyboard_id,'wl_keyboard','enter',(605,server.source_id,b''))
                    server._send(server.keyboard_id,'wl_keyboard','key',(606,125,59,1))
                    server.cv.notify_all()
                server.wait_for(lambda: json.loads(marker.read_text())['key']==59,'refocus did not reach provider')
                result=subprocess.run([str(binary),'widget','keys-test','hide'],env=env,capture_output=True,text=True,timeout=5)
                self.assertEqual(result.returncode,0,result.stderr)
                deadline=time.monotonic()+5
                while not lost.exists() and time.monotonic()<deadline:time.sleep(.01)
                self.assertTrue(lost.exists(),'hide did not cancel keyboard focus')
                self.assertIn('leave',json.loads(lost.read_text()),'hide did not cancel pointer input')
            finally:
                process.terminate()
                try: process.wait(timeout=2)
                except subprocess.TimeoutExpired: process.kill();process.wait()
                server.close();process.stderr.close()


if __name__ == '__main__':
    unittest.main()
