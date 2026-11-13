# Windows / macOS Portability

[日本語](./cross-platform.ja.md)

## SDK types differ between platforms

`after-effects-sys` ships bindgen output generated separately from the Windows
and macOS SDK headers. The same C declaration can become different Rust types:

| Item | Windows | macOS |
|---|---|---|
| `PF_WorldFlag_DEEP` and other `PF_WorldFlag_*` constants | `c_int` (`i32`) | `c_uint` (`u32`) |
| `PF_EffectWorld::world_flags` (`A_long`) | `i32` | `i32` |

`world.world_flags & PF_WorldFlag_DEEP` compiles on Windows and fails on macOS.
Hand-written `as i32` / `as u32` casts tend to compile on only one platform or
truncate silently. Widen both sides instead:

```rust
use ae_plugin_kit::sdk::{has_flag, same_bits};

if has_flag(world.world_flags, ae::sys::PF_WorldFlag_DEEP) { /* ... */ }
assert!(same_bits(a_long_value, some_u32_constant));
```

`ae_plugin_kit::sdk::SdkInt` widens every 8/16/32-bit signed or unsigned value
to `i64` exactly. `ae_plugin_kit::ae::has_world_flag(layer, flag)` wraps this
for layers.

## Pixel depth: use the World Suite

`Layer::bit_depth()` in the pinned wrapper returns 16 whenever
`PF_WorldFlag_DEEP` is set. AE sets that flag on 32 bpc (ARGB128) worlds too,
so a float world would be read as 16-bit integers. Use
`ae_plugin_kit::ae::pixel_depth(layer)`, which asks the World Suite for
`PF_PixelFormat_ARGB32 / ARGB64 / ARGB128` and rejects everything else
(including `GPU_BGRA128`).

## Platform-specific code

- Gate Windows-only APIs (CUDA through `nvcuda.dll`, D3D12, DirectX flags) with
  `#[cfg(windows)]` and make their crates `[target.'cfg(windows)'.dependencies]`.
  The probe plug-in does this for its CUDA path; on macOS it refuses
  `SmartRenderGpu` and logs the GPU world instead.
- Gate Metal code with `#[cfg(target_os = "macos")]` (see track 03).
- `#[cfg(target_os)]` inside **`build.rs`** describes the build host, not the
  target. Read `CARGO_CFG_TARGET_OS` instead. `ae_plugin_build::build` adds
  `CodeWin64X86` or `CodeMacIntel64` + `CodeMacARM64` for the target.
- Byte order: AE ARGB worlds use native endianness; both supported CPU
  families are little-endian, and `ae_plugin_kit::pixels` uses `from_ne_bytes`.

## Building for macOS

- Build on a Mac. `pipl::plugin_build` writes `<crate>.rsrc`, `<crate>_PkgInfo`
  and `<crate>_Info.plist` next to the build output; bundle them with the dylib
  into `<Name>.plugin/Contents/{MacOS,Resources}` and sign it. The template does
  not ship a bundling script yet.
- Universal binaries: build `aarch64-apple-darwin` and `x86_64-apple-darwin`
  and `lipo` them together.
- CI (`.github/workflows/ci.yml`) builds and tests the workspace on macOS
  arm64 and x86_64 runners; it does not bundle, sign or load the plug-ins.
- On Windows, `cargo check --target aarch64-apple-darwin` type-checks the
  crates (useful for cfg mistakes). It needs a C compiler for the `oslog`
  dependency, and the PiPL resource it produces is a placeholder. It cannot
  link or bundle.

## Windows PiPL resource

The pinned `pipl` crate writes the PiPL into the `.rc` file as an escaped string
literal. `rc.exe` re-encodes string literals through the active code page, so
on Japanese Windows (CP932) bytes >= 0x80 are corrupted (non-ASCII names and
some version/flag bytes). `ae_plugin_build` writes the exact bytes to
`$OUT_DIR/effect.pipl` and embeds the file as resource `16000 PiPL`, which
`rc.exe` copies verbatim.
