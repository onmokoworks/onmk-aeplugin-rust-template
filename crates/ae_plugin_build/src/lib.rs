//! `build.rs` helper for AE effect plug-ins.
//!
//! ```ignore
//! // build.rs
//! use pipl::*;
//! fn main() {
//!     ae_plugin_build::build("EffectMain", || vec![
//!         Property::Kind(PIPLType::AEEffect),
//!         Property::Name("My Effect"),
//!         Property::Category("My Category"),
//!         // ... everything except the Code* entry-point properties
//!     ]);
//! }
//! ```
//!
//! What it does differently from calling `pipl::plugin_build` directly:
//!
//! - **Entry points follow the *target*, not the build host.** `#[cfg(target_os)]`
//!   inside `build.rs` describes the machine running the build script. The
//!   helper reads `CARGO_CFG_TARGET_OS` and adds `CodeWin64X86` or
//!   `CodeMacIntel64` + `CodeMacARM64` itself.
//! - **Windows: the PiPL is embedded as a binary resource.** The pinned
//!   `pipl` crate writes the PiPL into the `.rc` file as an escaped string
//!   literal. `rc.exe` re-encodes string literals through the active code page,
//!   so on Japanese Windows (CP932) bytes >= 0x80 — non-ASCII names, some
//!   version/flag bytes — are corrupted. The helper writes the exact bytes to
//!   `$OUT_DIR/effect.pipl` and references the file from the `.rc`, which
//!   `rc.exe` copies verbatim.
//! - **macOS:** unchanged; `pipl::plugin_build` writes the `.rsrc`, `PkgInfo`
//!   and `Info.plist` next to the build output for bundling.
//! - Sets the `catch_panics` cfg so the `after-effects` entry point catches
//!   panics in release builds as a last line of defence.
//!
//! On Windows `pipl::plugin_build` is still called first because it emits the
//! `PIPL_*` environment values and cfgs that `ae::define_effect!` reads. Its
//! string-literal resource is then replaced: `winres` always writes
//! `$OUT_DIR/resource.rc` / `resource.lib`, so the second compile overwrites the
//! first and only one PiPL resource is linked. [`build`] verifies this.

use pipl::Property;
use std::path::PathBuf;

/// Resource id AE looks up for the PiPL.
pub const PIPL_RESOURCE_ID: u32 = 16000;

/// Emit the PiPL and build metadata for an effect.
///
/// `properties` must not contain `Code*` entry-point properties; they are added
/// for the current target. It is a closure because `pipl::Property` is not
/// `Clone` and the Windows path serializes the list twice.
pub fn build(entry_point: &'static str, properties: impl Fn() -> Vec<Property>) {
    println!("cargo:rustc-cfg=catch_panics");
    println!("cargo:rustc-check-cfg=cfg(catch_panics)");
    println!("cargo:rerun-if-changed=build.rs");

    let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let with_entry = || with_entry_points(properties(), &target, entry_point);

    if target == "windows" {
        // Env values + cfgs for define_effect!, plus a string-literal resource
        // that is overwritten below.
        pipl::plugin_build(with_entry());
        let bytes = pipl::build_pipl(with_entry()).expect("serialize PiPL");
        embed_binary_pipl(&bytes);
    } else if host_is_windows() {
        cross_check_from_windows(with_entry());
    } else {
        pipl::plugin_build(with_entry());
    }
}

fn host_is_windows() -> bool {
    std::env::var("HOST").is_ok_and(|host| host.contains("windows"))
}

/// `cargo check --target aarch64-apple-darwin` on a Windows host.
///
/// The pinned `pipl` selects its resource format with `#[cfg(target_os)]`,
/// i.e. by the *host*, so it tries to compile a Windows `.rc` for the macOS
/// target and `winres` refuses because the target env is not msvc/gnu. For a
/// type-check this resource is irrelevant, so let `winres` believe the target
/// is msvc while `pipl` emits its `PIPL_*` values, then restore the variable.
/// The result cannot be linked or bundled; build macOS plug-ins on a Mac.
fn cross_check_from_windows(props: Vec<Property>) {
    println!(
        "cargo:warning=Building a non-Windows target on a Windows host: the PiPL resource is a \
         placeholder (type-check only). Build macOS bundles on macOS."
    );
    let saved = std::env::var_os("CARGO_CFG_TARGET_ENV");
    std::env::set_var("CARGO_CFG_TARGET_ENV", "msvc");
    pipl::plugin_build(props);
    match saved {
        Some(value) => std::env::set_var("CARGO_CFG_TARGET_ENV", value),
        None => std::env::remove_var("CARGO_CFG_TARGET_ENV"),
    }
}

/// Insert the target's entry-point properties right after `Category` (or
/// `Name`/`Kind` if absent), matching the layout of the SDK samples.
pub fn with_entry_points(
    mut props: Vec<Property>,
    target_os: &str,
    entry_point: &'static str,
) -> Vec<Property> {
    assert!(
        !props.iter().any(|p| matches!(
            p,
            Property::CodeWin64X86(_) | Property::CodeMacIntel64(_) | Property::CodeMacARM64(_)
        )),
        "pass properties without Code* entries; ae_plugin_build adds them for the target"
    );
    let at = props
        .iter()
        .rposition(|p| {
            matches!(
                p,
                Property::Kind(_) | Property::Name(_) | Property::Category(_)
            )
        })
        .map_or(0, |i| i + 1);
    let entries: Vec<Property> = match target_os {
        "windows" => vec![Property::CodeWin64X86(entry_point)],
        "macos" => vec![
            Property::CodeMacIntel64(entry_point),
            Property::CodeMacARM64(entry_point),
        ],
        // Other targets (e.g. Linux CI `cargo check`) get no entry point;
        // pipl then defaults the env value to `EffectMain`.
        _ => Vec::new(),
    };
    for (offset, entry) in entries.into_iter().enumerate() {
        props.insert(at + offset, entry);
    }
    props
}

/// `.rc` line that embeds `path` verbatim as the PiPL resource.
pub fn rc_line(path: &std::path::Path) -> String {
    format!(
        "{PIPL_RESOURCE_ID} PiPL DISCARDABLE \"{}\"",
        path.display().to_string().replace('\\', "/")
    )
}

fn embed_binary_pipl(bytes: &[u8]) {
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let pipl_path = out_dir.join("effect.pipl");
    std::fs::write(&pipl_path, bytes).expect("write effect.pipl");
    let line = rc_line(&pipl_path);
    winres::WindowsResource::new()
        .append_rc_content(&line)
        .compile()
        .expect("compile binary PiPL resource (rc.exe from the Windows SDK is required)");
    // Guard against a future winres/pipl change that would link two PiPLs.
    let rc = std::fs::read_to_string(out_dir.join("resource.rc")).expect("read resource.rc");
    assert!(
        rc.contains(&line) && !rc.contains("PiPL DISCARDABLE BEGIN"),
        "resource.rc does not contain only the binary PiPL resource"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipl::*;

    fn props() -> Vec<Property> {
        vec![
            Property::Kind(PIPLType::AEEffect),
            Property::Name("Caf\u{e9} \u{30c6}\u{30b9}\u{30c8}"),
            Property::Category("Test"),
            Property::AE_Effect_Match_Name("TEST Match"),
        ]
    }

    #[test]
    fn entry_points_follow_target_os() {
        let win = with_entry_points(props(), "windows", "EffectMain");
        assert!(matches!(win[3], Property::CodeWin64X86("EffectMain")));
        let mac = with_entry_points(props(), "macos", "EffectMain");
        assert!(matches!(mac[3], Property::CodeMacIntel64("EffectMain")));
        assert!(matches!(mac[4], Property::CodeMacARM64("EffectMain")));
        assert_eq!(with_entry_points(props(), "linux", "EffectMain").len(), 4);
    }

    #[test]
    #[should_panic(expected = "without Code")]
    fn explicit_entry_points_are_rejected() {
        let mut p = props();
        p.push(Property::CodeWin64X86("EffectMain"));
        with_entry_points(p, "windows", "EffectMain");
    }

    #[test]
    fn non_ascii_bytes_are_serialized_verbatim() {
        let bytes = build_pipl(with_entry_points(props(), "windows", "EffectMain")).unwrap();
        let name = "Caf\u{e9} \u{30c6}\u{30b9}\u{30c8}".as_bytes();
        assert!(name.iter().any(|b| *b >= 0x80));
        assert!(bytes.windows(name.len()).any(|w| w == name));
    }

    #[test]
    fn rc_line_references_a_file_not_a_string_literal() {
        let line = rc_line(std::path::Path::new(r"C:\out dir\effect.pipl"));
        assert_eq!(line, "16000 PiPL DISCARDABLE \"C:/out dir/effect.pipl\"");
    }
}
