#!/usr/bin/env python3
"""Render real demo frames to a static contact sheet, without Wayland or user state."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

ROOT=Path(__file__).resolve().parents[1]


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=ROOT/'rs-wwidget')
    parser.add_argument('--output',type=Path,default=ROOT/'docs/images/addon-showcase.png')
    args=parser.parse_args();output=args.output.resolve();output.parent.mkdir(parents=True,exist_ok=True)
    spec=importlib.util.spec_from_file_location('showcase_sketchpad',ROOT/'widgets/sketchpad/main.py')
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    drawing=module.Drawing([
        dict(points=[[.08,.75],[.16,.2],[.26,.75],[.35,.2],[.45,.75]],color='accent',thickness=6),
        dict(points=[[.5,.25],[.6,.75],[.7,.25],[.8,.75],[.9,.25]],color='green',thickness=6),
        dict(points=[[.08,.9],[.9,.9]],color='foreground',thickness=2),
    ])
    provider=ROOT/'widgets/widget-provider'
    process=subprocess.Popen([str(provider),'focus','--package',str(ROOT/'widgets/focus')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    try:
        focus=json.loads(process.stdout.readline())
    finally:
        process.stdin.close()
        try:process.wait(timeout=5)
        except subprocess.TimeoutExpired:process.kill();process.wait()
        process.stdout.close();process.stderr.close()
    if process.returncode:raise SystemExit('native focus provider failed')
    gallery=[dict(bounds=[0,0,800,392],content=dict(type='rect',color='grid_background'))]
    for title,elements,x,width,height in [('Native Rust / Focus',focus['elements'],12,340,270),('Python / Vector sketchpad',[e.to_dict() for e in drawing.elements()],368,420,340)]:
        gallery.extend([dict(bounds=[x,40,width,height],content=dict(type='rect',color='background',radius=10)),
                        dict(bounds=[x+20,56,width-40,24],content=dict(type='text',text=title,size=16,color='foreground'))])
        for element in elements:
            element=dict(element);element.pop('id',None)
            element['bounds']=[element['bounds'][0]+x,element['bounds'][1]+40,*element['bounds'][2:]]
            gallery.append(element)
    gallery.append(dict(bounds=[12,10,776,24],content=dict(type='text',text='rs-wwidget / real canvas frames / no compositor',size=16,color='muted')))
    frame=output.with_suffix('.json');frame.write_text(json.dumps(dict(version=2,elements=gallery),ensure_ascii=False)+'\n')
    subprocess.run([str(args.binary.resolve()),'--render-frame',str(frame),str(output),'800','392'],check=True)
    print(output)


if __name__=='__main__':main()
