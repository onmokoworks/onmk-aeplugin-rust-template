struct LabParams {
    size: vec4<u32>,
    control0: vec4<f32>,
    control1: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: LabParams;
@group(0) @binding(1) var<storage, read> input_img: array<u32>;
@group(0) @binding(2) var<storage, read_write> tmp_img: array<u32>;
@group(0) @binding(3) var<storage, read_write> output_img: array<u32>;

fn inside(id: vec3<u32>) -> bool {
    return id.x < params.size.x && id.y < params.size.y;
}

fn idx(x: u32, y: u32) -> u32 {
    return y * params.size.x + x;
}

fn unpack_rgba(p: u32) -> vec4<f32> {
    return vec4<f32>(
        f32(p & 0xffu),
        f32((p >> 8u) & 0xffu),
        f32((p >> 16u) & 0xffu),
        f32((p >> 24u) & 0xffu)
    );
}

fn pack_rgba(c: vec4<f32>) -> u32 {
    let r = u32(clamp(c.r, 0.0, 255.0));
    let g = u32(clamp(c.g, 0.0, 255.0));
    let b = u32(clamp(c.b, 0.0, 255.0));
    let a = u32(clamp(c.a, 0.0, 255.0));
    return r | (g << 8u) | (b << 16u) | (a << 24u);
}

fn sample_px(x: i32, y: i32) -> vec4<f32> {
    let sx = u32(clamp(x, 0, i32(params.size.x) - 1));
    let sy = u32(clamp(y, 0, i32(params.size.y) - 1));
    return unpack_rgba(input_img[idx(sx, sy)]);
}

@compute @workgroup_size(16, 16, 1)
fn copy_main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) { return; }
    let p = idx(id.x, id.y);
    output_img[p] = input_img[p];
}

@compute @workgroup_size(16, 16, 1)
fn color_main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) { return; }
    let p = idx(id.x, id.y);
    let s = params.control0.x;
    let c = unpack_rgba(input_img[p]);
    output_img[p] = pack_rgba(vec4<f32>(
        c.r * (1.0 + s * 0.20) + c.g * 0.05,
        c.g * (1.0 - s * 0.10) + c.b * 0.04,
        c.b * (1.0 + s * 0.15) + c.r * 0.03,
        c.a
    ));
}

@compute @workgroup_size(16, 16, 1)
fn box_blur_main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) { return; }
    let r = i32(min(params.size.w, 32u));
    var sum = vec4<f32>(0.0);
    var count = 0.0;
    for (var oy = -r; oy <= r; oy = oy + 1) {
        for (var ox = -r; ox <= r; ox = ox + 1) {
            sum += sample_px(i32(id.x) + ox, i32(id.y) + oy);
            count += 1.0;
        }
    }
    output_img[idx(id.x, id.y)] = pack_rgba(sum / count);
}

@compute @workgroup_size(16, 16, 1)
fn diffusion_main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) { return; }
    let x = i32(id.x);
    let y = i32(id.y);
    let c = sample_px(x, y) * 0.50
        + sample_px(x - 1, y) * 0.125
        + sample_px(x + 1, y) * 0.125
        + sample_px(x, y - 1) * 0.125
        + sample_px(x, y + 1) * 0.125;
    output_img[idx(id.x, id.y)] = pack_rgba(c);
}

@compute @workgroup_size(16, 16, 1)
fn chroma_warp_main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) { return; }
    let fx = f32(id.x) / f32(max(params.size.x, 1u));
    let fy = f32(id.y) / f32(max(params.size.y, 1u));
    let dx = i32(sin(fy * 37.0) * params.control0.x * 12.0);
    let dy = i32(cos(fx * 31.0) * params.control0.x * 8.0);
    let r = sample_px(i32(id.x) + dx, i32(id.y));
    let g = sample_px(i32(id.x), i32(id.y) + dy);
    let b = sample_px(i32(id.x) - dx, i32(id.y) - dy);
    let a = sample_px(i32(id.x), i32(id.y)).a;
    output_img[idx(id.x, id.y)] = pack_rgba(vec4<f32>(r.r, g.g, b.b, a));
}
