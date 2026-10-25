"""Real pipe tests for buffered stdin events and the interactive reference provider."""
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import unittest
from sdk.python.rs_wwidget import EventStream, Rect, encode_canvas

class EventStreamTests(unittest.TestCase):
    def setUp(self):
        self.read, self.write = os.pipe()
        self.stream = os.fdopen(self.read, 'rb', buffering=0)
        self.events = EventStream(self.stream)
        self.addCleanup(self.stream.close)
        self.addCleanup(lambda: os.close(self.write) if self.write is not None else None)

    def test_buffered_multiple_events_timeout_and_eof(self):
        os.write(self.write, b'{"version":2,"type":"init"}\n{"version":2,"type":"key","key":57}\n')
        self.assertEqual(self.events.poll(0)['type'], 'init')
        self.assertEqual(self.events.poll(0)['key'], 57)
        self.assertIsNone(self.events.poll(0.001))
        os.close(self.write); self.write = None
        self.assertIsNone(self.events.poll(0))
        self.assertTrue(self.events.closed)

    def test_fragmented_and_truncated_event(self):
        os.write(self.write, b'{"version":2,')
        self.assertIsNone(self.events.poll(0.001))
        os.write(self.write, b'"type":"init"}\n')
        self.assertEqual(self.events.poll(0)['type'], 'init')
        os.write(self.write, b'{')
        os.close(self.write); self.write = None
        with self.assertRaisesRegex(ValueError, 'truncated'): self.events.poll(0)

    def test_duplicate_hotspots_fail_before_emitting(self):
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            encode_canvas([Rect((0,0,4,4), id='same'), Rect((0,0,4,4), id='same')])

class PlaygroundTests(unittest.TestCase):
    def test_click_and_key_change_real_provider_frames(self):
        root = Path(__file__).resolve().parent
        proc = subprocess.Popen([sys.executable, str(root/'widgets/playground/main.py')],
                                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        def cleanup():
            proc.terminate(); proc.wait(timeout=5)
            proc.stdin.close(); proc.stdout.close(); proc.stderr.close()
        self.addCleanup(cleanup)
        frame = json.loads(proc.stdout.readline())
        self.assertEqual(frame['version'], 2)
        self.assertTrue(any(e.get('id') == 'toggle' for e in frame['elements']))
        event = dict(version=2, type='pointer', kind='button', target='toggle',
                     button=272, pressed=True, drag_handle=False, x=40, y=60)
        proc.stdin.write((json.dumps(event)+'\n').encode()); proc.stdin.flush()
        frame = json.loads(proc.stdout.readline())
        texts = [e['content'].get('text') for e in frame['elements']]
        self.assertIn('OFF', texts)
        proc.stdin.write(b'{"version":2,"type":"key","key":106,"pressed":true}\n'); proc.stdin.flush()
        texts = [e['content'].get('text') for e in json.loads(proc.stdout.readline())['elements']]
        self.assertIn('Value: 55%', texts)

class StateStoreTests(unittest.TestCase):
    def test_atomic_state_and_size_limit(self):
        import tempfile
        from sdk.python.rs_wwidget import StateStore
        with tempfile.TemporaryDirectory() as directory:
            store=StateStore(directory)
            self.assertEqual(store.load({'count': 0}), {'count': 0})
            store.save({'count': 42, 'label': 'Привет'})
            self.assertEqual(store.load()['count'], 42)
            self.assertEqual(store.path.stat().st_mode & 0o777, 0o600)
            with self.assertRaises(ValueError): store.save('x' * 262145)
            self.assertEqual(store.load()['count'], 42)


class ScaffoldTests(unittest.TestCase):
    def test_python_starter_executes_and_persists_clicks(self):
        import tempfile
        root = Path(__file__).resolve().parent
        binary = root/'target/debug/rs-wwidget'
        if not binary.exists(): self.skipTest('cargo build first')
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)/'counter'
            result = subprocess.run([str(binary), '--init-widget', str(target)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            env = {**os.environ, 'RS_WWIDGET_SDK':str(root/'sdk/python'), 'RS_WWIDGET_STATE_DIR':str(Path(folder)/'state')}
            proc = subprocess.Popen([sys.executable,str(target/'main.py')],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            try:
                self.assertEqual(json.loads(proc.stdout.readline())['version'],2)
                event=dict(version=2,type='pointer',kind='button',target='increment',pressed=True,button=272)
                proc.stdin.write((json.dumps(event)+'\n').encode());proc.stdin.flush()
                frame=json.loads(proc.stdout.readline())
                self.assertIn('Count: 1',[e['content'].get('text') for e in frame['elements']])
                self.assertEqual(json.loads((Path(folder)/'state/state.json').read_text())['count'],1)
            finally:
                proc.terminate();proc.wait(timeout=5)
                proc.stdin.close();proc.stdout.close();proc.stderr.close()

    def test_javascript_starter_receives_clicks(self):
        import shutil
        import tempfile
        node=shutil.which('node') or os.environ.get('CODEX_PRIMARY_RUNTIME_NODE')
        if not node: self.skipTest('optional Node.js is not installed')
        root=Path(__file__).resolve().parent;binary=root/'target/debug/rs-wwidget'
        if not binary.exists(): self.skipTest('cargo build first')
        with tempfile.TemporaryDirectory() as folder:
            target=Path(folder)/'counter'
            result=subprocess.run([str(binary),'--init-widget',str(target),'--backend','javascript'],capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stderr)
            proc=subprocess.Popen([node,str(target/'main.mjs')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            try:
                self.assertTrue(select.select([proc.stdout],[],[],5)[0])
                self.assertEqual(json.loads(proc.stdout.readline())['version'],2)
                event=dict(version=2,type='pointer',kind='button',target='increment',pressed=True,button=272)
                proc.stdin.write((json.dumps(event)+'\n').encode());proc.stdin.flush()
                self.assertTrue(select.select([proc.stdout],[],[],5)[0])
                texts=[e['content'].get('text') for e in json.loads(proc.stdout.readline())['elements']]
                self.assertIn('Count: 1',texts)
            finally:
                proc.terminate();proc.wait(timeout=5)
                proc.stdin.close();proc.stdout.close();proc.stderr.close()
