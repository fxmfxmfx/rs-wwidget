#!/usr/bin/env python3
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import unittest

CONTROLLER = Path(__file__).resolve().parent / 'desktop-widgets.py'
SLEEP = Path('/usr/bin/sleep')


class ControllerIntegrationTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.child = None
        self.home = Path(self.temp.name)
        self.share = self.home / '.local/share/desktop-widgets'
        self.state = self.home / '.state/desktop-widgets'
        self.share.mkdir(parents=True)
        shutil.copy2(SLEEP, self.share / 'desktop-widgets')
        shutil.copy2(CONTROLLER, self.home / 'controller')
        self.env = {**os.environ, 'HOME': str(self.home),
                    'XDG_STATE_HOME': str(self.home / '.state')}

    def tearDown(self):
        # Avoid leaving fixture processes behind if an assertion fails.
        try:
            info = json.loads((self.state / 'status.json').read_text())
            os.kill(info['pid'], 15)
            if self.child is not None:
                self.child.wait(timeout=2)
        except (OSError, ValueError, KeyError):
            pass
        self.temp.cleanup()

    def run_controller(self, action):
        return subprocess.run([str(self.home / 'controller'), action], env=self.env,
                              text=True, capture_output=True, timeout=5)

    def test_stop_recognizes_process_after_atomic_executable_replacement(self):
        self.child = subprocess.Popen([str(self.share / 'desktop-widgets'), '3600'],
                                 env=self.env, stdout=subprocess.DEVNULL,
                                 stderr=subprocess.DEVNULL, start_new_session=True)
        pid = self.child.pid
        self.state.mkdir(parents=True)
        (self.state / 'status.json').write_text(json.dumps({
            'pid': pid, 'updated': int(time.time()),
            'rss_kib': 100, 'pss_kib': 50,
        }))
        replacement = self.share / 'replacement'
        shutil.copy2(SLEEP, replacement)
        os.replace(replacement, self.share / 'desktop-widgets')

        stopped = self.run_controller('stop')
        self.assertEqual(stopped.returncode, 0, stopped.stderr)
        self.child.wait(timeout=2)
        self.assertIsNotNone(self.child.returncode)

    def test_status_reports_cached_measurement_when_process_is_not_visible(self):
        self.state.mkdir(parents=True)
        now = int(time.time())
        (self.state / 'status.json').write_text(json.dumps({
            'pid': 99999999, 'updated': now - 120,
            'rss_kib': 2048, 'pss_kib': 1024,
        }))
        result = self.run_controller('status')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('сохранённый замер', result.stdout.lower())
        self.assertIn('2.0 MiB', result.stdout)
        self.assertIn('1.0 MiB', result.stdout)
        match = re.search(r'; (\d+) с назад\.', result.stdout)
        self.assertIsNotNone(match, result.stdout)
        # The subprocess may cross a wall-clock second while starting.
        self.assertGreaterEqual(int(match.group(1)), 120)
        self.assertLessEqual(int(match.group(1)), int(time.time()) - now + 120)
        self.assertNotIn('widgets are running', result.stdout.lower())

    def test_remove_preserves_unrelated_configuration(self):
        config = self.home / '.config/mango/config.conf'
        config.parent.mkdir(parents=True)
        config.write_text('source = /elsewhere/custom.conf\n# keep me\n')
        result = self.run_controller('remove')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(config.read_text(), 'source = /elsewhere/custom.conf\n# keep me\n')
        self.assertFalse(self.share.exists())


if __name__ == '__main__':
    unittest.main()
