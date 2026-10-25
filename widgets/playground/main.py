#!/usr/bin/env python3
"""Interactive reference provider: buttons, slider, hover and physical key events."""
from pathlib import Path
import os
import sys

sys.path.insert(0, os.environ.get('RS_WWIDGET_SDK', str(Path(__file__).resolve().parents[2] / 'sdk/python')))
from rs_wwidget import Bar, Graph, Rect, Text, emit_canvas, events


def main():
    value = 0.5
    enabled = True
    hover = None
    held = False
    focused = False

    def draw():
        emit_canvas([
            Rect((24, 56, 144, 42), color='green' if enabled else 'bar_background',
                 radius=8, id='toggle'),
            Text('ON' if enabled else 'OFF', (24, 65, 144, 24), align='center',
                 bold=True, color='background' if enabled else 'foreground'),
            Rect((192, 56, 144, 42), color='accent' if hover == 'reset' else 'bar_background',
                 radius=8, id='reset'),
            Text('RESET', (192, 65, 144, 24), align='center'),
            Text(f'Value: {value:.0%}', (24, 114, 312, 28), size=22),
            Bar(value, (24, 150, 312, 20), radius=10, id='slider'),
            Graph([0.15, value, 0.3, value, 0.8 if enabled else 0.2], (24, 190, 312, 36),
                  color='green' if enabled else 'muted', thickness=2),
            Text('Keys: arrows / space' if focused else 'Click to focus; scroll to adjust',
                 (24, 244, 312, 22), size=12, color='muted'),
        ], title='Interactive playground')

    draw()
    for event in events(timeout=5):
        if event:
            if event['type'] == 'keyboard_focus':
                focused = event['focused']
            elif event['type'] == 'key' and event['pressed']:
                if event['key'] == 57: enabled = not enabled  # Linux KEY_SPACE
                if event['key'] == 105: value -= 0.05         # KEY_LEFT
                if event['key'] == 106: value += 0.05         # KEY_RIGHT
            elif event['type'] == 'pointer':
                kind, target = event['kind'], event['target']
                hover = target
                if kind == 'leave': hover, held = None, False
                elif kind == 'button' and event['button'] == 272:
                    if event['pressed'] and not event['drag_handle']:
                        if target == 'toggle': enabled = not enabled
                        elif target == 'reset': value = 0.5
                        elif target == 'slider': held = True
                    else: held = False
                if held and kind in ('motion', 'button'):
                    value = (event['x'] - 24) / 312
                if kind == 'scroll' and not event['drag_handle']:
                    value -= event['value'] / 200
            value = min(1, max(0, value))
        draw()  # Identical idle frames keep the process alive without redrawing.


if __name__ == '__main__':
    main()
