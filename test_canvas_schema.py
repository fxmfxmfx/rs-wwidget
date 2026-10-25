"""Optional editor-schema checks; production host/SDK do not need jsonschema."""
import json
from pathlib import Path
import unittest
from sdk.python.rs_wwidget import Image, Text, Rect, Line, Bar, Graph, encode_canvas
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
            Bar(.5, (0,0,100,8)), Graph([0,.5,1], (0,0,100,40))]))
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
        ]:
            with self.subTest(element=element):
                self.assertTrue(list(self.validator.iter_errors({'version':2, 'elements':[element]})))
