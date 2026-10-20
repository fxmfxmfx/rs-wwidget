"""Small standard-library SDK for rs-wwidget external widgets (protocol v1)."""

from dataclasses import dataclass
import json
import math
import sys
from typing import Iterable, Optional


@dataclass(frozen=True)
class Row:
    """One display row. History values are normalized to the range 0..1."""

    label: str
    value: str
    fraction: Optional[float] = None
    history: Optional[list[float]] = None


def _validate_text(name: str, value: str, limit: int) -> None:
    if not isinstance(value, str):
        raise TypeError(f"{name} must be a string")
    if len(value) > limit:
        raise ValueError(f"{name} must be at most {limit} characters")


def _validate_number(name: str, value: float) -> None:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise TypeError(f"{name} must be a number")
    if not math.isfinite(value) or not 0 <= value <= 1:
        raise ValueError(f"{name} must be finite and between 0 and 1")


def emit(title: str, rows: Iterable[Row]) -> None:
    """Validate and write one compact JSON frame followed by a newline."""
    _validate_text("title", title, 128)
    frame_rows = []
    for row in rows:
        if not isinstance(row, Row):
            raise TypeError("rows must contain Row instances")
        _validate_text("row label", row.label, 512)
        _validate_text("row value", row.value, 512)
        item = {"label": row.label, "value": row.value}
        if row.fraction is not None:
            _validate_number("fraction", row.fraction)
            item["fraction"] = row.fraction
        if row.history is not None:
            if len(row.history) > 120:
                raise ValueError("history must contain at most 120 values")
            for value in row.history:
                _validate_number("history value", value)
            item["history"] = row.history
        frame_rows.append(item)
        if len(frame_rows) > 12:
            raise ValueError("rows must contain at most 12 entries")
    frame = {"version": 1, "title": title, "rows": frame_rows}
    line = json.dumps(frame, ensure_ascii=False, separators=(",", ":"), allow_nan=False) + "\n"
    if len(line[:-1].encode("utf-8")) > 65536:
        raise ValueError("frame must not exceed 65536 bytes")
    sys.stdout.write(line)
    sys.stdout.flush()
