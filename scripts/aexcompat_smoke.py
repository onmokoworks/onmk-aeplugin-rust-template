"""Optional headless smoke test of AeMinimal.aex with the AEXCompat harness.

AEXCompat (https://github.com/onmokoworks/AEXCompat) is a separate, optional
host harness that loads an .aex outside After Effects. This script never
starts, installs into or touches After Effects.

What it checks, per depth (8/16/32 bpc) and route (classic Render / SmartRender):
  invert     partial-alpha stripes, Mode=Invert: alpha unchanged and straight
             RGB inverted, compared with the harness's own input checkpoint.
  blur-edge  opaque white next to transparent pixels with hidden red RGB,
             Mode=Box Blur: every visible output pixel must stay white (no
             dark or red fringe) and partial alpha must appear.

The expected values come from the specification, not from the plug-in. The
oracles are unit tested (test_aexcompat_smoke.py), including that a
premultiplied output fails. A pass is evidence for this harness and these
fixtures only, not an After Effects certification.

No third-party Python packages are needed.

    python scripts/aexcompat_smoke.py --harness <aexcompat-harness.exe> \\
        --plugin dist/AeMinimal.aex --output local/smoke-minimal
"""
import argparse
import hashlib
import json
import math
import struct
import subprocess
import sys
import zlib
from pathlib import Path

DEPTHS = {'argb8': ('B', 255.0, 1.0), 'argb16': ('H', 32768.0, 1.0), 'argb32f': ('f', 1.0, 1e-6)}
ROUTES = ['classic', 'smart']
MODE_INVERT, MODE_BLUR = 1, 2


def sha256(path):
    h = hashlib.sha256()
    with open(path, 'rb') as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b''):
            h.update(chunk)
    return h.hexdigest()


def write_png(path, width, height, rgba):
    """Minimal RGBA8 PNG writer (straight alpha, as PNG defines)."""
    raw = b''.join(b'\0' + bytes(rgba[y * width * 4:(y + 1) * width * 4]) for y in range(height))

    def chunk(kind, data):
        body = kind + data
        return struct.pack('>I', len(data)) + body + struct.pack('>I', zlib.crc32(body) & 0xffffffff)

    Path(path).write_bytes(b'\x89PNG\r\n\x1a\n'
                           + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0))
                           + chunk(b'IDAT', zlib.compress(raw)) + chunk(b'IEND', b''))


def stripes(width, height):
    """Straight RGB (64, 192, 0) with alpha 0, 64, 128, 255 by column."""
    out = []
    for _y in range(height):
        for x in range(width):
            out += [64, 192, 0, [0, 64, 128, 255][x % 4]]
    return out


def edge(width, height):
    """Opaque white on the left half, transparent hidden red on the right."""
    out = []
    for _y in range(height):
        for x in range(width):
            out += [255, 255, 255, 255] if x < width // 2 else [255, 0, 0, 0]
    return out


def read_argb(path, depth, width, height):
    code, scale, _ = DEPTHS[depth]
    raw = Path(path).read_bytes()
    values = struct.unpack('<%d%s' % (width * height * 4, code), raw)
    return [tuple(values[i:i + 4]) for i in range(0, len(values), 4)], scale


def check_invert(inp, out, scale, tolerance):
    """Alpha preserved; visible RGB == scale - input RGB (straight)."""
    if len(inp) != len(out) or not inp:
        raise AssertionError('input/output size mismatch')
    if not any(0 < p[0] < scale for p in inp):
        raise AssertionError('fixture lacks partial alpha')
    worst = 0.0
    for i, (p, q) in enumerate(zip(inp, out)):
        if not all(math.isfinite(c) for c in q):
            raise AssertionError(f'non-finite output at {i}')
        if abs(p[0] - q[0]) > tolerance:
            raise AssertionError(f'alpha changed at {i}: {p[0]} -> {q[0]}')
        if p[0] > 0:
            for c in (1, 2, 3):
                worst = max(worst, abs((scale - p[c]) - q[c]))
    if worst > tolerance:
        raise AssertionError(f'straight invert violated: max error {worst} > {tolerance}')
    return worst


def check_white_edge(out, scale, tolerance):
    """Every visible pixel stays white; some alpha is partial."""
    if not any(0 < p[0] < scale for p in out):
        raise AssertionError('blur produced no partial alpha (fixture not discriminating)')
    worst = 0.0
    for p in out:
        if not all(math.isfinite(c) for c in p):
            raise AssertionError('non-finite output')
        if p[0] > 0:
            worst = max(worst, max(abs(scale - p[c]) for c in (1, 2, 3)))
    if worst > tolerance:
        raise AssertionError(f'visible edge is not white: max error {worst} > {tolerance}')
    return worst


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--harness', type=Path, required=True)
    parser.add_argument('--plugin', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True, help='new directory for fixtures and results')
    parser.add_argument('--depths', nargs='+', choices=list(DEPTHS), default=list(DEPTHS))
    parser.add_argument('--routes', nargs='+', choices=ROUTES, default=ROUTES)
    args = parser.parse_args()
    harness, plugin = args.harness.resolve(), args.plugin.resolve()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    hashes = dict(plugin_sha256=sha256(plugin), harness_sha256=sha256(harness),
                  runner_sha256=sha256(__file__))

    def invoke(*argv):
        if sha256(plugin) != hashes['plugin_sha256']:
            raise RuntimeError('plug-in changed during the run')
        p = subprocess.run([str(harness), '--headless', *map(str, argv)], capture_output=True,
                           text=True, encoding='utf-8', timeout=120)
        if p.returncode:
            raise RuntimeError((p.stdout + '\n' + p.stderr).strip()[-2000:])
        return json.loads(p.stdout)

    params = invoke('--inspect-experimental', plugin)
    (root / 'parameters.json').write_text(json.dumps(params, indent=2), encoding='utf-8')
    width, height = 16, 8
    sources = {'invert': stripes(width, height), 'blur-edge': edge(width, height)}
    for name, pixels in sources.items():
        write_png(root / f'{name}.png', width, height, pixels)

    results = []
    for case, mode in (('invert', MODE_INVERT), ('blur-edge', MODE_BLUR)):
        for depth in args.depths:
            for route in args.routes:
                label = f'{case}-{depth}-{route}'
                record = dict(case=label, depth=depth, route=route)
                try:
                    configured = json.loads(json.dumps(params))
                    values = {'Mode': mode, 'Amount': 100, 'Blur Radius': 1}
                    for p in configured:
                        if p['name'] in values:
                            p['value'] = values[p['name']]
                    fixture = root / f'{label}.json'
                    fixture.write_text(json.dumps(dict(
                        schema='aexcompat.render_fixture', schema_version=1,
                        primary_layer=f'{case}.png', parameters=configured, pixel_format=depth,
                        render_path=route, premultiplication='straight',
                        timing=dict(current_time=0, time_step=1, total_time=10, time_scale=1),
                        final_artifact='raw',
                        checkpoints=[dict(id='input', stage=f'{route}-input'),
                                     dict(id='output', stage=f'{route}-output')]), indent=2),
                        encoding='utf-8')
                    dest = root / 'renders' / label
                    dest.parent.mkdir(exist_ok=True)
                    report = invoke('--render-fixture', plugin, fixture, dest)
                    artifact = report['final_artifact']
                    if artifact['pixel_format'] != depth or (artifact['width'], artifact['height']) != (width, height):
                        raise AssertionError(f'unexpected output artifact {artifact}')
                    out, scale = read_argb(dest / 'final' / 'output.bin', depth, width, height)
                    tolerance = DEPTHS[depth][2]
                    if case == 'invert':
                        inp, _ = read_argb(dest / 'checkpoints' / 'input' / 'output.bin', depth, width, height)
                        record['max_error'] = check_invert(inp, out, scale, tolerance)
                    else:
                        record['max_error'] = check_white_edge(out, scale, tolerance)
                    record['status'] = 'passed'
                except Exception as error:  # record and continue with other cases
                    record.update(status='failed', error=str(error))
                results.append(record)
                print(f"{record['status']:7} {label} {record.get('error', '')}", flush=True)

    summary = dict(**hashes, plugin=str(plugin), harness=str(harness), results=results,
                   limitations=['AEXCompat harness only; not an After Effects run',
                                '8-bit PNG input: does not exercise full 16-bit input precision',
                                'No MFR, GPU, layer-parameter or resize coverage'])
    (root / 'summary.json').write_text(json.dumps(summary, indent=2), encoding='utf-8')
    failed = [r for r in results if r['status'] != 'passed']
    print(f'{len(results) - len(failed)}/{len(results)} passed; summary: {root / "summary.json"}')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
