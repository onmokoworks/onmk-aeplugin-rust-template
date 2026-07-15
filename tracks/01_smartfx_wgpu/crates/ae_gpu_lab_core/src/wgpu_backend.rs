use crate::backend::{BackendTiming, RenderBackend, RenderReport};
use crate::{BenchEffect, FrameRgba8, LabParams};
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;
use wgpu::*;

pub struct WgpuUploadReadback {
    device: Device,
    queue: Queue,
    bind_group_layout: BindGroupLayout,
    pipelines: HashMap<BenchEffect, ComputePipeline>,
    state: Mutex<Option<WgpuState>>,
}

struct WgpuState {
    width: u32,
    height: u32,
    bytes: u64,
    input_buf: Buffer,
    tmp_buf: Buffer,
    output_buf: Buffer,
    params_buf: Buffer,
    staging: Buffer,
    bind_group: BindGroup,
}

impl WgpuUploadReadback {
    pub fn new() -> Result<Self> {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> Result<Self> {
        let instance = Instance::new(&InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                ..Default::default()
            })
            .await?;
        let (device, queue) = adapter
            .request_device(&DeviceDescriptor {
                label: Some("ae_gpu_lab_device"),
                required_features: Features::empty(),
                required_limits: adapter.limits(),
                memory_hints: MemoryHints::Performance,
                trace: Trace::Off,
            })
            .await?;
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("ae_gpu_lab_shader"),
            source: ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("lab_bgl"),
            entries: &[
                bgl_uniform(0),
                bgl_storage_ro(1),
                bgl_storage_rw(2),
                bgl_storage_rw(3),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("lab_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let mut pipelines = HashMap::new();
        for effect in [
            BenchEffect::Copy,
            BenchEffect::Color,
            BenchEffect::BoxBlur,
            BenchEffect::Diffusion,
            BenchEffect::ChromaWarp,
        ] {
            pipelines.insert(
                effect,
                device.create_compute_pipeline(&ComputePipelineDescriptor {
                    label: Some(effect.shader_entry()),
                    layout: Some(&pipeline_layout),
                    module: &shader,
                    entry_point: Some(effect.shader_entry()),
                    compilation_options: Default::default(),
                    cache: None,
                }),
            );
        }

        Ok(Self {
            device,
            queue,
            bind_group_layout,
            pipelines,
            state: Mutex::new(None),
        })
    }

    pub fn render_frame(
        &self,
        effect: BenchEffect,
        input: &FrameRgba8,
        params: LabParams,
    ) -> Result<RenderReport> {
        let total_start = Instant::now();
        let n = input.width as u64 * input.height as u64;
        let bytes = n * 4;
        if input.pixels.len() as u64 != bytes {
            return Err(anyhow!(
                "input pixel buffer size does not match frame extent"
            ));
        }

        let mut state_guard = self.state.lock().map_err(|e| anyhow!("{e}"))?;
        if state_guard
            .as_ref()
            .map(|s| s.width != input.width || s.height != input.height)
            .unwrap_or(true)
        {
            *state_guard = Some(self.create_state(input.width, input.height, bytes));
        }
        let state = state_guard.as_ref().expect("wgpu state should exist");

        let upload_start = Instant::now();
        self.queue.write_buffer(&state.input_buf, 0, &input.pixels);
        self.queue.write_buffer(&state.tmp_buf, 0, &input.pixels);
        self.queue
            .write_buffer(&state.params_buf, 0, bytemuck::bytes_of(&params));
        let upload = upload_start.elapsed();
        let pipeline = self
            .pipelines
            .get(&effect)
            .ok_or_else(|| anyhow!("missing pipeline for {effect:?}"))?;

        let encode_start = Instant::now();
        let wg_x = (input.width + 15) / 16;
        let wg_y = (input.height + 15) / 16;
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("lab_encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &state.bind_group, &[]);
            pass.dispatch_workgroups(wg_x, wg_y, 1);
        }
        encoder.copy_buffer_to_buffer(&state.output_buf, 0, &state.staging, 0, state.bytes);
        let command_buffer = encoder.finish();
        let encode = encode_start.elapsed();

        let submit_start = Instant::now();
        self.queue.submit(Some(command_buffer));
        let submit_wait = submit_start.elapsed();

        let map_start = Instant::now();
        let slice = state.staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        let _ = self.device.poll(PollType::Wait);
        rx.recv().map_err(|e| anyhow!("{e}"))??;
        let readback_map = map_start.elapsed();

        let copy_start = Instant::now();
        let mapped = slice.get_mapped_range();
        let pixels = mapped.to_vec();
        drop(mapped);
        state.staging.unmap();
        let readback_copy = copy_start.elapsed();

        Ok(RenderReport {
            frame: FrameRgba8::new(input.width, input.height, pixels),
            timing: BackendTiming {
                upload,
                encode,
                submit_wait,
                readback_map,
                readback_copy,
                total: total_start.elapsed(),
            },
        })
    }

    fn create_state(&self, width: u32, height: u32, bytes: u64) -> WgpuState {
        let storage = BufferUsages::STORAGE | BufferUsages::COPY_SRC | BufferUsages::COPY_DST;
        let input_buf = self.device.create_buffer(&BufferDescriptor {
            label: Some("input"),
            size: bytes,
            usage: storage,
            mapped_at_creation: false,
        });
        let tmp_buf = self.device.create_buffer(&BufferDescriptor {
            label: Some("tmp"),
            size: bytes,
            usage: storage,
            mapped_at_creation: false,
        });
        let output_buf = self.device.create_buffer(&BufferDescriptor {
            label: Some("output"),
            size: bytes,
            usage: storage,
            mapped_at_creation: false,
        });
        let params_buf = self.device.create_buffer(&BufferDescriptor {
            label: Some("params"),
            size: std::mem::size_of::<LabParams>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let staging = self.device.create_buffer(&BufferDescriptor {
            label: Some("staging"),
            size: bytes,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = self.device.create_bind_group(&BindGroupDescriptor {
            label: Some("lab_bg"),
            layout: &self.bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: params_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: input_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: tmp_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: output_buf.as_entire_binding(),
                },
            ],
        });

        WgpuState {
            width,
            height,
            bytes,
            input_buf,
            tmp_buf,
            output_buf,
            params_buf,
            staging,
            bind_group,
        }
    }
}

impl RenderBackend for WgpuUploadReadback {
    fn name(&self) -> &'static str {
        "wgpu_upload_readback"
    }

    fn render(
        &self,
        effect: BenchEffect,
        input: &FrameRgba8,
        params: LabParams,
    ) -> Result<RenderReport> {
        self.render_frame(effect, input, params)
    }
}

fn bgl_uniform(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        count: None,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: BufferSize::new(std::mem::size_of::<LabParams>() as u64),
        },
    }
}

fn bgl_storage_ro(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        count: None,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
    }
}

fn bgl_storage_rw(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        count: None,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
    }
}
