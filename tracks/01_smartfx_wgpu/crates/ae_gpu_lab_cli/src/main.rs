use ae_gpu_lab_core::wgpu_backend::WgpuUploadReadback;
use ae_gpu_lab_core::{
    BackendTiming, BenchEffect, CpuBackend, FrameRgba8, LabParams, RenderBackend, RenderReport,
};
use anyhow::Result;
use clap::{Parser, ValueEnum};

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, value_enum, default_value_t = EffectArg::BoxBlur)]
    effect: EffectArg,

    #[arg(long, default_value_t = 1920)]
    width: u32,

    #[arg(long, default_value_t = 1080)]
    height: u32,

    #[arg(long, default_value_t = 3)]
    passes: u32,

    #[arg(long, default_value_t = 2.0)]
    strength: f32,

    #[arg(long, default_value_t = 5)]
    radius: u32,

    #[arg(long)]
    cpu: bool,

    #[arg(long, default_value_t = 1)]
    iterations: u32,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum EffectArg {
    Copy,
    Color,
    BoxBlur,
    Diffusion,
    ChromaWarp,
}

impl From<EffectArg> for BenchEffect {
    fn from(value: EffectArg) -> Self {
        match value {
            EffectArg::Copy => Self::Copy,
            EffectArg::Color => Self::Color,
            EffectArg::BoxBlur => Self::BoxBlur,
            EffectArg::Diffusion => Self::Diffusion,
            EffectArg::ChromaWarp => Self::ChromaWarp,
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let effect = BenchEffect::from(args.effect);
    let input = FrameRgba8::synthetic(args.width, args.height);
    let params = LabParams::new(
        args.width,
        args.height,
        args.passes,
        args.strength,
        args.radius,
    );

    println!(
        "effect={:?} size={}x{} pixels={} cpu={}",
        effect,
        args.width,
        args.height,
        args.width as u64 * args.height as u64,
        args.cpu
    );
    println!("iterations={}", args.iterations.max(1));

    if args.cpu {
        let cpu = CpuBackend;
        print_report(
            cpu.name(),
            run_iterations(&cpu, effect, &input, params, args.iterations)?,
        );
    }

    let gpu = WgpuUploadReadback::new()?;
    print_report(
        gpu.name(),
        run_iterations(&gpu, effect, &input, params, args.iterations)?,
    );

    Ok(())
}

fn run_iterations<B: RenderBackend>(
    backend: &B,
    effect: BenchEffect,
    input: &FrameRgba8,
    params: LabParams,
    iterations: u32,
) -> Result<RenderReport> {
    let count = iterations.max(1);
    let mut report = backend.render(effect, input, params)?;
    for _ in 1..count {
        let next = backend.render(effect, input, params)?;
        report.timing.upload += next.timing.upload;
        report.timing.encode += next.timing.encode;
        report.timing.submit_wait += next.timing.submit_wait;
        report.timing.readback_map += next.timing.readback_map;
        report.timing.readback_copy += next.timing.readback_copy;
        report.timing.total += next.timing.total;
        report.frame = next.frame;
    }
    report.timing.upload /= count;
    report.timing.encode /= count;
    report.timing.submit_wait /= count;
    report.timing.readback_map /= count;
    report.timing.readback_copy /= count;
    report.timing.total /= count;
    Ok(report)
}

fn print_report(name: &str, report: RenderReport) {
    println!(
        "{} total_ms={:.3} checksum={}",
        name,
        ms(report.timing.total),
        checksum(&report.frame)
    );
    print_timing(report.timing);
}

fn print_timing(timing: BackendTiming) {
    if timing.upload.is_zero()
        && timing.encode.is_zero()
        && timing.submit_wait.is_zero()
        && timing.readback_map.is_zero()
        && timing.readback_copy.is_zero()
    {
        return;
    }
    println!(
        "  upload_ms={:.3} encode_ms={:.3} submit_ms={:.3} map_wait_ms={:.3} copy_ms={:.3}",
        ms(timing.upload),
        ms(timing.encode),
        ms(timing.submit_wait),
        ms(timing.readback_map),
        ms(timing.readback_copy)
    );
}

fn ms(duration: std::time::Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn checksum(frame: &FrameRgba8) -> u64 {
    frame
        .pixels
        .iter()
        .fold(0u64, |acc, &v| acc.wrapping_mul(16777619) ^ v as u64)
}
