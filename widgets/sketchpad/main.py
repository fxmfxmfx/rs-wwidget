#!/usr/bin/env python3
"""Bounded vector drawing demo: no image decoder, dependencies or worker threads."""
import math
import os
from pathlib import Path
import sys

sys.path.insert(0, os.environ.get('RS_WWIDGET_SDK', str(Path(__file__).resolve().parents[2] / 'sdk/python')))
from rs_wwidget import Polyline, Rect, StateStore, Text, emit_canvas, events

PAPER = (20, 104, 380, 202)
COLORS = ('accent', 'green', 'foreground')
MAX_STROKES, MAX_POINTS = 24, 24


class Drawing:
    def __init__(self, saved=None):
        self.strokes = []
        self.current = None
        self.color = 0
        self.thickness = 3
        if isinstance(saved, list):
            for stroke in saved[-MAX_STROKES:]:
                try:
                    if not isinstance(stroke, dict): continue
                    points = stroke['points']
                    if not isinstance(points, list) or not 2 <= len(points) <= MAX_POINTS: continue
                    Polyline(points, PAPER, color=stroke['color'], thickness=stroke['thickness']).to_dict()
                    self.strokes.append(stroke)
                except (KeyError, TypeError, ValueError):
                    continue

    def point(self, event):
        local = event.get('local')
        if event.get('target') != 'paper' or not isinstance(local, list) or len(local) != 2:
            return None
        if any(isinstance(n, bool) or not isinstance(n, (int, float)) or not math.isfinite(n) for n in local):
            return None
        return [round(max(0, min(1, local[0] / PAPER[2])), 4),
                round(max(0, min(1, local[1] / PAPER[3])), 4)]

    def finish(self):
        if self.current is None: return False
        if len(self.current['points']) == 1:
            self.current['points'].append(self.current['points'][0][:])
        self.strokes.append(self.current)
        self.strokes = self.strokes[-MAX_STROKES:]
        self.current = None
        return True

    def handle(self, event):
        """Return whether committed state should be persisted."""
        if not event: return False
        kind = event.get('type')
        if kind == 'custom' and event.get('name') in ('clear', 'undo'):
            return self.action(event['name'])
        if kind == 'keyboard_focus' and not event.get('focused'):
            return self.finish()
        if kind == 'key' and event.get('pressed'):
            if event.get('key') == 14: return self.action('undo')  # Backspace
            if event.get('key') == 46: return self.action('clear')  # C
        if kind != 'pointer': return False
        pointer = event.get('kind')
        if pointer == 'leave': return self.finish()
        if pointer == 'scroll' and event.get('target') == 'paper':
            self.thickness = max(1, min(12, self.thickness + (1 if event.get('value', 0) < 0 else -1)))
        if pointer == 'button' and event.get('button') == 272:
            if not event.get('pressed'): return self.finish()
            if event.get('drag_handle'): return False
            target = event.get('target')
            if target in ('clear', 'undo'): return self.action(target)
            if target == 'color': self.color = (self.color + 1) % len(COLORS)
            point = self.point(event)
            if point is not None:
                self.current = dict(points=[point], color=COLORS[self.color], thickness=self.thickness)
        if pointer == 'motion' and self.current is not None:
            point = self.point(event)
            if point is not None and point != self.current['points'][-1]:
                if len(self.current['points']) >= MAX_POINTS:
                    self.finish()
                    self.current = dict(points=[self.strokes[-1]['points'][-1]], color=COLORS[self.color], thickness=self.thickness)
                self.current['points'].append(point)
        return False

    def action(self, action):
        self.current = None
        if action == 'clear': self.strokes.clear()
        elif self.strokes: self.strokes.pop()
        return True

    def elements(self):
        elements = [
            Rect((20, 54, 112, 34), color=COLORS[self.color], radius=6, id='color'),
            Text('COLOR', (20, 61, 112, 24), align='center', color='background'),
            Rect((144, 54, 112, 34), color='bar_background', radius=6, id='undo'),
            Text('UNDO', (144, 61, 112, 24), align='center'),
            Rect((268, 54, 132, 34), color='bar_background', radius=6, id='clear'),
            Text('CLEAR', (268, 61, 132, 24), align='center'),
            Rect(PAPER, color='bar_background', radius=8, id='paper'),
        ]
        for stroke in self.strokes + ([self.current] if self.current else []):
            points = stroke['points']
            if len(points) == 1: points = points * 2
            elements.append(Polyline(points, PAPER, color=stroke['color'], thickness=stroke['thickness'], clip_radius=8))
        elements.append(Text(f'Draw / scroll width: {self.thickness} / Backspace: undo / C: clear',
                             (20, 314, 380, 18), size=11, color='muted'))
        return elements


def main():
    store = StateStore()
    try: saved = store.load([])
    except (ValueError, OSError): saved = []
    drawing = Drawing(saved)
    emit_canvas(drawing.elements(), title='Vector sketchpad')
    for event in events(timeout=5):
        if drawing.handle(event): store.save(drawing.strokes)
        emit_canvas(drawing.elements(), title='Vector sketchpad')


if __name__ == '__main__': main()
