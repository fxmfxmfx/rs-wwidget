import json
import os
import pathlib
import select
import shutil
import subprocess
import tempfile
import time
import unittest


ROOT = pathlib.Path(__file__).resolve().parent


class MusicWidgetTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not shutil.which("dbus-daemon"):
            raise unittest.SkipTest("dbus-daemon is not installed")
        if not shutil.which("cc") or not shutil.which("pkg-config"):
            raise unittest.SkipTest("C compiler and pkg-config are required")
        cls.temp = tempfile.TemporaryDirectory(prefix="rs-wwidget-music-test-")
        cls.temp_path = pathlib.Path(cls.temp.name)
        cflags = subprocess.check_output(
            ["pkg-config", "--cflags", "gio-2.0"], text=True
        ).split()
        libs = subprocess.check_output(
            ["pkg-config", "--libs", "gio-2.0"], text=True
        ).split()
        cls.music = cls.temp_path / "music"
        cls.player = cls.temp_path / "test-player"
        subprocess.run(
            ["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", *cflags,
             str(ROOT / "widgets/music/music.c"), *libs, "-o", str(cls.music)],
            check=True,
        )
        subprocess.run(
            ["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", *cflags,
             str(ROOT / "widgets/music/test-player.c"), *libs, "-o", str(cls.player)],
            check=True,
        )

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def setUp(self):
        self.children = []
        self.bus = subprocess.Popen(
            ["dbus-daemon", "--session", "--nofork", "--print-address=1"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        ready, _, _ = select.select([self.bus.stdout], [], [], 3)
        self.assertTrue(ready, "private D-Bus did not start")
        address = self.bus.stdout.readline().strip()
        self.assertTrue(address, "private D-Bus did not print its address")
        self.env = os.environ.copy()
        self.env["DBUS_SESSION_BUS_ADDRESS"] = address

    def tearDown(self):
        for child in reversed(self.children):
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=2)
        if self.bus.poll() is None:
            self.bus.terminate()
            try:
                self.bus.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.bus.kill()
                self.bus.wait(timeout=2)
        for process in [*self.children, self.bus]:
            for stream in (process.stdout, process.stderr):
                if stream is not None:
                    stream.close()

    def wait_for_player(self, name, timeout=3):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            result = subprocess.run(
                [str(self.player), "--status", name], env=self.env, text=True,
                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=2,
            )
            if result.returncode == 0:
                return
            time.sleep(0.05)
        self.fail(f"MPRIS fixture did not acquire {name}")

    def start(self, command, **kwargs):
        process = subprocess.Popen(
            command, env=self.env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, **kwargs
        )
        self.children.append(process)
        return process

    def frame(self, process, timeout=5):
        ready, _, _ = select.select([process.stdout], [], [], timeout)
        self.assertTrue(ready, "music widget did not emit a frame")
        line = process.stdout.readline()
        self.assertTrue(line, "music widget exited before emitting a frame")
        return json.loads(line)

    def test_private_bus_mpris_behavior(self):
        widget = self.start([str(self.music)])
        no_player = self.frame(widget)
        self.assertEqual(no_player["rows"][0]["value"], "No active player")
        widget.terminate()
        widget.wait(timeout=2)

        paused_name = "org.mpris.MediaPlayer2.aaa_paused"
        playing_name = "org.mpris.MediaPlayer2.zzz_playing"
        paused = self.start([str(self.player), "--serve", paused_name, "Paused"])
        playing = self.start([str(self.player), "--serve", playing_name, "Playing"])
        self.wait_for_player(paused_name)
        self.wait_for_player(playing_name)

        widget = self.start([str(self.music)])
        metadata = self.frame(widget)
        rows = {row["label"]: row["value"] for row in metadata["rows"]}
        self.assertEqual(rows["Artist"], 'Artist "雪')
        self.assertEqual(rows["Track"], 'Track \\ "𝄞')
        self.assertEqual(rows["State"], "Playing")

        subprocess.run([str(self.music), "--play-pause"], env=self.env, check=True, timeout=3)
        status = subprocess.check_output(
            [str(self.player), "--status", playing_name], env=self.env, text=True, timeout=3
        ).strip()
        self.assertEqual(status, "Paused")

        self.bus.terminate()
        self.bus.wait(timeout=2)
        deadline = time.monotonic() + 5
        while True:
            disconnected = self.frame(widget, timeout=max(0.1, deadline - time.monotonic()))
            if disconnected["rows"][0]["value"] == "No session bus":
                break
            self.assertLess(time.monotonic(), deadline, "widget did not report session bus loss")


if __name__ == "__main__":
    unittest.main()
