"""SDK wire compatibility and bounded canvas frames."""
import contextlib
import io
import json
import unittest
from sdk.python.rs_wwidget import (Bar, Element, Graph, Image, Line, Rect, Text,
                                   Row, emit, emit_canvas, encode_canvas)

class CanvasSDKTests(unittest.TestCase):
    def test_all_primitives_and_unicode(self):
        elements = [Image("постер.png", (0, 0, 360, 480), fit="cover"),
                    Rect((0, 0, 100, 100), color="#28282880", radius=12),
                    Text("Привет", (8, 8, 90, 30), size=24, align="center"),
                    Line((0, 0, 10, 10)), Bar(.5, (0, 0, 100, 8)),
                    Graph([0, .2, 1], (0, 0, 100, 40), fill=True)]
        frame = json.loads(encode_canvas(elements))
        self.assertEqual(frame["version"], 2)
        self.assertEqual([e["content"]["type"] for e in frame["elements"]],
                         ["image", "rect", "text", "line", "bar", "graph"])
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf): emit_canvas(elements)
        self.assertEqual(len(buf.getvalue().splitlines()), 1)
        self.assertEqual(json.loads(buf.getvalue()), frame)

    def test_old_sdk_still_emits_v1(self):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf): emit("Old", [Row("CPU", "10%", fraction=.1)])
        self.assertEqual(json.loads(buf.getvalue())["version"], 1)

    def test_invalid_content_rejected_before_write(self):
        for element in [Rect((0, 0, -1, 40)), Rect((0, 0, 10, 10), opacity=float('nan')),
                        Text("x", (0, 0, 10, 10), color="red"), Image("x", (0, 0, 10, 10), fit="crop"),
                        Bar(2, (0, 0, 10, 10)), Graph([], (0, 0, 10, 10)),
                        Line((0, 0, 10, 10), thickness=0), Image("x", (0, 0, 10, 10), revision=-1),
                        Element((0,0,10,10), {"type":"rect","typo":0})]:
            with self.subTest(element=element), self.assertRaises((ValueError, TypeError)):
                encode_canvas([element])
        with self.assertRaises(ValueError): encode_canvas([Rect((0, 0, 10, 10))] * 257)
        with self.assertRaises(ValueError):
            encode_canvas([Text("я" * 4096, (0,0,100,100))] * 10)


class CanvasTransformTests(unittest.TestCase):
    def test_image_transforms_encode_explicitly(self):
        item = json.loads(encode_canvas([Image("photo.png", (32, 32, 100, 200),
            rotation=-15, origin=(0, 1), clip_radius=12, position=(1, 0), filter="nearest")]))["elements"][0]
        self.assertEqual(item["rotation"], -15)
        self.assertEqual(item["origin"], [0, 1])
        self.assertEqual(item["clip_radius"], 12)
        self.assertEqual(item["content"]["position"], [1, 0])
        self.assertEqual(item["content"]["filter"], "nearest")

    def test_invalid_transforms_and_missing_content_fail(self):
        for item in [Rect((0,0,10,10), rotation=361), Rect((0,0,10,10), origin=(2,0)),
                     Rect((0,0,10,10), clip_radius=-1), Image("x", (0,0,10,10), filter="fast"),
                     Image("x", (0,0,10,10), position=(-1,0)),
                     Element((0,0,10,10), {"type":"image"})]:
            with self.subTest(item=item), self.assertRaises((ValueError, TypeError)):
                encode_canvas([item])


if __name__ == '__main__': unittest.main()
