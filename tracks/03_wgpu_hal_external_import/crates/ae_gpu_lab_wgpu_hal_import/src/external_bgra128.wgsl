struct Params {
    extent_and_strides: vec4<u32>,
    controls: vec4<u32>,
}

@group(0) @binding(0) var<storage, read> input_pixels: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> output_pixels: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> params: Params;

@compute @workgroup_size(16, 16, 1)
fn process_bgra128(@builtin(global_invocation_id) id: vec3<u32>) {
    let width = params.extent_and_strides.x;
    let height = params.extent_and_strides.y;
    if id.x >= width || id.y >= height {
        return;
    }

    let input_stride = params.extent_and_strides.z;
    let output_stride = params.extent_and_strides.w;
    let input_index = id.y * input_stride + id.x;
    let output_index = id.y * output_stride + id.x;
    var pixel = input_pixels[input_index];
    if params.controls.x == 1u {
        pixel = vec4<f32>(vec3<f32>(1.0) - pixel.rgb, pixel.a);
    }
    output_pixels[output_index] = pixel;
}
