import struct
import tempfile
import unittest
from pathlib import Path
from alpha_contract import check, read_pixels


class AlphaContractTests(unittest.TestCase):
    def test_constant_straight_passes_premultiply_mutant_fails(self):
        color = (1., .5, .125)
        good = [(*color, a) for a in (0, .125, .5, 1)]
        check(good, 1/32768, 'constant-color', color=color)
        bad = [(*(c*a for c in color), a) for a in (0, .125, .5, 1)]
        with self.assertRaises(ValueError):
            check(bad, 1/32768, 'constant-color', color=color)

    def test_alpha_only_and_identity_check_hidden_rgb(self):
        original = [(1., .2, .3, 0), (1., .2, .3, .75)]
        masked = [(1., .2, .3, 0), (1., .2, .3, .25)]
        check(masked, 1e-6, 'alpha-only', reference=original)
        with self.assertRaises(ValueError):
            check(masked, 1e-6, 'identity', reference=original)
        with self.assertRaises(ValueError):
            check([(0, 0, 0, 0), masked[1]], 1e-6, 'alpha-only', reference=original)

    def test_no_vacuous_opaque_or_empty_pass(self):
        for pixels in ([], [(1, 1, 1, 1)], [(1, 1, 1, 0)]):
            with self.assertRaises(ValueError):
                check(pixels, 1e-6, 'constant-color', color=(1, 1, 1))

    def test_16bit_scale_and_float_hdr(self):
        with tempfile.TemporaryDirectory() as temp:
            p = Path(temp)/'pixels.raw'
            p.write_bytes(struct.pack('<HHHH', 32768, 16384, 8192, 16384))
            pixels, tol = read_pixels(p, 'rgba16le')
            self.assertEqual(pixels, [(1, .5, .25, .5)])
            check(pixels, tol, 'constant-color', color=(1, .5, .25))
            p.write_bytes(struct.pack('<ffff', 4, -1, .125, .5))
            pixels, tol = read_pixels(p, 'rgba32f-le')
            check(pixels, tol, 'constant-color', color=(4, -1, .125))
            for raw in (b'', b'broken', struct.pack('<ffff', float('nan'), 0, 0, .5),
                        struct.pack('<ffff', 1, 0, 0, 2)):
                p.write_bytes(raw)
                with self.assertRaises(ValueError):
                    read_pixels(p, 'rgba32f-le')


if __name__ == '__main__':
    unittest.main()
