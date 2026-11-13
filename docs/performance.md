# CPU Performance Scaffolding

[日本語](./performance.ja.md)

`crates/ae_plugin_kit` contains two small, tested patterns. AeMinimal's Box
Blur uses both.

## Bounded row parallelism (`ae_plugin_kit::parallel`)

```rust
use ae_plugin_kit::parallel::{for_each_row_chunk, workers_for, DEFAULT_MAX_WORKERS, DEFAULT_MIN_ELEMENTS};

let workers = workers_for(width * height * taps, DEFAULT_MIN_ELEMENTS, DEFAULT_MAX_WORKERS);
for_each_row_chunk(&mut out, width, workers, |first_row, rows| {
    for (r, row) in rows.chunks_exact_mut(width).enumerate() {
        let y = first_row + r;
        // write row y from read-only input
    }
});
```

- **No global pool, no render-global mutable state.** Threads are scoped and
  end before the call returns, so the helper is safe when AE Multi-Frame
  Rendering calls the effect from several threads at once.
- **Bounded.** At most `max_workers` threads per call (default 8); small jobs
  stay serial. AE already parallelises across frames, so do not size this to
  every core.
- **Deterministic.** Rows are split into contiguous chunks. When every output
  row depends only on shared read-only input, the result is bit-identical for
  every worker count; the tests check 1..12 workers against serial.
- Dependent passes (a vertical pass that reads the horizontal result) run as
  separate calls.

AeMinimal does not set `SupportsThreadedRendering` (MFR). The helper is ready
for it, but enabling MFR also requires thread-safe global/sequence data and
`SupportsGetFlattenedSequenceData`, and needs validation in AE.

## SIMD with runtime detection (`ae_plugin_kit::simd`)

`convolve_row` is a 1-D correlation used as an example:

1. `convolve_scalar` defines the exact arithmetic order.
2. `convolve_avx` (`#[target_feature(enable = "avx")]`) performs the same IEEE
   operations per lane in the same order: multiply, then add. It does **not**
   use FMA, which rounds once instead of twice and would change results.
3. The dispatcher checks `is_x86_feature_detected!("avx")` at run time, so one
   binary runs on every x86_64 CPU, and accepts `allow_simd = false` so tests
   can force the fallback without globals or environment variables.

`runtime_dispatch_matches_forced_scalar_bit_for_bit` compares both paths bit for
bit on random data and on special values (signed zero, subnormals, ±MAX,
±infinity; NaN must stay NaN). Rust does not contract `a + b * c` into FMA by
itself, so the scalar path is stable across machines. On aarch64 the scalar
path is used and auto-vectorised with NEON.

When writing a new kernel: keep the scalar reference, match its operation order
exactly in the SIMD version, handle the tail with the scalar code, and add the
forced-scalar equivalence test before optimising further.

## Measuring

- `cargo run -p ae_gpu_lab_cli --release -- --cpu --effect box-blur ...` times
  the lab's CPU path outside AE.
- Time inside AE with release builds only; debug builds of the
  `after-effects` crate install a panic hook and logger.
