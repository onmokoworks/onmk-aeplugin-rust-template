use anyhow::{bail, Result};
use wgpu::util::DeviceExt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Bgra128Mode {
    Copy,
    InvertRgb,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bgra128Dispatch {
    pub width: u32,
    pub height: u32,
    pub input_row_bytes: u32,
    pub output_row_bytes: u32,
    pub mode: Bgra128Mode,
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ShaderParams {
    extent_and_strides: [u32; 4],
    controls: [u32; 4],
}

pub struct ExternalBgra128Compute {
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
}

impl ExternalBgra128Compute {
    pub fn new(device: &wgpu::Device) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("AE external BGRA128 layout"),
            entries: &[
                storage_entry(0, true),
                storage_entry(1, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("AE external BGRA128 pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("AE external BGRA128 shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("external_bgra128.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("AE external BGRA128 pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("process_bgra128"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            bind_group_layout,
            pipeline,
        }
    }

    pub fn encode(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        input: &wgpu::Buffer,
        output: &wgpu::Buffer,
        dispatch: Bgra128Dispatch,
    ) -> Result<()> {
        dispatch.validate()?;
        let input_required = dispatch.input_row_bytes as u64 * dispatch.height as u64;
        let output_required = dispatch.output_row_bytes as u64 * dispatch.height as u64;
        if input.size() < input_required || output.size() < output_required {
            bail!("BGRA128 input or output buffer is smaller than its pitched frame layout");
        }
        let params = ShaderParams {
            extent_and_strides: [
                dispatch.width,
                dispatch.height,
                dispatch.input_row_bytes / 16,
                dispatch.output_row_bytes / 16,
            ],
            controls: [
                match dispatch.mode {
                    Bgra128Mode::Copy => 0,
                    Bgra128Mode::InvertRgb => 1,
                },
                0,
                0,
                0,
            ],
        };
        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("AE external BGRA128 params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("AE external BGRA128 bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params_buffer.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("AE external BGRA128 pass"),
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(dispatch.width.div_ceil(16), dispatch.height.div_ceil(16), 1);
        Ok(())
    }
}

impl Bgra128Dispatch {
    pub fn validate(self) -> Result<()> {
        if self.width == 0 || self.height == 0 {
            bail!("BGRA128 dispatch extent must be nonzero");
        }
        let minimum = self.width.checked_mul(16).ok_or_else(|| {
            anyhow::anyhow!("BGRA128 dispatch width overflowed row byte calculation")
        })?;
        if self.input_row_bytes < minimum || self.output_row_bytes < minimum {
            bail!("BGRA128 row pitch is smaller than width * 16");
        }
        if self.input_row_bytes % 16 != 0 || self.output_row_bytes % 16 != 0 {
            bail!("BGRA128 row pitch must be divisible by 16 bytes");
        }
        Ok(())
    }
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_pitched_bgra128_dispatch() {
        Bgra128Dispatch {
            width: 1920,
            height: 1080,
            input_row_bytes: 1920 * 16 + 256,
            output_row_bytes: 1920 * 16 + 512,
            mode: Bgra128Mode::Copy,
        }
        .validate()
        .unwrap();
    }

    #[test]
    fn rejects_unaligned_pitch() {
        assert!(Bgra128Dispatch {
            width: 1,
            height: 1,
            input_row_bytes: 17,
            output_row_bytes: 16,
            mode: Bgra128Mode::Copy,
        }
        .validate()
        .is_err());
    }
}
