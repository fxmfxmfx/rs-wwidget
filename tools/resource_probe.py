#!/usr/bin/env python3
"""Compare release hosts on identical synthetic Wayland outputs; Linux /proc required.

This measures host + child processes, not the Python compositor/test runner.
No display, user config, IPC commands, shell scripts or real desktop are used.
"""
import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from test_wayland_handoff import FakeWayland


def process_sample(pid):
    root = Path('/proc') / str(pid)
    status = dict(line.split(':', 1) for line in (root / 'status').read_text().splitlines() if ':' in line)
    memory = dict(line.split(':', 1) for line in (root / 'smaps_rollup').read_text().splitlines() if ':' in line)
    # comm may itself contain spaces and parentheses.
    stat = (root / 'stat').read_text().rsplit(')', 1)[1].split()
    return dict(rss_kib=int(memory['Rss'].split()[0]), pss_kib=int(memory['Pss'].split()[0]),
                threads=int(status['Threads']), cpu_ticks=int(stat[11]) + int(stat[12]),
                children=[int(p) for p in (root/'task'/str(pid)/'children').read_text().split()])


def measure(binary, seconds, rounds):
    results = []
    for _ in range(rounds):
        with tempfile.TemporaryDirectory(prefix='wwidget-resources-') as folder:
            root = Path(folder); runtime=root/'runtime'; runtime.mkdir(mode=0o700)
            home=root/'home';home.mkdir()
            config=root/'config.toml'
            config.write_text('widgets=["clock","calendar"]\nfont_family="monospace"\n')
            env={**os.environ, 'HOME':str(home), 'XDG_CONFIG_HOME':str(home/'config'),
                 'XDG_STATE_HOME':str(home/'state'), 'XDG_DATA_HOME':str(home/'data'),
                 'XDG_RUNTIME_DIR':str(runtime), 'WAYLAND_DISPLAY':'wayland-0'}
            # Ensure installed optional packages cannot affect this comparison.
            env.pop('RS_WWIDGET_CONFIG', None)
            server=FakeWayland(runtime).start()
            process=subprocess.Popen([str(binary), '--config', str(config), '--run'],env=env,
                                     stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
            try:
                server.wait_for(lambda:len(server.source_ids)==2 and len(server.commits)>=2,
                                'two built-in widgets did not render')
                time.sleep(1.0)  # Allow asynchronous font initialization to settle.
                start=process_sample(process.pid);begin=time.monotonic()
                samples=[]
                for _ in range(5):
                    time.sleep(seconds/5)
                    if process.poll() is not None:
                        raise RuntimeError(process.stderr.read().decode(errors='replace'))
                    sample=process_sample(process.pid); samples.append(sample)
                    if sample['children']: raise RuntimeError('built-in widgets spawned child processes')
                elapsed=time.monotonic()-begin
                results.append(dict(rss_kib=statistics.median(s['rss_kib'] for s in samples),
                                    pss_kib=statistics.median(s['pss_kib'] for s in samples),
                                    threads=max(s['threads'] for s in samples),children=0,
                                    idle_cpu_percent=100*(samples[-1]['cpu_ticks']-start['cpu_ticks'])/os.sysconf('SC_CLK_TCK')/elapsed))
            finally:
                process.terminate()
                try:process.wait(timeout=3)
                except subprocess.TimeoutExpired:process.kill();process.wait()
                process.stderr.close();server.close()
    return {key:statistics.median(row[key] for row in results) for key in results[0]}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline',type=Path,required=True)
    parser.add_argument('--candidate',type=Path,required=True)
    parser.add_argument('--seconds',type=float,default=6)
    parser.add_argument('--rounds',type=int,default=3)
    parser.add_argument('--max-pss-growth-kib',type=int,default=1024)
    parser.add_argument('--output',type=Path)
    args=parser.parse_args()
    if not Path('/proc/self/smaps_rollup').exists():parser.error('Linux /proc/smaps_rollup is required')
    if not 1<=args.rounds<=10 or not 1<=args.seconds<=60:parser.error('rounds: 1..10; seconds: 1..60')
    baseline=measure(args.baseline.resolve(),args.seconds,args.rounds)
    candidate=measure(args.candidate.resolve(),args.seconds,args.rounds)
    delta={key:candidate[key]-baseline[key] for key in baseline}
    report=dict(scenario='two built-in widgets; synthetic 1920x1080 scale-1 output',baseline=baseline,candidate=candidate,delta=delta)
    text=json.dumps(report,indent=2); print(text,flush=True)
    if args.output:args.output.write_text(text+'\n')
    failures=[]
    if delta['threads']>0:failures.append('additional host threads')
    if delta['pss_kib']>args.max_pss_growth_kib:failures.append('PSS growth exceeds budget')
    if candidate['idle_cpu_percent']>baseline['idle_cpu_percent']+1:failures.append('idle CPU regression > 1 percentage point')
    if failures:raise SystemExit('; '.join(failures))


if __name__=='__main__':main()
