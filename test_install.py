#!/usr/bin/env python3
"""Check portable installation without touching the real desktop."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent


class InstallationTest(unittest.TestCase):
    def test_installs_under_selected_home_and_can_be_removed(self):
        self.assertTrue((ROOT / 'install.sh').is_file(), 'portable installer is missing')
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            project, home = root / 'source', root / 'user home'
            project.mkdir()
            home.mkdir()
            for name in ('install.sh', 'desktop-widgets.py'):
                shutil.copy2(ROOT / name, project / name)
            shutil.copy2('/usr/bin/sleep', project / 'desktop-widgets')
            env = dict(os.environ, HOME=str(home), XDG_STATE_HOME=str(home / '.state'))
            result = subprocess.run(['sh', str(project / 'install.sh')], env=env,
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            share = home / '.local/share/desktop-widgets'
            self.assertTrue(os.access(share / 'desktop-widgets', os.X_OK))
            controller = home / '.local/bin/desktop-widgets'
            self.assertTrue(os.access(controller, os.X_OK))
            config = (share / 'mango.conf').read_text()
            self.assertIn(str(controller), config)
            self.assertNotIn(str(ROOT), config)
            launcher = home / '.local/share/applications/desktop-widgets.desktop'
            self.assertTrue(launcher.is_file())
            check = subprocess.run(['desktop-file-validate', str(launcher)],
                                   capture_output=True, text=True)
            self.assertEqual(check.returncode, 0, check.stderr)
            self.assertFalse((home / '.config/mango/config.conf').exists())
            result = subprocess.run([str(controller), 'remove'], env=env,
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(share.exists())
            self.assertFalse(controller.exists())
            self.assertFalse(launcher.exists())


if __name__ == '__main__':
    unittest.main()
