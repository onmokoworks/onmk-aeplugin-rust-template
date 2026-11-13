use pipl::*;

fn main() {
    // Entry points are added for the *target* OS and the PiPL is embedded as
    // a binary resource on Windows (see crates/ae_plugin_build).
    ae_plugin_build::build("EffectMain", || {
        vec![
            Property::Kind(PIPLType::AEEffect),
            Property::Name("AeGpuLab"),
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
            // Without DeepColorAware / FloatColorAware AE hands this effect
            // 8 bpc worlds only, so the 16/32 bpc boundary would never run.
            Property::AE_Effect_Global_OutFlags(OutFlags::DeepColorAware),
            Property::AE_Effect_Global_OutFlags_2(
                OutFlags2::SupportsSmartRender | OutFlags2::FloatColorAware,
            ),
            Property::AE_Effect_Match_Name("ONMK AeGpuLab"),
            Property::AE_Reserved_Info(10),
            Property::AE_Effect_Support_URL(
                "https://github.com/onmokoworks/onmk-aeplugin-rust-template",
            ),
        ]
    });
}
