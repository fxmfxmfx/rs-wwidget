"""Small standard-library SDK for rs-wwidget external widgets (protocol v1/v2)."""

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


# Canvas v2. Coordinates are logical pixels, independent of monitor scale.
@dataclass(frozen=True)
class Element:
    bounds: tuple[float, float, float, float]
    content: dict
    opacity: float = 1.0

    def to_dict(self) -> dict:
        if len(self.bounds) != 4:
            raise ValueError("bounds must contain x, y, width, height")
        for n in self.bounds:
            if isinstance(n, bool) or not isinstance(n, (int, float)) or not math.isfinite(n) or abs(n) > 8192:
                raise ValueError("bounds must be finite numbers within +/-8192")
        if self.bounds[2] <= 0 or self.bounds[3] <= 0:
            raise ValueError("element width and height must be positive")
        _validate_number("opacity", self.opacity)
        _validate_content(self.content)
        return {"bounds": list(self.bounds), "opacity": self.opacity, "content": self.content}


def _color(value: str) -> None:
    if value in ("background", "foreground", "muted", "accent", "green", "border",
                 "bar_background", "grid_background", "transparent"):
        return
    if not isinstance(value, str) or not value.startswith("#") or len(value) not in (7, 9):
        raise ValueError("color must be a palette name, #RRGGBB or #RRGGBBAA")
    if any(c not in "0123456789abcdefABCDEF" for c in value[1:]):
        raise ValueError("invalid hex color")


def _range(name: str, n: float, lo: float, hi: float) -> None:
    if isinstance(n, bool) or not isinstance(n, (int, float)) or not math.isfinite(n) or not lo <= n <= hi:
        raise ValueError(f"{name} must be finite and between {lo} and {hi}")


def _validate_content(c: dict) -> None:
    if not isinstance(c, dict):
        raise TypeError("element content must be a dict")
    kind = c.get("type")
    allowed = {
        "text": {"type", "text", "size", "color", "align", "wrap"},
        "image": {"type", "path", "fit", "revision"},
        "rect": {"type", "color", "radius"},
        "line": {"type", "color", "thickness"},
        "bar": {"type", "value", "color", "background", "radius"},
        "graph": {"type", "values", "color", "fill", "thickness"},
    }
    if kind not in allowed or set(c) - allowed[kind]:
        raise ValueError("unknown content type or field")
    if "color" in c:
        _color(c["color"])
    if "background" in c:
        _color(c["background"])
    if "radius" in c:
        _range("radius", c["radius"], 0, 2048)
    if "thickness" in c:
        _range("thickness", c["thickness"], 0.1, 128)
    if kind == "text":
        _validate_text("text", c["text"], 4096)
        _range("font size", c.get("size", 16), 1, 512)
        if c.get("align", "left") not in ("left", "center", "right"):
            raise ValueError("invalid text alignment")
        if not isinstance(c.get("wrap", False), bool):
            raise TypeError("wrap must be bool")
    elif kind == "image":
        _validate_text("path", c["path"], 4096)
        if not c["path"].strip() or "\0" in c["path"] or len(c["path"].encode("utf-8")) > 4096:
            raise ValueError("invalid image path")
        if c.get("fit", "contain") not in ("contain", "cover", "stretch"):
            raise ValueError("invalid image fit")
        revision = c.get("revision", 0)
        if isinstance(revision, bool) or not isinstance(revision, int) or not 0 <= revision <= 2**64 - 1:
            raise ValueError("revision must be an unsigned 64-bit integer")
    elif kind == "bar":
        _validate_number("bar value", c["value"])
    elif kind == "graph":
        values = c["values"]
        if not 1 <= len(values) <= 512:
            raise ValueError("graph needs 1..512 values")
        for v in values:
            _validate_number("graph value", v)
        if not isinstance(c.get("fill", False), bool):
            raise TypeError("fill must be bool")


def Text(text: str, bounds, *, size=16, color="foreground", align="left", wrap=False, opacity=1.0) -> Element:
    return Element(bounds, dict(type="text", text=text, size=size, color=color, align=align, wrap=wrap), opacity)


def Image(path: str, bounds, *, fit="contain", revision=0, opacity=1.0) -> Element:
    return Element(bounds, dict(type="image", path=path, fit=fit, revision=revision), opacity)


def Rect(bounds, *, color="accent", radius=0, opacity=1.0) -> Element:
    return Element(bounds, dict(type="rect", color=color, radius=radius), opacity)


def Line(bounds, *, color="accent", thickness=1, opacity=1.0) -> Element:
    """A line from the bottom-left to the top-right of its bounding box."""
    return Element(bounds, dict(type="line", color=color, thickness=thickness), opacity)


def Bar(value: float, bounds, *, color="accent", background="bar_background", radius=0, opacity=1.0) -> Element:
    return Element(bounds, dict(type="bar", value=value, color=color, background=background, radius=radius), opacity)


def Graph(values, bounds, *, color="accent", fill=False, thickness=1, opacity=1.0) -> Element:
    return Element(bounds, dict(type="graph", values=list(values), color=color, fill=fill, thickness=thickness), opacity)


def encode_canvas(elements: Iterable[Element], *, title="") -> str:
    """Validated v2 JSON, suitable for a static frame.json or stdout."""
    _validate_text("title", title, 128)
    items = []
    for element in elements:
        if not isinstance(element, Element):
            raise TypeError("elements must contain Element instances")
        items.append(element.to_dict())
        if len(items) > 256:
            raise ValueError("at most 256 elements per frame")
    line = json.dumps(dict(version=2, title=title, elements=items),
                      ensure_ascii=False, separators=(",", ":"), allow_nan=False)
    if len(line.encode("utf-8")) > 65536:
        raise ValueError("frame must not exceed 65536 bytes")
    return line


def emit_canvas(elements: Iterable[Element], *, title="") -> None:
    """Emit a complete replacement display list; unchanged frames are heartbeats."""
    sys.stdout.write(encode_canvas(elements, title=title) + "\n")
    sys.stdout.flush()
