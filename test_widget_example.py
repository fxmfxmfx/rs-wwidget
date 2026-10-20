"""Focused checks for the standard-library example widget's /proc parsing."""

import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from examples.widgets import cpu_python as example


class CpuCounterTests(unittest.TestCase):
    def test_aggregate_cpu_ignores_guest_counters(self):
        # guest and guest_nice follow the eight ordinary counters and must not
        # be counted again in the aggregate total.
        before = example.parse_cpu("cpu 100 20 30 400 50 6 7 8 900 100\n")
        after = example.parse_cpu("cpu 110 20 40 420 60 6 7 8 9999 9999\n")
        self.assertEqual(example.cpu_fraction(before, after), 0.4)

    def test_idle_includes_iowait(self):
        before = (0, 0, 0, 100, 100, 0, 0, 0)
        after = (0, 0, 10, 110, 120, 0, 0, 0)
        # 10 busy ticks, 10 idle ticks and 20 iowait ticks.
        self.assertEqual(example.cpu_fraction(before, after), 0.25)

    def test_bad_or_reset_counter_intervals_are_unavailable(self):
        valid = (1, 2, 3, 4, 5, 6, 7, 8)
        self.assertIsNone(example.cpu_fraction(None, valid))
        self.assertIsNone(example.cpu_fraction(valid, valid))
        self.assertIsNone(example.cpu_fraction(valid, (1, 2, 3, 3, 5, 6, 7, 8)))
        self.assertIsNone(example.parse_cpu("cpu 1 2 nope 4 5 6 7 8\n"))
        self.assertIsNone(example.parse_cpu("cpu 1 2 3 4 5 6 7\n"))

    def test_reads_only_first_aggregate_cpu_line(self):
        with tempfile.TemporaryDirectory() as directory:
            stat_path = Path(directory) / "stat"
            stat_path.write_text("cpu 1 2 3 4 5 6 7 8\ncpu0 99 99 99 99 99 99 99 99\n")
            with patch.object(example, "CPU_STAT_PATH", stat_path):
                self.assertEqual(example._read_cpu(), (1, 2, 3, 4, 5, 6, 7, 8))


class UptimeTests(unittest.TestCase):
    def test_formats_days_hours_and_minutes(self):
        self.assertEqual(example.format_uptime("187000.75 123.4\n"), "2d 03h 56m")
        self.assertEqual(example.format_uptime("59.9 0.0"), "0d 00h 00m")

    def test_invalid_or_unavailable_uptime_is_not_fabricated(self):
        self.assertIsNone(example.format_uptime("not uptime"))
        self.assertIsNone(example.format_uptime("-1 0"))
        self.assertIsNone(example.format_uptime("nan 0"))
        self.assertIsNone(example.format_uptime("inf 0"))
        with patch.object(example, "UPTIME_PATH", Path("/missing/widget-uptime")):
            self.assertEqual(example._uptime(), "—")


if __name__ == "__main__":
    unittest.main()
