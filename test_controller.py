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

ROOT = Path(__file__).resolve().parent
CONTROLLER = ROOT / 'rs-wwidget'
SLEEP = Path('/usr/bin/sleep')


class ControllerIntegrationTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.child = None
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
        self.child = subprocess.Popen([str(self.binary), '3600'], env=self.env,
                                      stdout=subprocess.DEVNULL,
                                      stderr=subprocess.DEVNULL, start_new_session=True)
        pid = self.child.pid
        self.state.mkdir(parents=True)
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
