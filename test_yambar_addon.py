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


@unittest.skipUnless(os.environ.get('RS_YAMBAR_TEST_BINARY'),'optional upstream source build')
class RealYambarWaylandTests(unittest.TestCase):
    def test_original_bar_renders_28px_surface_and_workspace_click_runs_local_script(self):
        import mmap
        import signal
        import time
        from test_wayland_handoff import FakeWayland

        class BarServer(FakeWayland):
            def _configure(self, layer_id, held):
                self.configured_layers.add(layer_id)
                self._send(layer_id,'zwlr_layer_surface_v1','configure',(layer_id+1000,1920,28))

            def _request(self,obj,iface,opcode,payload,fds):
                name,args=self._decode(iface,opcode,payload,fds)
                if iface=='wl_shm_pool' and name=='resize':
                    # The assertion thread reads pixels under the same lock.
                    # Publish the replacement before closing the old mapping.
                    with self.cv:
                        fd,_,mapped=self.pools[obj]
                        self.pools[obj]=(fd,args[0],mmap.mmap(fd,args[0],access=mmap.ACCESS_READ))
                        mapped.close()
                else:
                    super()._request(obj,iface,opcode,payload,fds)

        with tempfile.TemporaryDirectory(prefix='yambar-wayland-') as folder:
            root=Path(folder);runtime=root/'runtime';runtime.mkdir(mode=0o700)
            home=root/'home';home.mkdir();bin_dir=root/'bin';bin_dir.mkdir();marker=root/'click'
            mmsg=bin_dir/'mmsg'
            mmsg.write_text(f'#!{sys.executable}\nimport json,os,sys,time\nfrom pathlib import Path\n'
                'args=sys.argv[1:]\n'
                'if args and args[0]=="dispatch":\n'
                ' Path(os.environ["TEST_CLICK"]).write_text(json.dumps(args));sys.exit(0)\n'
                'if "keyboardlayout" in args:print("{\\"layout\\":\\"English (US)\\"}");sys.exit(0)\n'
                'if "tags" in args:\n'
                ' print(json.dumps({"tags":[{"tag_index":i,"is_active":i==1,"client_count":1 if i<4 else 0} for i in range(1,9)]}),flush=True)\n'
                ' if args[0]=="watch":time.sleep(30)\n'
                'else:print("{}")\n')
            mmsg.chmod(0o755)
            wpctl=bin_dir/'wpctl';wpctl.write_text('#!/bin/sh\nprintf "Volume: 0.50\\n"\n');wpctl.chmod(0o755)
            env={**os.environ,'HOME':str(home),'XDG_RUNTIME_DIR':str(runtime),'XDG_CONFIG_HOME':str(home/'config'),
                 'XDG_STATE_HOME':str(home/'state'),'XDG_CACHE_HOME':str(home/'cache'),'WAYLAND_DISPLAY':'wayland-0',
                 'RS_YAMBAR_BINARY':os.environ['RS_YAMBAR_TEST_BINARY'],'RS_WWIDGET_ASSET_DIR':str(PACKAGE),
                 'RS_YAMBAR_CONFIG':str(PACKAGE/'config.toml'),'TEST_CLICK':str(marker),
                 'PATH':str(bin_dir)+os.pathsep+os.environ['PATH']}
            server=BarServer(runtime).start()
            process=subprocess.Popen(['sh',str(PACKAGE/'run.sh')],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,start_new_session=True)
            try:
                server.wait_for(lambda:server.pointer_id is not None and any(c['visible'] for c in server.commits),'original bar did not render')
                with server.cv:
                    entry=server.commits[-1]
                    pool,offset,width,height,stride=server.buffers[entry['buffer']]
                    self.assertEqual((width,height),(1920,28))
                    self.assertEqual(bytes(server.pools[pool][2][offset:offset+4]),bytes([0x28,0x28,0x28,255]))
                    surface=server.layers[next(iter(server.layers))]['surface']
                    server._send(server.pointer_id,'wl_pointer','enter',(500,surface,60*256,14*256))
                    server._send(server.pointer_id,'wl_pointer','frame',())
                deadline=time.monotonic()+5
                while not marker.exists() and time.monotonic()<deadline:
                    with server.cv:
                        server._send(server.pointer_id,'wl_pointer','button',(501,1,272,1))
                        server._send(server.pointer_id,'wl_pointer','frame',())
                        server._send(server.pointer_id,'wl_pointer','button',(502,2,272,0))
                        server._send(server.pointer_id,'wl_pointer','frame',())
                    time.sleep(.1)
                self.assertTrue(marker.exists(),'workspace click did not reach the bundled view-tag.sh')
                self.assertEqual(json.loads(marker.read_text()),['dispatch','viewcrossmon,3,DP-1'])
            finally:
                try:os.killpg(process.pid,signal.SIGTERM)
                except ProcessLookupError:pass
                try:process.wait(timeout=3)
                except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()
                stderr=process.stderr.read().decode(errors='replace')
                process.stderr.close();server.close()
                if process.returncode not in (0,-signal.SIGTERM):self.fail(stderr)
