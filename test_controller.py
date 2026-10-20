#!/usr/bin/env python3
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parent
CONTROLLER = ROOT / 'rs-wwidget'
SLEEP = Path('/usr/bin/sleep')


class ControllerIntegrationTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.child = None
        self.children = []
        self.home = Path(self.temp.name)
        self.share = self.home / '.local/share/rs-wwidget'
        self.bin_dir = self.home / '.local/bin'
        self.binary = self.bin_dir / 'rs-wwidget'
        self.state = self.home / '.state/rs-wwidget'
        self.share.mkdir(parents=True)
        self.bin_dir.mkdir(parents=True)
        shutil.copy2(CONTROLLER, self.home / 'controller')
        shutil.copy2(SLEEP, self.binary)
        self.env = {**os.environ, 'HOME': str(self.home),
                    'XDG_STATE_HOME': str(self.home / '.state'),
                    'XDG_CONFIG_HOME': str(self.home / '.config')}

    def tearDown(self):
        children = list(self.children)
        if self.child is not None:
            children.append(self.child)
        for child in children:
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=2)
        self.temp.cleanup()

    def run_controller(self, action):
        return subprocess.run([str(self.home / 'controller'), action], env=self.env,
                              text=True, capture_output=True, timeout=5)

    def start_lock_holder(self, owner='auto', executable=None):
        """Start a small process that holds the app lock, optionally with owner JSON."""
        self.state.mkdir(parents=True, exist_ok=True)
        script = self.home / 'lock-holder.py'
        script.write_text(
            "import fcntl, json, os, signal, sys, time\n"
            "from pathlib import Path\n"
            "lock_path, ready_path, owner_arg = sys.argv[1:]\n"
            "lock = open(lock_path, 'a+')\n"
            "fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)\n"
            "if owner_arg == 'auto':\n"
            "    stat = Path('/proc/self/stat').read_text()\n"
            "    fields = stat[stat.rfind(')') + 2:].split()\n"
            "    owner = {'version': 1, 'pid': os.getpid(),\n"
            "             'start_ticks': int(fields[19]),\n"
            "             'exe': os.readlink('/proc/self/exe')}\n"
            "else:\n"
            "    owner = json.loads(owner_arg)\n"
            "lock.seek(0); lock.truncate(); json.dump(owner, lock); lock.flush()\n"
            "def stop(_signum, _frame):\n"
            "    try: Path(lock_path).parent.joinpath('status.json').unlink()\n"
            "    except FileNotFoundError: pass\n"
            "    time.sleep(0.35)\n"
            "    raise SystemExit(0)\n"
            "signal.signal(signal.SIGTERM, stop)\n"
            "Path(ready_path).touch()\n"
            "while True: time.sleep(1)\n"
        )
        ready = self.state / 'lock-holder-ready'
        process = subprocess.Popen(
            [str(executable or sys.executable), str(script),
             str(self.state / 'lock'), str(ready), owner],
            env=self.env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
        self.children.append(process)
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline and not ready.exists():
            if process.poll() is not None:
                self.fail('lock holder exited before acquiring the lock')
            time.sleep(0.01)
        self.assertTrue(ready.exists(), 'lock holder did not publish its owner record')
        return process

    def test_stop_recognizes_process_after_atomic_executable_replacement(self):
        shutil.copy2(sys.executable, self.binary)
        self.child = self.start_lock_holder(owner='{}', executable=self.binary)
        pid = self.child.pid
        self.state.mkdir(parents=True, exist_ok=True)
        (self.state / 'status.json').write_text(json.dumps({
            'pid': pid, 'updated': int(time.time()),
            'rss_kib': 100, 'pss_kib': 50,
        }))
        replacement = self.bin_dir / 'replacement'
        shutil.copy2(SLEEP, replacement)
        os.replace(replacement, self.binary)

        stopped = self.run_controller('stop')
        self.assertEqual(stopped.returncode, 0, stopped.stderr)
        self.child.wait(timeout=2)
        self.assertIsNotNone(self.child.returncode)

    def test_stale_status_does_not_select_same_executable_without_its_lock(self):
        self.child = subprocess.Popen([str(self.binary), '3600'], env=self.env,
                                      stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.state.mkdir(parents=True)
        (self.state / 'status.json').write_text(json.dumps({'pid': self.child.pid}))
        stopped = self.run_controller('stop')
        self.assertEqual(stopped.returncode, 0, stopped.stderr)
        self.assertIsNone(self.child.poll())

    def test_stop_uses_lock_owner_without_status_and_waits_for_lock_release(self):
        self.child = self.start_lock_holder()
        self.assertFalse((self.state / 'status.json').exists())

        started = time.monotonic()
        stopped = self.run_controller('stop')
        elapsed = time.monotonic() - started

        self.assertEqual(stopped.returncode, 0, stopped.stderr)
        self.assertIn('stopped', stopped.stdout.lower())
        self.assertGreaterEqual(elapsed, 0.25, 'stop returned before the owner released its lock')
        self.child.wait(timeout=2)

    def test_start_keeps_log_when_lock_holder_identity_cannot_be_validated(self):
        self.child = self.start_lock_holder(owner='{}')
        log = self.state / 'session.log'
        log.write_text('keep previous diagnostic\n')

        started = self.run_controller('start')

        self.assertNotEqual(started.returncode, 0)
        self.assertTrue(
            any(word in started.stderr.lower() for word in ('lock', 'instance', 'owner')),
            started.stderr,
        )
        self.assertEqual(log.read_text(), 'keep previous diagnostic\n')

    def test_stop_does_not_signal_pid_from_stale_lock_owner_record(self):
        self.child = subprocess.Popen([str(self.binary), '3600'], env=self.env,
                                      stdout=subprocess.DEVNULL,
                                      stderr=subprocess.DEVNULL, start_new_session=True)
        stat = Path(f'/proc/{self.child.pid}/stat').read_text()
        fields = stat[stat.rfind(')') + 2:].split()
        owner = json.dumps({
            'version': 1,
            'pid': self.child.pid,
            'start_ticks': int(fields[19]) + 1,
            'exe': str(self.binary),
        })
        self.start_lock_holder(owner=owner)

        stopped = self.run_controller('stop')

        self.assertNotEqual(stopped.returncode, 0)
        self.assertIsNone(self.child.poll(), 'controller signaled a PID with a mismatched start time')

    def test_status_reports_cached_measurement_when_process_is_not_visible(self):
        self.state.mkdir(parents=True)
        now = int(time.time())
        (self.state / 'status.json').write_text(json.dumps({
            'pid': 99999999, 'updated': now - 120,
            'rss_kib': 2048, 'pss_kib': 1024,
        }))
        result = self.run_controller('status')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('cached measurement', result.stdout.lower())
        self.assertIn('2.0 MiB', result.stdout)
        self.assertIn('1.0 MiB', result.stdout)
        match = re.search(r'; (\d+) seconds ago\.', result.stdout)
        self.assertIsNotNone(match, result.stdout)
        self.assertGreaterEqual(int(match.group(1)), 120)
        self.assertLessEqual(int(match.group(1)), int(time.time()) - now + 120)

    def test_remove_preserves_unrelated_configuration_and_user_palette(self):
        mango = self.home / '.config/mango/config.conf'
        mango.parent.mkdir(parents=True)
        mango.write_text('source = /elsewhere/custom.conf\n# keep me\n')
        user_config = self.home / '.config/rs-wwidget/config.toml'
        user_config.parent.mkdir(parents=True)
        user_config.write_text('monitor = "DP-1"\n[colors]\naccent = "#123456"\n')
        result = self.run_controller('remove')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(mango.read_text(), 'source = /elsewhere/custom.conf\n# keep me\n')
        self.assertEqual(user_config.read_text(), 'monitor = "DP-1"\n[colors]\naccent = "#123456"\n')
        self.assertFalse(self.share.exists())
        self.assertFalse(self.binary.exists())


if __name__ == '__main__':
    unittest.main()
