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
    rotation: float = 0.0
    origin: tuple[float, float] = (0.5, 0.5)
    clip_radius: float = 0.0
    id: Optional[str] = None

    def to_dict(self) -> dict:
        if len(self.bounds) != 4:
            raise ValueError("bounds must contain x, y, width, height")
        for n in self.bounds:
            if isinstance(n, bool) or not isinstance(n, (int, float)) or not math.isfinite(n) or abs(n) > 8192:
                raise ValueError("bounds must be finite numbers within +/-8192")
        if self.bounds[2] <= 0 or self.bounds[3] <= 0:
            raise ValueError("element width and height must be positive")
        _validate_number("opacity", self.opacity)
        _range("rotation", self.rotation, -360, 360)
        _pair("origin", self.origin)
        _range("clip_radius", self.clip_radius, 0, 2048)
        _validate_content(self.content)
        result = {"bounds": list(self.bounds), "opacity": self.opacity,
                  "rotation": self.rotation, "origin": list(self.origin),
                  "clip_radius": self.clip_radius, "content": self.content}
        if self.id is not None:
            _validate_text("element id", self.id, 64)
            if not self.id or not self.id.isascii() or any(not (c.isalnum() or c in "_-") for c in self.id):
                raise ValueError("element id must use ASCII letters, digits, _ or -")
            result["id"] = self.id
        return result


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


def _pair(name: str, value) -> None:
    if not isinstance(value, (list, tuple)) or len(value) != 2:
        raise ValueError(f"{name} must contain two normalized coordinates")
    for n in value:
        _validate_number(name, n)


def _validate_content(c: dict) -> None:
    if not isinstance(c, dict):
        raise TypeError("element content must be a dict")
    kind = c.get("type")
    allowed = {
        "text": {"type", "text", "size", "color", "align", "wrap", "bold", "italic", "font"},
        "image": {"type", "path", "fit", "revision", "position", "filter"},
        "rect": {"type", "color", "radius"},
        "line": {"type", "color", "thickness"},
        "bar": {"type", "value", "color", "background", "radius"},
        "graph": {"type", "values", "color", "fill", "thickness"},
    }
    if not isinstance(kind, str) or kind not in allowed or set(c) - allowed[kind]:
        raise ValueError("unknown content type or field")
    required = {"text": {"text"}, "image": {"path"}, "bar": {"value"}, "graph": {"values"}}
    missing = required.get(kind, set()) - set(c)
    if missing:
        raise ValueError(f"missing {kind} fields: {', '.join(sorted(missing))}")
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
        if c.get("font") is not None:
            _validate_text("font family", c["font"], 128)
            if not c["font"].strip() or "\0" in c["font"]:
                raise ValueError("invalid font family")
        for flag in ("wrap", "bold", "italic"):
            if not isinstance(c.get(flag, False), bool):
                raise TypeError(f"{flag} must be bool")
    elif kind == "image":
        _validate_text("path", c["path"], 4096)
        if not c["path"].strip() or "\0" in c["path"] or len(c["path"].encode("utf-8")) > 4096:
            raise ValueError("invalid image path")
        if c.get("fit", "contain") not in ("contain", "cover", "stretch"):
            raise ValueError("invalid image fit")
        _pair("image position", c.get("position", (0.5, 0.5)))
        if c.get("filter", "bilinear") not in ("nearest", "bilinear", "best"):
            raise ValueError("invalid image filter")
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


def Text(text: str, bounds, *, size=16, color="foreground", align="left", wrap=False,
         bold=False, italic=False, font=None, opacity=1.0, rotation=0, origin=(0.5, 0.5), clip_radius=0, id=None) -> Element:
    return Element(bounds, dict(type="text", text=text, size=size, color=color,
                               align=align, wrap=wrap, bold=bold, italic=italic, font=font),
                   opacity, rotation, origin, clip_radius, id)


def Image(path: str, bounds, *, fit="contain", revision=0, position=(0.5, 0.5),
          filter="bilinear", opacity=1.0, rotation=0, origin=(0.5, 0.5), clip_radius=0, id=None) -> Element:
    return Element(bounds, dict(type="image", path=path, fit=fit, revision=revision,
                               position=list(position), filter=filter),
                   opacity, rotation, origin, clip_radius, id)


def Rect(bounds, *, color="accent", radius=0, opacity=1.0, rotation=0,
         origin=(0.5, 0.5), clip_radius=0, id=None) -> Element:
    return Element(bounds, dict(type="rect", color=color, radius=radius),
                   opacity, rotation, origin, clip_radius, id)


def Line(bounds, *, color="accent", thickness=1, opacity=1.0, rotation=0,
         origin=(0.5, 0.5), clip_radius=0, id=None) -> Element:
    """A line from the bottom-left to the top-right of its bounding box."""
    return Element(bounds, dict(type="line", color=color, thickness=thickness),
                   opacity, rotation, origin, clip_radius, id)


def Bar(value: float, bounds, *, color="accent", background="bar_background", radius=0,
        opacity=1.0, rotation=0, origin=(0.5, 0.5), clip_radius=0, id=None) -> Element:
    return Element(bounds, dict(type="bar", value=value, color=color,
                               background=background, radius=radius),
                   opacity, rotation, origin, clip_radius, id)


def Graph(values, bounds, *, color="accent", fill=False, thickness=1, opacity=1.0,
          rotation=0, origin=(0.5, 0.5), clip_radius=0, id=None) -> Element:
    return Element(bounds, dict(type="graph", values=list(values), color=color,
                               fill=fill, thickness=thickness),
                   opacity, rotation, origin, clip_radius, id)


def encode_canvas(elements: Iterable[Element], *, title="") -> str:
    """Validated v2 JSON, suitable for a static frame.json or stdout."""
    _validate_text("title", title, 128)
    items = []
    ids = set()
    for element in elements:
        if not isinstance(element, Element):
            raise TypeError("elements must contain Element instances")
        item = element.to_dict()
        if element.id is not None:
            if element.id in ids:
                raise ValueError(f"duplicate element id: {element.id}")
            ids.add(element.id)
        items.append(item)
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


class EventStream:
    """Single-consumer, bounded NDJSON stdin reader; no threads or dependencies.

    poll() returns an event dict, or None on timeout/EOF. Check closed for EOF.
    Only input='interactive' providers receive this channel.
    """
    def __init__(self, stream=None):
        self.stream = sys.stdin if stream is None else stream
        self.fd = self.stream.fileno()
        self.buffer = bytearray()
        self.closed = False

    def poll(self, timeout=None):
        import os
        import select
        import time
        if timeout is not None:
            if not isinstance(timeout, (int, float)) or not math.isfinite(timeout) or timeout < 0:
                raise ValueError("timeout must be a nonnegative finite number or None")
        deadline = None if timeout is None else time.monotonic() + timeout
        while True:
            end = self.buffer.find(b'\n')
            if end >= 0:
                if end > 262144:
                    raise ValueError("host event exceeds 256 KiB")
                line = bytes(self.buffer[:end])
                del self.buffer[:end + 1]
                event = json.loads(line)
                if not isinstance(event, dict) or event.get("version") != 2 or event.get("type") not in (
                    "init", "pointer", "keyboard_focus", "key", "modifiers", "keymap", "repeat_info", "custom"):
                    raise ValueError("invalid host event")
                return event
            if len(self.buffer) > 262144:
                raise ValueError("host event exceeds 256 KiB")
            if self.closed:
                if self.buffer:
                    raise ValueError("truncated host event")
                return None
            remaining = None if deadline is None else max(0, deadline - time.monotonic())
            if not select.select([self.fd], [], [], remaining)[0]:
                return None
            data = os.read(self.fd, 16384)
            if data:
                self.buffer.extend(data)
            else:
                self.closed = True


def events(timeout=5.0):
    """Yield event dicts and None on idle timeouts (use them for heartbeats)."""
    channel = EventStream()
    while not channel.closed:
        event = channel.poll(timeout)
        if not channel.closed or event is not None:
            yield event


class StateStore:
    """Optional per-provider JSON state; writes are atomic and use mode 0600."""
    def __init__(self, directory=None):
        import os
        from pathlib import Path
        if directory is None:
            directory = os.environ.get("RS_WWIDGET_STATE_DIR")
        if not directory:
            raise ValueError("provide a directory or run inside rs-wwidget")
        self.directory = Path(directory)
        self.path = self.directory / "state.json"

    def load(self, default=None):
        try:
            with self.path.open("rb") as stream:
                data = stream.read(262145)
        except FileNotFoundError:
            return default
        if len(data) > 262144:
            raise ValueError("state exceeds 256 KiB")
        return json.loads(data)

    def save(self, value):
        import os
        import tempfile
        data = json.dumps(value, ensure_ascii=False, allow_nan=False).encode("utf-8")
        if len(data) > 262144:
            raise ValueError("state exceeds 256 KiB")
        self.directory.mkdir(parents=True, exist_ok=True, mode=0o700)
        descriptor, path = tempfile.mkstemp(prefix=".state-", dir=self.directory)
        try:
            with os.fdopen(descriptor, "wb") as stream:
                stream.write(data)
            os.replace(path, self.path)
        finally:
            if os.path.exists(path):
                os.unlink(path)


def control(request, timeout=2.0):
    """Send one control request to the current host; returns the result or raises."""
    import os
    import socket
    import tempfile
    from pathlib import Path
    endpoint = os.environ.get("RS_WWIDGET_CONTROL_SOCKET")
    if not endpoint:
        raise ValueError("RS_WWIDGET_CONTROL_SOCKET is unavailable")
    data = json.dumps(request, allow_nan=False).encode("utf-8")
    if len(data) > 16384:
        raise ValueError("control request exceeds 16 KiB")
    directory = Path(endpoint).parent
    with tempfile.TemporaryDirectory(prefix="rpc-", dir=directory) as folder:
        path = str(Path(folder) / "client.sock")
        with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as client:
            client.settimeout(timeout)
            client.bind(path)
            os.chmod(path, 0o600)
            client.connect(endpoint)
            client.send(data)
            response = client.recv(16385)
    if len(response) > 16384:
        raise ValueError("oversized control response")
    result = json.loads(response)
    if not result.get("ok"):
        raise RuntimeError(result.get("error", "widget control failed"))
    return result["result"]


def theme():
    """Resolved host #RRGGBB colors for custom raster renderers and integrations."""
    import os
    return json.loads(os.environ.get("RS_WWIDGET_THEME", "{}"))
