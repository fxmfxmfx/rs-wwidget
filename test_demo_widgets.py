"""Exercise real addon pipes and persistent drawing state without a desktop."""
import importlib.util
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile
import unittest

ROOT=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('sketchpad',ROOT/'widgets/sketchpad/main.py')
sketchpad=importlib.util.module_from_spec(spec);spec.loader.exec_module(sketchpad)
from rs_wwidget import Polyline, encode_canvas


class DrawingTests(unittest.TestCase):
    def test_drawing_is_bounded_and_undo_custom_clear_and_restore_work(self):
        drawing=sketchpad.Drawing()
        drawing.handle(dict(type='pointer',kind='button',button=272,pressed=True,target='paper',local=[20,20]))
        for i in range(900):
            drawing.handle(dict(type='pointer',kind='motion',target='paper',local=[i%380,i%202]))
        drawing.handle(dict(type='pointer',kind='button',button=272,pressed=False))
        self.assertLessEqual(len(drawing.strokes),sketchpad.MAX_STROKES)
        self.assertTrue(all(len(s['points'])<=sketchpad.MAX_POINTS for s in drawing.strokes))
        self.assertLessEqual(len(encode_canvas(drawing.elements()).encode()),65536)
        restored=sketchpad.Drawing(drawing.strokes)
        before=len(restored.strokes)
        self.assertTrue(restored.handle(dict(type='custom',name='undo')))
        self.assertEqual(len(restored.strokes),before-1)
        self.assertTrue(restored.handle(dict(type='custom',name='clear')))
        self.assertEqual(restored.strokes,[])
        self.assertEqual(sketchpad.Drawing([None,{},dict(points='bad')]).strokes,[])

    def test_sdk_rejects_malformed_or_unbounded_vector_paths(self):
        for points,kwargs in [([],{}),([[0,0]],{}),([[0,0],[1,2]],{}),([[0,0]]*1025,{}),([[0,0],[1,1]],dict(fill='green')),([[0,0],[1,1]],dict(closed=True,fill='green'))]:
            with self.subTest(points=len(points),kwargs=kwargs):
                with self.assertRaises(ValueError):Polyline(points,(0,0,100,100),**kwargs).to_dict()

    def test_real_drawing_provider_persists_on_release(self):
        with tempfile.TemporaryDirectory() as folder:
            env={**os.environ,'RS_WWIDGET_STATE_DIR':folder,'RS_WWIDGET_SDK':str(ROOT/'sdk/python')}
            process=subprocess.Popen([sys.executable,str(ROOT/'widgets/sketchpad/main.py')],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            try:
                self.assertEqual(json.loads(process.stdout.readline())['version'],2)
                for event in [dict(kind='button',button=272,pressed=True,target='paper',local=[20,20]),dict(kind='motion',target='paper',local=[120,100]),dict(kind='button',button=272,pressed=False)]:
                    process.stdin.write((json.dumps(dict(version=2,type='pointer',**event))+'\n').encode());process.stdin.flush()
                    frame=json.loads(process.stdout.readline())
                self.assertTrue(any(e['content']['type']=='polyline' for e in frame['elements']))
                self.assertEqual(len(json.loads((Path(folder)/'state.json').read_text())),1)
                binary=ROOT/'target/debug/rs-wwidget'
                if binary.exists():
                    frame_path=Path(folder)/'frame.json';frame_path.write_text(json.dumps(frame))
                    result=subprocess.run([str(binary),'--check-frame',str(frame_path)],capture_output=True,text=True)
                    self.assertEqual(result.returncode,0,result.stderr)
            finally:
                process.terminate();process.wait(timeout=5)
                process.stdin.close();process.stdout.close();process.stderr.close()


class NativeFocusTests(unittest.TestCase):
    def test_native_event_reader_rejects_truncation_and_oversized_lines(self):
        binary=ROOT/'widgets/widget-provider'
        if not binary.exists():self.skipTest('build native providers first')
        for data,expected in [(b'{','truncated'),(b'x'*262145,'exceeds'),(b'{"version":1,"type":"init"}\n','invalid')]:
            with self.subTest(expected=expected):
                result=subprocess.run([str(binary),'focus','--package',str(ROOT/'widgets/focus')],input=data,capture_output=True,timeout=5)
                self.assertNotEqual(result.returncode,0)
                self.assertIn(expected,result.stderr.decode())

    def test_native_timer_reacts_to_click_custom_commands_and_split_stdin(self):
        binary=ROOT/'widgets/widget-provider'
        if not binary.exists():self.skipTest('build native providers first')
        process=subprocess.Popen([str(binary),'focus','--package',str(ROOT/'widgets/focus')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        def read():
            self.assertTrue(select.select([process.stdout],[],[],5)[0],'timer did not emit a frame')
            line=process.stdout.readline()
            self.assertTrue(line,'timer exited: '+process.stderr.read().decode() if process.poll() is not None else 'no frame')
            return json.loads(line)
        def send(event):
            line=(json.dumps(dict(version=2,**event))+'\n').encode()
            process.stdin.write(line[:9]);process.stdin.flush()
            process.stdin.write(line[9:]);process.stdin.flush()
            return read()
        def texts(frame):return [e['content'].get('text') for e in frame['elements']]
        try:
            self.assertIn('25:00',texts(read()))
            self.assertIn('00:02',texts(send(dict(type='custom',name='set',data={'seconds':2}))))
            frame=send(dict(type='pointer',kind='button',button=272,pressed=True,target='toggle'))
            self.assertIn('FOCUS',texts(frame))
            self.assertIn('READY / PAUSED',texts(send(dict(type='custom',name='pause'))))
            self.assertIn('01:02',texts(send(dict(type='key',key=106,pressed=True))))
            process.stdin.close();process.wait(timeout=5)
            self.assertEqual(process.returncode,0)
        finally:
            if process.poll() is None:process.terminate();process.wait(timeout=5)
            if not process.stdin.closed:process.stdin.close()
            process.stdout.close();process.stderr.close()
