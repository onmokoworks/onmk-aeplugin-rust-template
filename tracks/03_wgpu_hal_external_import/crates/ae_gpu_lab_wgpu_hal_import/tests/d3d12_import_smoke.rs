#![cfg(windows)]

use ae_gpu_lab_wgpu_hal_import::d3d12::{D3D12ExternalImporter, D3D12ImportPlan};
use ae_gpu_lab_wgpu_hal_import::{
    ExternalAccess, ExternalBackend, ExternalFormat, ExternalFrameDesc, ExternalFrameRef,
    ExternalHandle, ExternalResourceKind,
};
use windows::Win32::Graphics::Direct3D12::{
    ID3D12Resource, D3D12_HEAP_FLAG_NONE, D3D12_HEAP_PROPERTIES, D3D12_HEAP_TYPE_DEFAULT,
    D3D12_RESOURCE_DESC, D3D12_RESOURCE_DIMENSION_BUFFER,
    D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_COMMON,
    D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC};
use windows_core::Interface;

#[test]
#[ignore = "requires a working D3D12 adapter"]
fn wraps_same_device_d3d12_buffer_without_copy() {
    pollster::block_on(async {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12,
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("D3D12 adapter");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("D3D12 external import smoke device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .expect("D3D12 device");
        let hal_device = unsafe { device.as_hal::<wgpu_hal::api::Dx12>() }
            .expect("device should expose D3D12 HAL");
        let heap = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_DEFAULT,
            ..Default::default()
        };
        let resource_desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Alignment: 0,
            Width: 16,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_UNKNOWN,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            Flags: D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
        };
        let mut resource: Option<ID3D12Resource> = None;
        unsafe {
            hal_device.raw_device().CreateCommittedResource(
                &heap,
                D3D12_HEAP_FLAG_NONE,
                &resource_desc,
                D3D12_RESOURCE_STATE_COMMON,
                None,
                &mut resource,
            )
        }
        .expect("create simulated AE D3D12 buffer");
        let resource = resource.expect("resource");
        let raw = resource.as_raw();
        let frame = ExternalFrameRef {
            desc: ExternalFrameDesc {
                backend: ExternalBackend::D3D12,
                width: 1,
                height: 1,
                row_bytes: 16,
                byte_len: 16,
                format: ExternalFormat::Bgra32Float,
                access: ExternalAccess::ReadWrite,
                resource_kind: ExternalResourceKind::Buffer,
            },
            handle: ExternalHandle::D3D12Resource(raw),
        };
        let importer = D3D12ExternalImporter {
            plan: D3D12ImportPlan {
                synchronization_defined: true,
                ownership_defined: true,
                allow_write_access: true,
            },
        };
        let imported = unsafe {
            importer.import_buffer_into_wgpu(
                &device,
                frame,
                wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
            )
        }
        .expect("same-device buffer import");
        assert_eq!(imported.size(), 16);

        let input = [0.25f32, 0.5, 0.75, 1.0];
        let mut input_bytes = [0u8; 16];
        for (chunk, value) in input_bytes.chunks_exact_mut(4).zip(input) {
            chunk.copy_from_slice(&value.to_ne_bytes());
        }
        queue.write_buffer(&imported, 0, &input_bytes);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("external import smoke shader"),
            source: wgpu::ShaderSource::Wgsl(
                "@group(0) @binding(0) var<storage, read_write> pixels: array<vec4f>;\n\
                 @compute @workgroup_size(1) fn main() {\n\
                   pixels[0] = vec4f(1.0 - pixels[0].rgb, pixels[0].a);\n\
                 }"
                .into(),
            ),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("external import smoke layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("external import smoke bind group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: imported.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("external import smoke pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("external import smoke pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("external import smoke readback"),
            size: 16,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("external import smoke encoder"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&imported, 0, &readback, 0, 16);
        queue.submit([encoder.finish()]);

        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
        device.poll(wgpu::PollType::Wait).unwrap();
        rx.recv().unwrap().unwrap();
        let mapped = slice.get_mapped_range();
        let output = mapped
            .chunks_exact(4)
            .map(|chunk| f32::from_ne_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(output, vec![0.75, 0.5, 0.25, 1.0]);
    });
}
