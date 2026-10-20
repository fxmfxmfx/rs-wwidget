"""Native package discovery and provider checks without the user's desktop or network."""
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent
NAMES = ["cpu", "network", "temperatures", "music", "weather"]


class WidgetPackageTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="widget-packages-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.env = {**os.environ, "HOME": str(self.root), "XDG_CONFIG_HOME": str(self.root / "config"),
                    "XDG_STATE_HOME": str(self.root / "state"), "XDG_DATA_HOME": str(self.root / "data"),
                    "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + str(self.root / "no-bus")}
        self.curl_log = self.root / "curl.log"
        fake_bin = self.root / "bin"
        fake_bin.mkdir()
        curl = fake_bin / "curl"
        curl.write_text(f"#!{sys.executable}\n" +
            "import json, os, sys\nfrom pathlib import Path\n"
            "with Path(os.environ['WIDGET_TEST_CURL_LOG']).open('a') as log: log.write(sys.argv[-1]+'\\n')\n"
            "if 'ipwho.is' in sys.argv[-1]:\n"
            " print(json.dumps({'success':True,'latitude':43.12,'longitude':131.89,'city':'Vladivostok','country':'Russia'}))\n"
            "else:\n"
            " print(json.dumps({'current':{'temperature_2m':12.5,'apparent_temperature':10.0,'rain':0.1},'hourly':{'precipitation_probability':[35]}}))\n")
        curl.chmod(0o755)
        self.env["PATH"] = str(fake_bin) + os.pathsep + os.environ.get("PATH", "")
        self.env["WIDGET_TEST_CURL_LOG"] = str(self.curl_log)

    def start(self, name, *actions):
        command = [str(ROOT / "widgets/widget-provider"), name, "--package", str(ROOT / "widgets" / name), *actions]
        if name == "music": command = [str(ROOT / "widgets/music/music"), *actions]
        if name == "cpu": command = [str(ROOT / "widgets/cpu/cpu-monitor"), *actions]
        process = subprocess.Popen(command, env=self.env, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, start_new_session=True, bufsize=0)

        def cleanup():
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
            process.wait(timeout=5)
            process.stdout.close()
            process.stderr.close()

        self.addCleanup(cleanup)
        return process

    def frame(self, process):
        self.assertTrue(select.select([process.stdout], [], [], 5)[0], "provider did not emit a frame")
        line = process.stdout.readline()
        if not line:
            self.fail("provider exited: " + process.stderr.read().decode())
        self.assertLessEqual(len(line), 65537)
        frame = json.loads(line)
        self.assertEqual(frame["version"], 1)
        self.assertLessEqual(len(frame["title"]), 128)
        self.assertLessEqual(len(frame["rows"]), 12)
        for row in frame["rows"]:
            for key in ("label", "value"):
                self.assertLessEqual(len(row.get(key, "")), 512)
            if "fraction" in row:
                self.assertTrue(0 <= row["fraction"] <= 1)
            self.assertLessEqual(len(row.get("history", [])), 120)
            self.assertTrue(all(0 <= value <= 1 for value in row.get("history", [])))
        return frame

    def test_selecting_names_resolves_packages_without_launching_them(self):
        config = self.root / "config.toml"
        config.write_text("widgets = " + json.dumps(["clock", *NAMES]) + "\n")
        result = subprocess.run([str(ROOT / "rs-wwidget"), "--config", str(config), "--check-config"],
                                env=self.env, capture_output=True, text=True, timeout=5)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(self.curl_log.exists(), "config validation launched weather")
        listing = subprocess.check_output([str(ROOT / "rs-wwidget"), "--list-widgets"], env=self.env, text=True)
        self.assertTrue(set(NAMES).issubset(listing.splitlines()))

    def test_native_packages_emit_valid_frames(self):
        for name in [n for n in NAMES if n != "weather"]:
            with self.subTest(name=name):
                process = self.start(name)
                self.frame(process)
                process.terminate()
                process.wait(timeout=5)
        self.assertFalse(self.curl_log.exists())

    def test_weather_ip_lookup_and_location_cache_without_network(self):
        process = self.start("weather")
        for _ in range(4):
            frame = self.frame(process)
            rows = {row["label"]: row["value"] for row in frame["rows"]}
            if "Temperature" in rows: break
        self.assertEqual(rows["Temperature"], "12.5°C")
        self.assertIn("Vladivostok", rows["Location"])
        process.terminate()
        process.wait(timeout=5)
        self.assertTrue((self.root / "state/rs-wwidget/weather-location.json").is_file())
        process = self.start("weather")
        self.frame(process)
        process.terminate()
        process.wait(timeout=5)
        calls = self.curl_log.read_text().splitlines()
        self.assertEqual(sum("ipwho.is" in call for call in calls), 1)
        self.assertTrue(any("latitude=43.12" in call and "longitude=131.89" in call for call in calls))

if __name__ == "__main__":
    unittest.main()
