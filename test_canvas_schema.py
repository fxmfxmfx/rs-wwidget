"""Optional editor-schema checks; production host/SDK do not need jsonschema."""
import json
from pathlib import Path
import unittest
from sdk.python.rs_wwidget import Image, Text, Rect, Line, Bar, Graph, Polyline, encode_canvas
try:
    from jsonschema import Draft202012Validator
except ImportError:
    Draft202012Validator = None

@unittest.skipIf(Draft202012Validator is None, 'optional jsonschema package is not installed')
class CanvasSchemaTests(unittest.TestCase):
    def setUp(self):
        schema = json.loads((Path(__file__).parent / 'sdk/schema/frame-v2.schema.json').read_text())
        Draft202012Validator.check_schema(schema)
        self.validator = Draft202012Validator(schema)

    def test_sdk_and_bundled_poster_match_schema(self):
        frame = json.loads(encode_canvas([
            Image('poster.png', (0,0,360,480), rotation=-15, clip_radius=12, filter='best'),
            Text('Hi', (0,0,100,32), bold=True), Rect((0,0,32,32)), Line((0,0,32,32)),
            Bar(.5, (0,0,100,8)), Graph([0,.5,1], (0,0,100,40)),
            Polyline([[0,0],[1,0],[0.5,1]], (0,0,64,64), closed=True, fill="green")]))
        self.validator.validate(frame)
        self.validator.validate(json.loads((Path(__file__).parent/'widgets/poster/frame.json').read_text()))

    def test_bad_fields_and_transform_limits_fail_schema(self):
        for element in [
            {'bounds':[0,0,0,10], 'content':{'type':'rect'}},
            {'bounds':[0,0,10,10], 'rotation':361, 'content':{'type':'rect'}},
            {'bounds':[0,0,10,10], 'origin':[0,2], 'content':{'type':'rect'}},
            {'bounds':[0,0,10,10], 'content':{'type':'image','path':'x','filter':'fast'}},
            {'bounds':[0,0,10,10], 'content':{'type':'image','path':'  '}},
            {'bounds':[0,0,10,10], 'content':{'type':'image','path':'x\0y'}},
            {'bounds':[0,0,10,10], 'content':{'type':'text','text':'x','typo':True}},
            {'bounds':[0,0,10,10], 'content':{'type':'polyline','points':[[0,0]]}},
            {'bounds':[0,0,10,10], 'content':{'type':'polyline','points':[[0,0],[1,2]]}},
            {'bounds':[0,0,10,10], 'content':{'type':'polyline','points':[[0,0],[1,1]],'fill':'green'}},
            {'bounds':[0,0,10,10], 'content':{'type':'polyline','points':[[0,0],[1,1]],'closed':True,'fill':'green'}},
        ]:
            with self.subTest(element=element):
                self.assertTrue(list(self.validator.iter_errors({'version':2, 'elements':[element]})))

@unittest.skipIf(Draft202012Validator is None, 'optional jsonschema package is not installed')
class ManifestSchemaTests(unittest.TestCase):
    def test_bundled_manifests_match_editor_schema(self):
        try: import tomllib
        except ImportError: self.skipTest('tomllib requires Python 3.11+')
        root=Path(__file__).parent
        schema=json.loads((root/'sdk/schema/widget.schema.json').read_text())
        Draft202012Validator.check_schema(schema)
        validator=Draft202012Validator(schema)
        for path in (root/'widgets').glob('*/widget.toml'):
            with self.subTest(path=path): validator.validate(tomllib.loads(path.read_text()))

    def test_invalid_interactive_and_external_combinations_fail(self):
        root=Path(__file__).parent
        validator=Draft202012Validator(json.loads((root/'sdk/schema/widget.schema.json').read_text()))
        for values in [dict(input='interactive'),dict(keyboard=True),dict(render_mode='external'),dict(input='none',enabled=True)]:
            self.assertTrue(list(validator.iter_errors(dict(version=2,id='sample',command=['program'],**values))))
