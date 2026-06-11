use ae_gpu_lab_native_probe::{AeGpuDeviceProbe, AeGpuFramework};
use after_effects as ae;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

mod cuda_driver;

static LAST_PROBE: OnceLock<Mutex<ProbeState>> = OnceLock::new();

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    EnableGpuProbe,
    RequestGpuSmartRender,
    CudaCopyOutput,
    CudaInvertOutput,
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
            Params::EnableGpuProbe,
            "Enable GPU Probe",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
                f.set_label("Log GPU selectors only");
            }),
        )?;
        params.add(
            Params::RequestGpuSmartRender,
            "Request GPU SmartRender",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_label("Hot probe");
            }),
        )?;
        params.add(
            Params::CudaCopyOutput,
            "CUDA Copy Output",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_label("input -> output");
            }),
        )?;
        params.add(
            Params::CudaInvertOutput,
            "CUDA Invert Output",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_label("invert rgb");
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
            ae::Command::GpuDeviceSetup { extra } => {
                let probe = AeGpuDeviceProbe {
                    framework: map_framework(extra.what_gpu()),
                    device_index: extra.device_index() as i32,
                    can_render_f32: true,
                };
                record_probe(probe, "GpuDeviceSetup");
                out_data.set_out_flag2(ae::OutFlags2::SupportsGpuRenderF32, true);
                if matches!(
                    probe.framework,
                    AeGpuFramework::DirectX11 | AeGpuFramework::DirectX12
                ) {
                    out_data.set_out_flag2(ae::OutFlags2::SupportsDirectXRendering, true);
                }
            }
            ae::Command::GpuDeviceSetdown { extra } => {
                append_log(&format!(
                    "GpuDeviceSetdown framework={:?} device={}",
                    extra.what_gpu(),
                    extra.device_index()
                ));
            }
            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => {
                copy_layer(&in_layer, &mut out_layer);
            }
            ae::Command::SmartPreRender { mut extra } => {
                let enabled = params.get(Params::EnableGpuProbe)?.as_checkbox()?.value();
                let request_gpu = params
                    .get(Params::RequestGpuSmartRender)?
                    .as_checkbox()?
                    .value();
                let cuda_copy = params.get(Params::CudaCopyOutput)?.as_checkbox()?.value();
                let cuda_invert = params.get(Params::CudaInvertOutput)?.as_checkbox()?.value();
                let gpu_possible =
                    enabled && request_gpu && extra.what_gpu() == ae::GpuFramework::Cuda;
                append_log(&format!(
                    "SmartPreRender what_gpu={:?} device={} bit_depth={} enabled={} request_gpu={} cuda_copy={} cuda_invert={} gpu_possible={}",
                    extra.what_gpu(),
                    extra.device_index(),
                    extra.bit_depth(),
                    enabled,
                    request_gpu,
                    cuda_copy,
                    cuda_invert,
                    gpu_possible
                ));
                smart_pre_render(&in_data, &mut extra)?;
                extra.set_gpu_render_possible(gpu_possible);
            }
            ae::Command::SmartRender { extra } => {
                smart_render(&extra)?;
            }
            ae::Command::SmartRenderGpu { extra } => {
                let cuda_copy = params.get(Params::CudaCopyOutput)?.as_checkbox()?.value();
                let cuda_invert = params.get(Params::CudaInvertOutput)?.as_checkbox()?.value();
                let mode = if cuda_invert {
                    CudaRenderMode::Invert
                } else {
                    CudaRenderMode::Copy
                };
                append_log(&format!(
                    "SmartRenderGpu what_gpu={:?} device={} bit_depth={} cuda_copy={} cuda_invert={} mode={:?} note=copy_is_default",
                    extra.what_gpu(),
                    extra.device_index(),
                    extra.bit_depth(),
                    cuda_copy,
                    cuda_invert,
                    mode
                ));
                if extra.what_gpu() == ae::GpuFramework::Cuda {
                    cuda_render_gpu_worlds(&in_data, &extra, mode)?;
                } else {
                    log_gpu_worlds(&in_data, &extra);
                    append_log("SmartRenderGpu refused: only CUDA copy currently writes output");
                    return Err(ae::Error::Generic);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
enum CudaRenderMode {
    Copy,
    Invert,
}

impl CudaRenderMode {
    fn kernel_mode(self) -> u32 {
        match self {
            Self::Copy => 0,
            Self::Invert => 1,
        }
    }
}

fn cuda_render_gpu_worlds(
    in_data: &ae::InData,
    extra: &ae::pf::SmartRenderExtra,
    mode: CudaRenderMode,
) -> Result<(), ae::Error> {
    let Ok(gpu_suite) = ae::pf::suites::GPUDevice::new() else {
        append_log("CUDA render failed: GPUDeviceSuite unavailable");
        return Err(ae::Error::Generic);
    };
    let cb = extra.callbacks();
    let mut input = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
    let mut output = cb.checkout_output()?.ok_or(ae::Error::Generic)?;

    append_gpu_world("input", in_data, &gpu_suite, &mut input);
    append_gpu_world("output", in_data, &gpu_suite, &mut output);

    let input_ptr = gpu_suite
        .gpu_world_data(in_data.effect_ref(), &mut input)
        .map_err(|err| {
            append_log(&format!("CUDA render failed: input gpu_world_data {err:?}"));
            ae::Error::Generic
        })?;
    let output_ptr = gpu_suite
        .gpu_world_data(in_data.effect_ref(), &mut output)
        .map_err(|err| {
            append_log(&format!(
                "CUDA render failed: output gpu_world_data {err:?}"
            ));
            ae::Error::Generic
        })?;
    let info = gpu_suite
        .device_info(in_data.effect_ref(), extra.device_index())
        .map_err(|err| {
            append_log(&format!("CUDA render failed: device_info {err:?}"));
            ae::Error::Generic
        })?;

    let width = input.width().min(output.width());
    let height = input.height().min(output.height());
    let context = info.contextPV;
    let stream = info.command_queuePV;
    match cuda_driver::process_bgra128_pitched(
        context,
        stream,
        input_ptr,
        output_ptr,
        width as u32,
        height as u32,
        input.row_bytes() as u32,
        output.row_bytes() as u32,
        mode.kernel_mode(),
    ) {
        Ok(()) => {
            append_log(&format!(
                "CUDA render OK mode={mode:?} context={context:p} stream={stream:p} input={input_ptr:p} output={output_ptr:p} width={width} height={height} in_rowbytes={} out_rowbytes={}",
                input.row_bytes(),
                output.row_bytes(),
            ));
            let _ = cb.checkin_layer_pixels(0);
            Ok(())
        }
        Err(err) => {
            append_log(&format!("CUDA render failed: {err}"));
            let _ = cb.checkin_layer_pixels(0);
            Err(ae::Error::Generic)
        }
    }
}

fn log_gpu_worlds(in_data: &ae::InData, extra: &ae::pf::SmartRenderExtra) {
    let Ok(gpu_suite) = ae::pf::suites::GPUDevice::new() else {
        append_log("GPUDeviceSuite unavailable");
        return;
    };
    let cb = extra.callbacks();

    match gpu_suite.device_count(in_data.effect_ref()) {
        Ok(count) => append_log(&format!("GPUDeviceSuite device_count={count}")),
        Err(err) => append_log(&format!("GPUDeviceSuite device_count error={err:?}")),
    }
    match gpu_suite.device_info(in_data.effect_ref(), extra.device_index()) {
        Ok(info) => append_log(&format!("GPUDeviceInfo raw={info:?}")),
        Err(err) => append_log(&format!("GPUDeviceInfo error={err:?}")),
    }

    match cb.checkout_layer_pixels(0) {
        Ok(Some(mut input)) => {
            append_gpu_world("input", in_data, &gpu_suite, &mut input);
            let _ = cb.checkin_layer_pixels(0);
        }
        Ok(None) => append_log("SmartRenderGpu input checkout returned None"),
        Err(err) => append_log(&format!("SmartRenderGpu input checkout error={err:?}")),
    }

    match cb.checkout_output() {
        Ok(Some(mut output)) => {
            append_gpu_world("output", in_data, &gpu_suite, &mut output);
        }
        Ok(None) => append_log("SmartRenderGpu output checkout returned None"),
        Err(err) => append_log(&format!("SmartRenderGpu output checkout error={err:?}")),
    }
}

fn append_gpu_world(
    label: &str,
    in_data: &ae::InData,
    gpu_suite: &ae::pf::suites::GPUDevice,
    world: &mut ae::Layer,
) {
    let world_ptr = WorldPtr(ae::AsMutPtr::as_mut_ptr(world));
    append_log(&format!(
        "{label} world width={} height={} rowbytes={} bit_depth={}",
        world.width(),
        world.height(),
        world.row_bytes(),
        world.bit_depth()
    ));

    match gpu_suite.gpu_world_device_index(in_data.effect_ref(), world_ptr) {
        Ok(index) => append_log(&format!("{label} gpu_world_device_index={index}")),
        Err(err) => append_log(&format!("{label} gpu_world_device_index error={err:?}")),
    }
    match gpu_suite.gpu_world_size(in_data.effect_ref(), world_ptr) {
        Ok(size) => append_log(&format!("{label} gpu_world_size={size}")),
        Err(err) => append_log(&format!("{label} gpu_world_size error={err:?}")),
    }
    match gpu_suite.gpu_world_data(in_data.effect_ref(), world) {
        Ok(ptr) => append_log(&format!("{label} gpu_world_data={ptr:p}")),
        Err(err) => append_log(&format!("{label} gpu_world_data error={err:?}")),
    }
    match ae::pf::suites::PixelData::new() {
        Ok(pixel_suite) => match pixel_suite.pixel_data_float_gpu(world_ptr) {
            Ok(ptr) => append_log(&format!("{label} pixel_data_float_gpu={ptr:p}")),
            Err(err) => append_log(&format!("{label} pixel_data_float_gpu error={err:?}")),
        },
        Err(err) => append_log(&format!("{label} PixelDataSuite unavailable error={err:?}")),
    }
}

#[derive(Clone, Copy)]
struct WorldPtr(*mut ae::sys::PF_EffectWorld);

impl ae::AsPtr<*mut ae::sys::PF_EffectWorld> for WorldPtr {
    fn as_ptr(&self) -> *mut ae::sys::PF_EffectWorld {
        self.0
    }
}

#[derive(Clone, Copy, Debug)]
struct ProbeState {
    last: Option<AeGpuDeviceProbe>,
    setup_count: u32,
}

impl Default for ProbeState {
    fn default() -> Self {
        Self {
            last: None,
            setup_count: 0,
        }
    }
}

fn smart_pre_render(
    in_data: &ae::InData,
    extra: &mut ae::pf::PreRenderExtra,
) -> Result<(), ae::Error> {
    let req = extra.output_request();
    let input = extra.callbacks().checkout_layer(
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

fn smart_render(extra: &ae::pf::SmartRenderExtra) -> Result<(), ae::Error> {
    let cb = extra.callbacks();
    let input = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
    let mut output = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
    copy_layer(&input, &mut output);
    cb.checkin_layer_pixels(0)?;
    Ok(())
}

fn copy_layer(input: &ae::Layer, output: &mut ae::Layer) {
    let width = input.width().min(output.width()) as usize;
    let height = input.height().min(output.height()) as usize;
    let in_stride = input.buffer_stride();
    let out_stride = output.buffer_stride();
    let src = input.buffer();
    let dst = output.buffer_mut();
    let row_len = width * 4;
    for y in 0..height {
        let si = y * in_stride;
        let di = y * out_stride;
        if si + row_len <= src.len() && di + row_len <= dst.len() {
            dst[di..di + row_len].copy_from_slice(&src[si..si + row_len]);
        }
    }
}

fn record_probe(probe: AeGpuDeviceProbe, selector: &str) {
    if let Ok(mut state) = state_cell().lock() {
        state.last = Some(probe);
        state.setup_count += 1;
    }
    append_log(&format!(
        "{} framework={:?} device={} class={:?}",
        selector,
        probe.framework,
        probe.device_index,
        probe.classify()
    ));
}

fn state_cell() -> &'static Mutex<ProbeState> {
    LAST_PROBE.get_or_init(|| Mutex::new(ProbeState::default()))
}

fn about_message() -> String {
    let state = state_cell().lock().map(|s| *s).unwrap_or_default();
    let header = match state.last {
        Some(probe) => format!(
            "AeGpuProbe v0.1\rGPU selector/logger\rSetups: {}\rLast: {:?} device {}\rClass: {:?}\rLog: {}",
            state.setup_count,
            probe.framework,
            probe.device_index,
            probe.classify(),
            log_path().display(),
        ),
        None => format!(
            "AeGpuProbe v0.1\rNo GPU setup selector observed yet.\rLog: {}",
            log_path().display()
        ),
    };
    format!("{header}\r\rRecent log:\r{}", recent_log_tail(14))
}

fn append_log(line: &str) {
    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

fn recent_log_tail(max_lines: usize) -> String {
    let Ok(contents) = fs::read_to_string(log_path()) else {
        return "(log file not found yet)".to_string();
    };
    let mut lines = contents.lines().rev().take(max_lines).collect::<Vec<_>>();
    lines.reverse();
    if lines.is_empty() {
        "(log is empty)".to_string()
    } else {
        lines.join("\r")
    }
}

fn log_path() -> PathBuf {
    std::env::temp_dir()
        .join("onmk-ae-gpu-lab")
        .join("AeGpuProbe.log")
}

fn map_framework(framework: ae::GpuFramework) -> AeGpuFramework {
    match framework {
        ae::GpuFramework::Metal => AeGpuFramework::Metal,
        ae::GpuFramework::Cuda => AeGpuFramework::Cuda,
        ae::GpuFramework::DirectX => AeGpuFramework::DirectX12,
        ae::GpuFramework::OpenCl => AeGpuFramework::OpenGL,
        ae::GpuFramework::None => AeGpuFramework::Unknown(0),
    }
}
