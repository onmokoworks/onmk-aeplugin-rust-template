use ae_gpu_lab_core::wgpu_backend::WgpuUploadReadback;
use ae_gpu_lab_core::{
    render_cpu, BackendTiming, BenchEffect, FrameDesc, FrameRgba8, FrameView, FrameViewMut,
    LabParams, PixelFormat,
};
use ae_plugin_kit::ae::{world_layout, write_layer};
use ae_plugin_kit::{with_checkouts, Depth};
use after_effects as ae;
use std::panic::catch_unwind;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

static GPU: OnceLock<Option<WgpuUploadReadback>> = OnceLock::new();
static LAST_STATS: OnceLock<Mutex<RenderStats>> = OnceLock::new();

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Effect,
    UseGpu,
    Strength,
    Radius,
    Passes,
}

#[derive(Default)]
struct Plugin;

ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: ae::InData,
        _out_data: ae::OutData,
    ) -> Result<(), ae::Error> {
        params.add(
            Params::Effect,
            "Effect",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Copy", "Color", "Box Blur", "Diffusion", "Chroma Warp"]);
                f.set_default(1);
            }),
        )?;

        params.add(
            Params::UseGpu,
            "Use GPU",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
                f.set_label("wgpu upload/readback");
            }),
        )?;

        params.add(
            Params::Strength,
            "Strength",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(10.0);
                f.set_slider_min(0.0);
                f.set_slider_max(5.0);
                f.set_default(1.0);
                f.set_precision(2);
            }),
        )?;

        params.add(
            Params::Radius,
            "Radius",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(32);
                f.set_slider_min(1);
                f.set_slider_max(16);
                f.set_default(3);
            }),
        )?;

        params.add(
            Params::Passes,
            "Passes",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(64);
                f.set_slider_min(1);
                f.set_slider_max(16);
                f.set_default(3);
            }),
        )?;

        Ok(())
    }

    fn handle_command(
        &mut self,
        cmd: ae::Command,
        in_data: ae::InData,
        mut out_data: ae::OutData,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        match cmd {
            ae::Command::About => {
                out_data.set_return_msg(&about_message());
            }
            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => {
                render_layer(params, &in_layer, &mut out_layer)?;
            }
            ae::Command::SmartPreRender { mut extra } => {
                smart_pre_render(&in_data, &mut extra)?;
            }
            ae::Command::SmartRender { extra } => {
                smart_render(&extra, params)?;
            }
            // This track is a CPU SmartFX effect that runs its own wgpu work.
            // AE official GPU worlds belong to track 02 and have no CPU buffer.
            ae::Command::SmartRenderGpu { .. } => return Err(ae::Error::Generic),
            _ => {}
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
struct PluginParams {
    effect: BenchEffect,
    use_gpu: bool,
    strength: f32,
    radius: u32,
    passes: u32,
}

fn get_params(params: &ae::Parameters<Params>) -> Result<PluginParams, ae::Error> {
    let effect = match params.get(Params::Effect)?.as_popup()?.value() {
        2 => BenchEffect::Color,
        3 => BenchEffect::BoxBlur,
        4 => BenchEffect::Diffusion,
        5 => BenchEffect::ChromaWarp,
        _ => BenchEffect::Copy,
    };
    Ok(PluginParams {
        effect,
        use_gpu: params.get(Params::UseGpu)?.as_checkbox()?.value(),
        strength: params.get(Params::Strength)?.as_float_slider()?.value() as f32,
        radius: params.get(Params::Radius)?.as_slider()?.value().max(1) as u32,
        passes: params.get(Params::Passes)?.as_slider()?.value().max(1) as u32,
    })
}

fn dispatch(pp: PluginParams, frame: &FrameRgba8) -> FrameRgba8 {
    let total_start = Instant::now();
    let lab_params = LabParams::new(frame.width, frame.height, pp.passes, pp.strength, pp.radius);
    if pp.use_gpu {
        if let Some(gpu) = gpu() {
            if let Ok(report) = gpu.render_frame(pp.effect, frame, lab_params) {
                record_stats(RenderStats {
                    backend: "wgpu",
                    effect: pp.effect,
                    width: frame.width,
                    height: frame.height,
                    timing: report.timing,
                    total_with_copies: total_start.elapsed(),
                    fallback: false,
                });
                return report.frame;
            }
        }
    }
    let cpu_start = Instant::now();
    let frame_out = render_cpu(pp.effect, frame, lab_params);
    record_stats(RenderStats {
        backend: "cpu",
        effect: pp.effect,
        width: frame.width,
        height: frame.height,
        timing: BackendTiming::cpu(cpu_start.elapsed()),
        total_with_copies: total_start.elapsed(),
        fallback: pp.use_gpu,
    });
    frame_out
}

fn gpu() -> Option<&'static WgpuUploadReadback> {
    GPU.get_or_init(|| {
        catch_unwind(WgpuUploadReadback::new)
            .ok()
            .and_then(Result::ok)
    })
    .as_ref()
}

fn render_layer(
    params: &ae::Parameters<Params>,
    in_layer: &ae::Layer,
    out_layer: &mut ae::Layer,
) -> Result<(), ae::Error> {
    let pp = get_params(params)?;
    if world_layout(out_layer)?.is_empty() {
        return Ok(());
    }
    if world_layout(in_layer)?.is_empty() {
        // Nothing visible in the input: return a transparent frame.
        let pixels = vec![[0.0; 4]; out_layer.width() * out_layer.height()];
        return write_layer(out_layer, &pixels).map(|_| ());
    }
    let frame = layer_to_rgba(in_layer)?;
    let out = dispatch(pp, &frame);
    rgba_to_layer(&out, out_layer)?;
    Ok(())
}

fn smart_pre_render(
    in_data: &ae::InData,
    extra: &mut ae::pf::PreRenderExtra,
) -> Result<(), ae::Error> {
    let req = extra.output_request();
    let cb = extra.callbacks();
    let input = cb.checkout_layer(
        0,
        0,
        &req,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    )?;

    let req_rect: ae::Rect = req.rect.into();
    let mut res: ae::Rect = input.result_rect.into();
    let mut max_res: ae::Rect = input.max_result_rect.into();
    res.left = res.left.max(req_rect.left);
    res.top = res.top.max(req_rect.top);
    res.right = res.right.min(req_rect.right);
    res.bottom = res.bottom.min(req_rect.bottom);
    max_res.left = max_res.left.max(req_rect.left);
    max_res.top = max_res.top.max(req_rect.top);
    max_res.right = max_res.right.min(req_rect.right);
    max_res.bottom = max_res.bottom.min(req_rect.bottom);
    extra.set_result_rect(res);
    extra.set_max_result_rect(max_res);
    Ok(())
}

fn smart_render(
    extra: &ae::pf::SmartRenderExtra,
    params: &ae::Parameters<Params>,
) -> Result<(), ae::Error> {
    let cb = extra.callbacks();
    // The input is checked in again on success, error and panic.
    with_checkouts(&cb, ae::Error::Generic, |t| {
        let input = t.checkout_required(0, ae::Error::Generic)?;
        let mut output = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
        render_layer(params, &input, &mut output)
    })
}

fn layer_to_rgba(layer: &ae::Layer) -> Result<FrameRgba8, ae::Error> {
    let desc = layer_desc(layer)?;
    FrameView::new(desc, layer.buffer())
        .map(FrameView::to_rgba8)
        .map_err(|_| ae::Error::Generic)
}

fn rgba_to_layer(frame: &FrameRgba8, layer: &mut ae::Layer) -> Result<(), ae::Error> {
    let desc = layer_desc(layer)?;
    FrameViewMut::new(desc, layer.buffer_mut())
        .and_then(|mut view| view.write_rgba8(frame))
        .map_err(|_| ae::Error::Generic)
}

/// Validate the world and describe it for `FrameView`.
///
/// - Depth comes from the World Suite pixel format. `Layer::bit_depth()`
///   reports 16 for every DEEP world, including ARGB128 float worlds.
/// - Stride, alignment, NULL and overflow are checked before
///   `Layer::buffer()` forms a slice.
/// - `FrameView` addresses rows from the start of the slice, so negative
///   strides (bottom-up worlds) are rejected here instead of being read
///   upside down. `ae_plugin_kit::ae::read_layer` supports them if needed.
fn layer_desc(layer: &ae::Layer) -> Result<FrameDesc, ae::Error> {
    let layout = world_layout(layer)?;
    if layout.is_empty() || layout.row_bytes < 0 {
        return Err(ae::Error::BadCallbackParameter);
    }
    let format = match layout.depth {
        Depth::U8 => PixelFormat::AeArgb8,
        Depth::U16 => PixelFormat::AeArgb16,
        Depth::F32 => PixelFormat::AeArgb32Float,
    };
    Ok(FrameDesc {
        width: u32::try_from(layout.width).map_err(|_| ae::Error::BadCallbackParameter)?,
        height: u32::try_from(layout.height).map_err(|_| ae::Error::BadCallbackParameter)?,
        row_bytes: layout.row_bytes as usize,
        format,
    })
}

#[derive(Clone, Copy, Debug)]
struct RenderStats {
    backend: &'static str,
    effect: BenchEffect,
    width: u32,
    height: u32,
    timing: BackendTiming,
    total_with_copies: Duration,
    fallback: bool,
}

impl Default for RenderStats {
    fn default() -> Self {
        Self {
            backend: "none",
            effect: BenchEffect::Copy,
            width: 0,
            height: 0,
            timing: BackendTiming::default(),
            total_with_copies: Duration::ZERO,
            fallback: false,
        }
    }
}

fn record_stats(stats: RenderStats) {
    if let Ok(mut lock) = stats_cell().lock() {
        *lock = stats;
    }
}

fn stats_cell() -> &'static Mutex<RenderStats> {
    LAST_STATS.get_or_init(|| Mutex::new(RenderStats::default()))
}

fn about_message() -> String {
    let stats = stats_cell().lock().map(|s| *s).unwrap_or_default();
    format!(
        "AeGpuLab v0.2\rRust SmartFX + wgpu upload/readback\rLast: {} {:?} {}x{}\rTotal incl AE copies: {:.2} ms\rCore total: {:.2} ms\rUpload {:.2} / Encode {:.2} / Map {:.2} / Copy {:.2} ms\rFallback: {}",
        stats.backend,
        stats.effect,
        stats.width,
        stats.height,
        ms(stats.total_with_copies),
        ms(stats.timing.total),
        ms(stats.timing.upload),
        ms(stats.timing.encode),
        ms(stats.timing.readback_map),
        ms(stats.timing.readback_copy),
        if stats.fallback { "yes" } else { "no" },
    )
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}
