"""A minimal OpenEXR reader: scanline images, single- or multi-part,
uncompressed (codec NONE), HALF or FLOAT channels. Enough to read what
units.py has Cycles write (a multilayer EXR of the beauty and its AOVs, one
part per pass) without the compositor, which needs a GPU context headless
Blender does not have.

    read(path) -> {channel name: float32 array (height, width)}
"""
import struct

import numpy as np

_TYPES = {1: "<f2", 2: "<f4"}


def _header(data, pos):
    """One header's attributes from `pos`: ({name: (type, bytes)}, next pos)."""
    attrs = {}
    while data[pos] != 0:
        end = data.index(b"\0", pos)
        name = data[pos:end].decode()
        pos = end + 1
        end = data.index(b"\0", pos)
        kind = data[pos:end].decode()
        pos = end + 1
        size = struct.unpack_from("<i", data, pos)[0]
        pos += 4
        attrs[name] = (kind, data[pos:pos + size])
        pos += size
    return attrs, pos + 1


def _channels(value):
    out, p = [], 0
    while value[p] != 0:
        e = value.index(b"\0", p)
        name = value[p:e].decode()
        ptype, _, xs, ys = struct.unpack_from("<iB3xii", value, e + 1)
        if (xs, ys) != (1, 1):
            raise ValueError("subsampled channels are not supported")
        if ptype not in _TYPES:
            raise ValueError("UINT channels are not supported")
        out.append((name, ptype))
        p = e + 1 + 16
    return out


def read(path):
    with open(path, "rb") as fh:
        data = fh.read()
    if struct.unpack_from("<I", data, 0)[0] != 20000630:
        raise ValueError("%s: not an OpenEXR file" % path)
    flags = struct.unpack_from("<I", data, 4)[0]
    if flags & 0xA00:
        raise ValueError("%s: tiled or deep EXR is not supported" % path)
    multi = bool(flags & 0x1000)
    pos, headers = 8, []
    while True:
        attrs, pos = _header(data, pos)
        headers.append(attrs)
        if not multi or data[pos] == 0:
            pos += 1 if multi else 0
            break
    out = {}
    for part, attrs in enumerate(headers):
        if attrs["compression"][1][0] != 0:
            raise ValueError("%s: compressed EXR; write it with codec NONE" % path)
        if attrs.get("type", ("", b"scanlineimage"))[1].rstrip(b"\0") != b"scanlineimage":
            raise ValueError("%s: only scanline parts are supported" % path)
        x0, y0, x1, y1 = struct.unpack_from("<iiii", attrs["dataWindow"][1])
        w, h = x1 - x0 + 1, y1 - y0 + 1
        chans = _channels(attrs["channels"][1])
        offsets = np.frombuffer(data, "<u8", h, pos)
        pos += 8 * h
        line = np.dtype([(n, _TYPES[t], (w,)) for n, t in chans])
        planes = {n: np.empty((h, w), np.float32) for n, _ in chans}
        for off in offsets:
            off = int(off)
            if multi:
                if struct.unpack_from("<i", data, off)[0] != part:
                    raise ValueError("%s: chunk of another part" % path)
                off += 4
            y = struct.unpack_from("<i", data, off)[0] - y0
            row = np.frombuffer(data, line, 1, off + 8)[0]
            for n, _ in chans:
                planes[n][y] = row[n]
        out.update(planes)
    return out
