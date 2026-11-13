"""AE straight-alpha boundary checks, v1. No AE, builds or network access.

Canonical source: onmk-aeplugin-rust-template/scripts/alpha_contract.py.
Monorepo keeps a byte-identical copy; --compare-tool detects drift explicitly.
Input is packed, little-endian raw RGBA, not PNG or native ARGB world bytes.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct


def read_pixels(path, fmt):
    code, scale = {'rgba8': ('BBBB', 255), 'rgba16le': ('HHHH', 32768),
                   'rgba32f-le': ('ffff', 1)}[fmt]
    raw = Path(path).read_bytes()
    size = struct.calcsize('<' + code)
    if not raw or len(raw) % size:
        raise ValueError('empty or misaligned packed RGBA buffer')
    pixels = [tuple(c / scale for c in p) for p in struct.iter_unpack('<' + code, raw)]
    for p in pixels:
        if not all(math.isfinite(c) for c in p) or not 0 <= p[3] <= 1:
            raise ValueError('nonfinite channel or alpha outside [0,1]')
        if scale != 1 and any(not 0 <= c <= 1 for c in p):
            raise ValueError('integer channel outside AE range (16-bit white is 32768)')
    return pixels, 1 / scale if scale != 1 else 1e-6


def check(actual, tolerance, contract, reference=None, color=None):
    if not actual or not any(0.01 < p[3] < 0.99 for p in actual):
        raise ValueError('fixture must contain partial alpha; opaque-only is not evidence')
    if contract == 'constant-color':
        if color is None or len(color) != 3 or not all(math.isfinite(c) for c in color):
            raise ValueError('constant-color requires three finite --color values')
        pairs = [(p[:3], color) for p in actual if p[3] > 0]
    else:
        if reference is None or len(reference) != len(actual):
            raise ValueError('reference dimensions/length must match')
        channels = 4 if contract == 'identity' else 3
        # Hidden RGB also matters for identity and alpha-only operations.
        pairs = [(p[:channels], q[:channels]) for p, q in zip(actual, reference)]
    error = max(abs(a - b) for p, q in pairs for a, b in zip(p, q))
    if error > tolerance + 1e-12:
        raise ValueError(f'{contract} violated: maximum channel error {error:g}, tolerance {tolerance:g}')
    return {'contract': contract, 'pixels': len(actual), 'max_error': error,
            'partial_alpha_pixels': sum(0 < p[3] < 1 for p in actual)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--compare-tool', type=Path)
    parser.add_argument('--raw', type=Path)
    parser.add_argument('--format', choices=['rgba8', 'rgba16le', 'rgba32f-le'])
    parser.add_argument('--contract', choices=['constant-color', 'identity', 'alpha-only'])
    parser.add_argument('--reference', type=Path)
    parser.add_argument('--color', nargs=3, type=float)
    args = parser.parse_args()
    try:
        if args.compare_tool:
            if args.compare_tool.read_bytes() != Path(__file__).read_bytes():
                raise ValueError('contract tool copies differ; sync deliberately and rerun tests')
            print('contract tool copies match')
            return 0
        if not all([args.raw, args.format, args.contract]):
            parser.error('--raw, --format and --contract are required')
        actual, tolerance = read_pixels(args.raw, args.format)
        reference = read_pixels(args.reference, args.format)[0] if args.reference else None
        report = check(actual, tolerance, args.contract, reference, args.color)
        report.update(raw_sha256=hashlib.sha256(args.raw.read_bytes()).hexdigest(),
                      representation='straight', scope='this raw fixture only; not AE/GPU certification')
        print(json.dumps(report, indent=2))
        return 0
    except (ValueError, OSError) as error:
        print(json.dumps({'passed': False, 'error': str(error)}))
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
