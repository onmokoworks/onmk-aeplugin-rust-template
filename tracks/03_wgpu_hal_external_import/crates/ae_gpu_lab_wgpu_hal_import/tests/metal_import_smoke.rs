#![cfg(target_os = "macos")]

use ae_gpu_lab_wgpu_hal_import::metal::{
    MetalExternalContext, MetalExternalImporter, MetalImportPlan,
};
use ae_gpu_lab_wgpu_hal_import::{
    ExternalAccess, ExternalBackend, ExternalFormat, ExternalFrameDesc, ExternalFrameRef,
    ExternalHandle, ExternalResourceKind,
};
use metal::foreign_types::ForeignType;

#[test]
#[ignore = "requires a working Metal adapter"]
fn wraps_same_device_metal_buffer_without_copy() {
    pollster::block_on(async {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::METAL,
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("Metal adapter");
        let (device, _queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Metal external import smoke device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .expect("Metal device");

        let hal_device = unsafe { device.as_hal::<wgpu_hal::api::Metal>() }
            .expect("device should expose Metal HAL");
        let metal_device = hal_device.raw_device().lock();
        let buffer = metal_device.new_buffer(16, metal::MTLResourceOptions::StorageModeShared);
        let command_queue = metal_device.new_command_queue();
        let context = MetalExternalContext {
            device: metal_device.as_ptr().cast(),
            command_queue: command_queue.as_ptr().cast(),
        };
        drop(metal_device);

        let frame = ExternalFrameRef {
            desc: ExternalFrameDesc {
                backend: ExternalBackend::Metal,
                width: 1,
                height: 1,
                row_bytes: 16,
                byte_len: 16,
                format: ExternalFormat::Bgra32Float,
                access: ExternalAccess::ReadWrite,
                resource_kind: ExternalResourceKind::Buffer,
            },
            handle: ExternalHandle::MetalBuffer(buffer.as_ptr().cast()),
        };
        let importer = MetalExternalImporter {
            plan: MetalImportPlan {
                synchronization_defined: true,
                ownership_defined: true,
                allow_write_access: true,
            },
        };
        let imported = unsafe {
            importer.import_buffer_into_wgpu(&device, context, frame, wgpu::BufferUsages::STORAGE)
        }
        .expect("same-device buffer import");
        assert_eq!(imported.size(), 16);
    });
}
