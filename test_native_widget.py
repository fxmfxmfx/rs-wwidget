"""Exercise the native widget over its public JSON stream with controlled proc data."""

import json
import os
from pathlib import Path
import select
import shlex
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent


class NativeWidgetTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory(prefix="native-widget-")
        cls.addClassCleanup(cls.directory.cleanup)
        cls.root = Path(cls.directory.name)
        cls.binary = cls.root / "cpu-monitor"
        subprocess.run([*shlex.split(os.environ.get("CC", "cc")), "-std=c11", "-O2",
                        "-Wall", "-Wextra", "-Werror", str(ROOT / "examples/widgets/cpu.c"),
                        "-o", str(cls.binary)], check=True, capture_output=True)

    def start(self, command):
        process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)

        def cleanup():
            if process.poll() is None:
                process.terminate()
            process.wait(timeout=5)
            process.stdout.close()
            process.stderr.close()

        self.addCleanup(cleanup)
        return process

    def frame(self, process):
        self.assertTrue(select.select([process.stdout], [], [], 4)[0], "widget stopped emitting frames")
        line = process.stdout.readline()
        self.assertTrue(line, "widget exited before emitting a frame")
        self.assertLessEqual(len(line), 65537)
        data = json.loads(line)
        self.assertEqual(data["version"], 1)
        return data["rows"]

    def write_stat(self, contents):
        temporary = self.root / "stat.new"
        temporary.write_text(contents)
        temporary.replace(self.root / "stat")

    def test_real_samples_guest_exclusion_and_recovery(self):
        self.write_stat("cpu 100 0 0 900 0 0 0 0 999 999\n")
        (self.root / "loadavg").write_text("1.25 2.50 3.75 1/100 42\n")
        (self.root / "uptime").write_text("187000.75 123.4\n")
        process = self.start([str(self.binary), "--proc-root", str(self.root)])
        rows = self.frame(process)
        self.assertEqual(rows[0]["value"], "Sampling…")
        self.assertEqual(rows[1]["value"], "1.25 2.50 3.75")
        self.assertEqual(rows[2]["value"], "2d 03h 56m")

        self.write_stat("cpu 140 0 0 960 0 0 0 0 99999 99999\n")
        rows = self.frame(process)
        self.assertEqual(rows[0]["value"], "40%")
        self.assertEqual(len(rows[0]["history"]), 120)
        self.assertEqual(rows[0]["history"][-1], 0.4)
        self.assertEqual(rows[0]["history"][:-1], [0] * 119)

        # Iowait joins idle: 10 busy, 10 idle, 20 iowait ticks = 25%.
        self.write_stat("cpu 150 0 0 970 20 0 0 0\n")
        rows = self.frame(process)
        self.assertEqual(rows[0]["value"], "25%")
        self.assertEqual(rows[0]["history"][-2:], [0.4, 0.25])

        self.write_stat("cpu -1 0 0 970 20 0 0 0\n")
        (self.root / "loadavg").write_text("nan 1 2\n")
        (self.root / "uptime").write_text("inf 0\n")
        rows = self.frame(process)
        self.assertEqual([row["value"] for row in rows], ["—", "—", "—"])
        self.assertEqual(rows[0]["history"][-2:], [0.4, 0.25])

        self.write_stat("cpu 200 0 0 1000 20 0 0 0\n")
        self.assertEqual(self.frame(process)[0]["value"], "—")
        self.write_stat("cpu 220 0 0 1080 20 0 0 0\n")
        self.assertEqual(self.frame(process)[0]["value"], "20%")

    def test_existing_python_command_replaces_the_interpreter(self):
        launcher = self.root / "example.py"
        shutil.copyfile(ROOT / "examples/widgets/example.py", launcher)
        process = self.start([sys.executable, str(launcher)])
        self.frame(process)
        self.assertEqual(Path(f"/proc/{process.pid}/exe").resolve(), self.binary)
        self.assertEqual(len(list(Path(f"/proc/{process.pid}/task").iterdir())), 1)
