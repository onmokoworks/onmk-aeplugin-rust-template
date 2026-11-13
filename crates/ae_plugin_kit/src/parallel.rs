//! Bounded, deterministic row parallelism.
//!
//! - No global thread pool and no render-global mutable state: every call
//!   creates scoped threads that end before it returns. This is safe under AE
//!   Multi-Frame Rendering, where several frames render concurrently and each
//!   render thread may call into the plug-in.
//! - The worker count is capped, so N concurrent MFR renders spawn at most
//!   `N * max_workers` short-lived threads.
//! - Output rows are partitioned into contiguous chunks. As long as each output
//!   row is a pure function of the (shared, read-only) input, the result is
//!   bit-identical for every worker count, including 1.

/// Default cap; AE already parallelises across frames with MFR.
pub const DEFAULT_MAX_WORKERS: usize = 8;

/// Default amount of work below which threading costs more than it saves.
pub const DEFAULT_MIN_ELEMENTS: usize = 262_144;

/// Pick a worker count for `elements` units of work.
pub fn workers_for(elements: usize, min_elements: usize, max_workers: usize) -> usize {
    if elements < min_elements.max(1) {
        return 1;
    }
    std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(max_workers.max(1))
}

/// Process `out` (a row-major buffer of `row_len` elements per row) in up to
/// `workers` contiguous row chunks. `run(first_row, chunk)` receives the index
/// of the first row in the chunk and a whole number of rows.
pub fn for_each_row_chunk<T: Send>(
    out: &mut [T],
    row_len: usize,
    workers: usize,
    run: impl Fn(usize, &mut [T]) + Sync,
) {
    if out.is_empty() || row_len == 0 {
        return;
    }
    assert_eq!(
        out.len() % row_len,
        0,
        "buffer is not a whole number of rows"
    );
    let rows = out.len() / row_len;
    let workers = workers.clamp(1, rows);
    if workers == 1 {
        run(0, out);
        return;
    }
    let rows_per_chunk = rows.div_ceil(workers);
    std::thread::scope(|scope| {
        let run = &run;
        for (i, chunk) in out.chunks_mut(rows_per_chunk * row_len).enumerate() {
            scope.spawn(move || run(i * rows_per_chunk, chunk));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(width: usize, height: usize, workers: usize) -> Vec<f32> {
        let input: Vec<f32> = (0..width * height).map(|i| (i as f32).sin()).collect();
        let mut out = vec![0.0; width * height];
        for_each_row_chunk(&mut out, width, workers, |first_row, chunk| {
            for (r, row) in chunk.chunks_exact_mut(width).enumerate() {
                let y = first_row + r;
                for (x, value) in row.iter_mut().enumerate() {
                    let above = input[y.saturating_sub(1) * width + x];
                    let below = input[(y + 1).min(height - 1) * width + x];
                    *value = input[y * width + x] * 0.5 + (above + below) * 0.25;
                }
            }
        });
        out
    }

    #[test]
    fn every_worker_count_is_bit_identical_to_serial() {
        for (w, h) in [(1, 1), (3, 7), (17, 5), (64, 33)] {
            let serial = render(w, h, 1);
            for workers in 2..=12 {
                let parallel = render(w, h, workers);
                assert!(
                    serial
                        .iter()
                        .zip(&parallel)
                        .all(|(a, b)| a.to_bits() == b.to_bits()),
                    "w={w} h={h} workers={workers}"
                );
            }
        }
    }

    #[test]
    fn each_row_is_visited_exactly_once() {
        let mut out = vec![0u32; 13 * 4];
        for_each_row_chunk(&mut out, 4, 5, |first_row, chunk| {
            for (r, row) in chunk.chunks_exact_mut(4).enumerate() {
                row.iter_mut()
                    .for_each(|v| *v += (first_row + r) as u32 + 1);
            }
        });
        for (y, row) in out.chunks_exact(4).enumerate() {
            assert!(row.iter().all(|v| *v == y as u32 + 1));
        }
    }

    #[test]
    fn empty_inputs_and_small_work_are_serial() {
        let mut empty: Vec<u8> = Vec::new();
        for_each_row_chunk(&mut empty, 4, 8, |_, _| panic!("must not run"));
        assert_eq!(workers_for(10, DEFAULT_MIN_ELEMENTS, 8), 1);
        assert!(workers_for(usize::MAX, 1, 3) <= 3);
    }
}
