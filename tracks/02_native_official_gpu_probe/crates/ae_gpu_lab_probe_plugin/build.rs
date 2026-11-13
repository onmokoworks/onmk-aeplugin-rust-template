use pipl::*;

fn main() {
    // `cfg!(target_os)` in a build script describes the build host, so read
    // the target from Cargo instead.
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    ae_plugin_build::build("EffectMain", || {
        let mut flags2 = OutFlags2::SupportsSmartRender
            | OutFlags2::FloatColorAware
            | OutFlags2::SupportsGpuRenderF32;
        if windows {
            flags2 |= OutFlags2::SupportsDirectxRendering;
        }
        vec![
            Property::Kind(PIPLType::AEEffect),
            Property::Name("AeGpuProbe"),
            Property::Category("ONMK Lab"),
            Property::AE_PiPL_Version { major: 2, minor: 0 },
            Property::AE_Effect_Spec_Version {
                major: 13,
                minor: 28,
            },
            Property::AE_Effect_Version {
                version: 1,
                subversion: 0,
                bugversion: 0,
                stage: Stage::Develop,
                build: 1,
            },
            Property::AE_Effect_Info_Flags(0),
            Property::AE_Effect_Global_OutFlags(OutFlags::DeepColorAware),
            Property::AE_Effect_Global_OutFlags_2(flags2),
            Property::AE_Effect_Match_Name("ONMK AeGpuProbe"),
            Property::AE_Reserved_Info(10),
            Property::AE_Effect_Support_URL(
                "https://github.com/onmokoworks/onmk-aeplugin-rust-template",
            ),
        ]
    });
}
