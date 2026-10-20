#!/usr/bin/env python3
"""Check native installation in a temporary home directory."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent


class InstallationTest(unittest.TestCase):
    def test_installs_native_binary_and_preserves_custom_config(self):
        self.assertTrue((ROOT / 'install.sh').is_file(), 'portable installer is missing')
        self.assertTrue((ROOT / 'config.example.toml').is_file(), 'example config is missing')
        self.assertTrue((ROOT / 'rs-wwidget').is_file(), 'build the native binary first')
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            project, home = root / 'source', root / 'user home'
            project.mkdir()
            home.mkdir()
            for name in ('install.sh', 'config.example.toml'):
                shutil.copy2(ROOT / name, project / name)
            shutil.copy2(ROOT / 'rs-wwidget', project / 'rs-wwidget')
            shutil.copytree(ROOT / 'widgets', project / 'widgets')
            env = dict(os.environ, HOME=str(home),
                       XDG_STATE_HOME=str(home / '.state'),
                       XDG_CONFIG_HOME=str(home / '.config'))

            result = subprocess.run(['sh', str(project / 'install.sh')], env=env,
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            share = home / '.local/share/rs-wwidget'
            binary = home / '.local/bin/rs-wwidget'
            self.assertTrue(os.access(binary, os.X_OK))
            self.assertEqual(binary.read_bytes(), (project / 'rs-wwidget').read_bytes())
            self.assertTrue(os.access(share / 'widgets/widget-provider', os.X_OK))
            self.assertTrue(os.access(share / 'widgets/music/music', os.X_OK))
            self.assertTrue((share / 'widgets/network/widget.toml').is_file())
            widget_settings = share / 'widgets/network/settings.toml'
            widget_settings.write_text('interface = "custom-interface"\n')
            widget_data = share / 'widgets/network/networkcustomnotes.txt'
            widget_data.write_text('Build=Passed\n')
            for retired in ('agenda', 'summary', 'timer'):
                retired_path = share / 'widgets' / retired
                retired_path.mkdir()
                (retired_path / 'widget.toml').write_text('retired fixture\n')
            config = (share / 'mango.conf').read_text()
            self.assertIn('layerrule=noanim:1,noblur:1,noshadow:1,layer_name:rs-wwidget\n', config)
            self.assertIn(str(binary), config)
            self.assertNotIn(str(ROOT), config)
            launcher = home / '.local/share/applications/rs-wwidget.desktop'
            self.assertTrue(launcher.is_file())
            check = subprocess.run(['desktop-file-validate', str(launcher)],
                                   capture_output=True, text=True)
            self.assertEqual(check.returncode, 0, check.stderr)

            user_config = home / '.config/rs-wwidget/config.toml'
            self.assertEqual(user_config.read_text(), (project / 'config.example.toml').read_text())
            user_config.write_text('monitor = "DP-1"\n[colors]\naccent = "#123456"\n')
            result = subprocess.run(['sh', str(project / 'install.sh')], env=env,
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(user_config.read_text(), 'monitor = "DP-1"\n[colors]\naccent = "#123456"\n')
            self.assertEqual(widget_settings.read_text(), 'interface = "custom-interface"\n')
            self.assertEqual(widget_data.read_text(), 'Build=Passed\n')
            self.assertTrue(all(not (share / 'widgets' / retired).exists()
                                for retired in ('agenda', 'summary', 'timer')))
            package_config = root / 'packages.toml'
            package_config.write_text('widgets = ["network", "music", "weather"]\n')
            package_check = subprocess.run([str(binary), '--config', str(package_config), '--check-config'],
                                           env=env, capture_output=True, text=True)
            self.assertEqual(package_check.returncode, 0, package_check.stderr)
            check_config = subprocess.run([str(binary), '--check-config'], env=env,
                                          capture_output=True, text=True)
            self.assertEqual(check_config.returncode, 0, check_config.stderr)

            mango = home / '.config/mango/config.conf'
            mango.parent.mkdir(parents=True, exist_ok=True)
            mango.write_text('source = /elsewhere/custom.conf\n# preserve this\n')
            result = subprocess.run([str(binary), 'remove'], env=env,
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(share.exists())
            self.assertFalse(binary.exists())
            self.assertFalse(launcher.exists())
            self.assertTrue(user_config.exists())
            self.assertEqual(mango.read_text(), 'source = /elsewhere/custom.conf\n# preserve this\n')


if __name__ == '__main__':
    unittest.main()
