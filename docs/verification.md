# Verification

[日本語](./verification.ja.md)

Evidence comes in levels. Say which level a claim rests on, and never present a
lower level as a higher one.

| Level | What it shows | How |
|---|---|---|
| 1. Unit tests | Render logic, codecs, world validation, cleanup and alpha oracles behave as specified | `cargo test --release` |
| 2. CI | The portable crates build and pass tests on Windows x64, macOS arm64 and macOS x86_64 | `.github/workflows/ci.yml` |
| 3. Headless host harness (optional) | The built `.aex` loads, its parameters enumerate, and Render / SmartRender at 8/16/32 bpc produce the expected raw pixels | AEXCompat, below |
| 4. After Effects | Real host behaviour: UI, caching, MFR, GPU, saved projects, other host versions | Manual, in AE |

Levels 1-3 do **not** replace level 4. Source inventory and arithmetic-only
tests are not AE validation.

## Local checks

```powershell
cargo fmt --all --check
cargo build --release
cargo test --release
cargo clippy --release -p ae_plugin_kit --all-features -p ae_plugin_build -p ae_minimal_plugin --all-targets -- -D warnings
python -m unittest discover -s scripts -p "test_*.py"
```

The Python tests cover the alpha checker, the smoke-test oracles and the
installer (against a fake root under the temp directory; Windows only).

GPU tests that need an adapter are `#[ignore]`d. Run them explicitly:
`cargo test --release -p ae_gpu_lab_core -- --ignored`.

## Headless smoke test with AEXCompat (optional)

[AEXCompat](https://github.com/onmokoworks/AEXCompat) is a separate project
with a headless harness that loads an `.aex` outside After Effects and renders
a JSON fixture through classic Render or SmartRender at a chosen pixel format.
It is **optional**: nothing in this template needs it to build or test, and CI
does not run it.

`scripts/aexcompat_smoke.py` is a small, dependency-free example for AeMinimal:

```powershell
.\scripts\install-ae-plugin.ps1 -Plugin minimal          # builds dist\AeMinimal.aex, no install
python scripts/aexcompat_smoke.py `
    --harness <path>\aexcompat-harness.exe `
    --plugin dist\AeMinimal.aex `
    --output local\smoke-minimal                          # must not exist yet
```

For each depth (`argb8`, `argb16`, `argb32f`) and route (`classic`, `smart`):

- `invert`: partial-alpha stripes with Mode = Invert. Alpha must be unchanged
  and straight RGB inverted, compared with the harness's own input checkpoint.
- `blur-edge`: opaque white next to transparent pixels with hidden red RGB,
  Mode = Box Blur. Every visible pixel must stay white and partial alpha must
  appear.

The expected values come from the specification, not from the plug-in, and
`scripts/test_aexcompat_smoke.py` proves the oracles reject premultiplied
output, dark or red fringes and opaque-only fixtures. `summary.json` records the
SHA-256 of the plug-in, the harness and the runner, every case and its error,
and the limitations (no AE, 8-bit PNG input, no MFR/GPU/layer-parameter or
resize coverage).

To adapt it to your effect: change the parameter values per case and write an
oracle derived from what the effect is supposed to do. Keep a negative control:
running the smoke against a different plug-in (for example the pass-through
`AeGpuProbe.aex`) must fail.

Harness results are only evidence for the harness and the fixtures you ran.
Report routes you did not run as not covered.

## Before claiming a release works in AE

- Record the artifact SHA-256 and the source commit.
- Test 8, 16 and 32 bpc projects, Render and SmartRender hosts, partial alpha
  (see [alpha contract](alpha-contract.md)), and a project saved with the
  previous version (see [parameter compatibility](parameter-compatibility.md)).
- Install with `scripts/install-ae-plugin.ps1 -Install` (backed up, verified);
  undo with `scripts/restore-ae-plugin.ps1`.
