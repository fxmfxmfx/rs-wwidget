"""Yambar integration is tested without launching a bar on the user's desktop."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib
import unittest

ROOT=Path(__file__).resolve().parent
PACKAGE=ROOT/'widgets/rs-yambar'


class YambarAddonTests(unittest.TestCase):
    def test_launcher_execs_renderer_and_preserves_paths_with_spaces(self):
        with tempfile.TemporaryDirectory(prefix='bar addon ') as folder:
            root=Path(folder); binary=root/'fake bar';config=root/'my config.toml'
            config.write_text('[bar]\nheight=28\n')
            binary.write_text(f'#!{sys.executable}\nimport json,os,sys\nprint(json.dumps(dict(pid=os.getpid(),argv=sys.argv[1:],asset=os.environ["RS_WWIDGET_ASSET_DIR"])))\n')
            binary.chmod(0o755)
            env={**os.environ,'RS_YAMBAR_BINARY':str(binary),'RS_YAMBAR_CONFIG':str(config),
                 'RS_WWIDGET_ASSET_DIR':str(PACKAGE)}
            process=subprocess.Popen(['sh',str(PACKAGE/'run.sh'),'--validate'],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            stdout,stderr=process.communicate(timeout=5)
            self.assertEqual(process.returncode,0,stderr)
            result=json.loads(stdout)
            self.assertEqual(result['pid'],process.pid,'launcher must exec, not leave a wrapper process')
            self.assertEqual(result['argv'],['-c',str(config),'--validate'])
            self.assertEqual(result['asset'],str(PACKAGE))

    def test_copied_config_keeps_modules_and_package_local_scripts(self):
        config=tomllib.loads((PACKAGE/'config.toml').read_text())
        self.assertEqual(config['bar']['height'],28)
        self.assertEqual(config['left'][0]['tag_count'],8)
        self.assertEqual([m['type'] for m in config['right']],['script','memory','pipewire','clock'])
        self.assertIn('RS_WWIDGET_ASSET_DIR',config['left'][0]['on_click'])
        self.assertIn('RS_WWIDGET_ASSET_DIR',config['right'][0]['command'][-1])
        self.assertTrue(os.access(PACKAGE/'scripts/view-tag.sh',os.X_OK))

    def test_workspace_click_and_keyboard_script_with_fake_mango(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);log=root/'log';binary=root/'mmsg'
            binary.write_text(f'#!{sys.executable}\nimport json,os,sys\nfrom pathlib import Path\nPath(os.environ["TEST_MMSG_LOG"]).write_text(json.dumps(sys.argv[1:]))\nprint(json.dumps({{"layout":"Russian"}}))\n')
            binary.chmod(0o755)
            env={**os.environ,'PATH':str(root)+os.pathsep+os.environ['PATH'],'TEST_MMSG_LOG':str(log)}
            subprocess.run(['sh',str(PACKAGE/'scripts/view-tag.sh'),'3','DP-2'],env=env,check=True,timeout=5)
            self.assertEqual(json.loads(log.read_text()),['dispatch','viewcrossmon,3,DP-2'])
            result=subprocess.run(['sh',str(PACKAGE/'scripts/kblayout.sh')],env=env,capture_output=True,text=True,check=True,timeout=5)
            self.assertEqual(result.stdout,'layout|string|RU\n\n')

    @unittest.skipUnless(os.environ.get('RS_YAMBAR_TEST_BINARY'),'optional upstream source build')
    def test_real_upstream_renderer_validates_the_ported_config(self):
        result=subprocess.run([os.environ['RS_YAMBAR_TEST_BINARY'],'--validate','-c',str(PACKAGE/'config.toml')],capture_output=True,text=True,timeout=10)
        self.assertEqual(result.returncode,0,result.stderr)
