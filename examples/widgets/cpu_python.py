#!/usr/bin/env python3
"""Optional Python SDK CPU example; use cpu-monitor for lower memory usage."""

import math
import signal
import sys
import time
from pathlib import Path
from typing import Optional, Sequence, Tuple

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "sdk" / "python"))
from rs_wwidget import Row, emit


CPU_HISTORY_SIZE = 120
SAMPLE_INTERVAL = 1.0
CPU_STAT_PATH = Path("/proc/stat")
LOADAVG_PATH = Path("/proc/loadavg")
UPTIME_PATH = Path("/proc/uptime")
CpuCounters = Tuple[int, int, int, int, int, int, int, int]


def parse_cpu(line: str) -> Optional[CpuCounters]:
    """Read the aggregate CPU counters, excluding guest fields to avoid double counting."""
    fields = line.split()
    if not fields or fields[0] != "cpu" or len(fields) < 9:
        return None
    try:
        counters = tuple(int(value) for value in fields[1:9])
    except ValueError:
        return None
    if any(value < 0 for value in counters):
        return None
    return counters  # type: ignore[return-value]


def cpu_fraction(before: Optional[Sequence[int]], after: Optional[Sequence[int]]) -> Optional[float]:
    """Return busy CPU utilization for one interval, or None for invalid counters."""
    if before is None or after is None or len(before) != 8 or len(after) != 8:
        return None
    deltas = [current - previous for previous, current in zip(before, after)]
    if any(delta < 0 for delta in deltas):
        return None
    total = sum(deltas)
    if total <= 0:
        return None
    idle = deltas[3] + deltas[4]  # Linux idle plus iowait are both idle time.
    return min(1.0, max(0.0, (total - idle) / total))


def format_uptime(value: str) -> Optional[str]:
    """Format /proc/uptime's first field as days, hours, and minutes."""
    try:
        seconds = float(value.split()[0])
        if not math.isfinite(seconds) or seconds < 0:
            return None
    except (IndexError, ValueError):
        return None
    minutes = int(seconds) // 60
    days, remainder = divmod(minutes, 24 * 60)
    hours, minutes = divmod(remainder, 60)
    return f"{days}d {hours:02d}h {minutes:02d}m"


def _read_cpu() -> Optional[CpuCounters]:
    try:
        with CPU_STAT_PATH.open(encoding="ascii") as stat_file:
            return parse_cpu(stat_file.readline())
    except (OSError, UnicodeError):
        return None


def _load_average() -> str:
    try:
        with LOADAVG_PATH.open(encoding="ascii") as load_file:
            values = load_file.readline().split()
        if len(values) < 3:
            return "—"
        loads = [float(value) for value in values[:3]]
        if any(not math.isfinite(value) or value < 0 for value in loads):
            return "—"
        return " ".join(f"{value:.2f}" for value in loads)
    except (OSError, UnicodeError, ValueError):
        return "—"


def _uptime() -> str:
    try:
        with UPTIME_PATH.open(encoding="ascii") as uptime_file:
            result = format_uptime(uptime_file.readline())
        return result if result is not None else "—"
    except (OSError, UnicodeError):
        return "—"


def main() -> None:
    previous = _read_cpu()
    history = []
    emit("CPU monitor", [Row("CPU", "Sampling…"), Row("Load avg", _load_average()), Row("Uptime", _uptime())])
    deadline = time.monotonic() + SAMPLE_INTERVAL
    while True:
        # Measure at a steady one-second cadence. A delayed frame moves the next
        # sample forward instead of triggering a burst of catch-up samples.
        time.sleep(max(0.0, deadline - time.monotonic()))
        current = _read_cpu()
        utilization = cpu_fraction(previous, current)
        previous = current

        if utilization is not None:
            history.append(utilization)
            del history[:-CPU_HISTORY_SIZE]
            graph = [0.0] * (CPU_HISTORY_SIZE - len(history)) + history
            cpu_text = f"{utilization * 100:.0f}%"
        else:
            # Keep the last valid graph intact across a temporarily bad read.
            graph = [0.0] * (CPU_HISTORY_SIZE - len(history)) + history if history else None
            cpu_text = "Sampling…" if current is not None and not history else "—"

        rows = [Row("CPU", cpu_text, history=graph)]
        rows.append(Row("Load avg", _load_average()))
        rows.append(Row("Uptime", _uptime()))
        emit("CPU monitor", rows)
        deadline += SAMPLE_INTERVAL
        if deadline <= time.monotonic():
            deadline = time.monotonic() + SAMPLE_INTERVAL


if __name__ == "__main__":
    signal.signal(signal.SIGTERM, signal.SIG_DFL)
    signal.signal(signal.SIGINT, signal.SIG_DFL)
    main()
