"""Oracle tests for aexcompat_smoke.py (no harness needed)."""
import struct
import tempfile
import unittest
import zlib
from pathlib import Path

from aexcompat_smoke import check_invert, check_white_edge, edge, stripes, write_png

INPUT = [(a, 64, 192, 0) for a in (0, 64, 128, 255)]  # ARGB8, straight


class InvertOracle(unittest.TestCase):
    def test_straight_invert_passes(self):
        out = [(a, 255 - r, 255 - g, 255 - b) for a, r, g, b in INPUT]
        check_invert(INPUT, out, 255, 1)

    def test_premultiplied_invert_fails(self):
        out = [(a, round((255 - r) * a / 255), round((255 - g) * a / 255), round((255 - b) * a / 255))
               for a, r, g, b in INPUT]
        with self.assertRaises(AssertionError):
            check_invert(INPUT, out, 255, 1)

    def test_alpha_change_and_opaque_only_fixture_fail(self):
        out = [(255, 255 - r, 255 - g, 255 - b) for a, r, g, b in INPUT]
        with self.assertRaises(AssertionError):
            check_invert(INPUT, out, 255, 1)
        opaque = [(255, 1, 2, 3)]
        with self.assertRaises(AssertionError):
            check_invert(opaque, [(255, 254, 253, 252)], 255, 1)


class EdgeOracle(unittest.TestCase):
    def test_white_edge_passes_and_dark_or_red_fringe_fails(self):
        check_white_edge([(255, 255, 255, 255), (85, 255, 255, 255), (0, 9, 9, 9)], 255, 1)
        with self.assertRaises(AssertionError):  # premultiplied: darkened
            check_white_edge([(255, 255, 255, 255), (85, 85, 85, 85)], 255, 1)
        with self.assertRaises(AssertionError):  # hidden red leaked
            check_white_edge([(255, 255, 255, 255), (85, 255, 170, 170)], 255, 1)
        with self.assertRaises(AssertionError):  # no partial alpha
            check_white_edge([(255, 255, 255, 255)], 255, 1)


class Fixtures(unittest.TestCase):
    def test_fixtures_have_partial_alpha_and_hidden_colour(self):
        self.assertIn(64, stripes(4, 1)[3::4])
        e = edge(4, 1)
        self.assertEqual(e[8:12], [255, 0, 0, 0])

    def test_png_writer_roundtrip(self):
        pixels = stripes(4, 2)
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'x.png'
            write_png(path, 4, 2, pixels)
            data = path.read_bytes()
            self.assertEqual(data[:8], b'\x89PNG\r\n\x1a\n')
            width, height = struct.unpack('>II', data[16:24])
            self.assertEqual((width, height), (4, 2))
            idat = data.index(b'IDAT')
            length = struct.unpack('>I', data[idat - 4:idat])[0]
            raw = zlib.decompress(data[idat + 4:idat + 4 + length])
            rows = [raw[i * 17 + 1:(i + 1) * 17] for i in range(2)]
            self.assertEqual(list(b''.join(rows)), pixels)


if __name__ == '__main__':
    unittest.main()
