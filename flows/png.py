"""PNG files built chunk by chunk, so a flow decides every byte the server reads."""
import struct
import zlib


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


def png(width, height, rows, depth=8, colour=2):
    """An image of `rows`, its scanlines, each a filter byte and then its pixels."""
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, depth, colour, 0, 0, 0)) + chunk(
        b"IDAT", zlib.compress(rows, 1)) + chunk(b"IEND", b"")
