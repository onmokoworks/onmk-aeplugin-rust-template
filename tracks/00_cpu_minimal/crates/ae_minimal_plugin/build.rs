use pipl::*;

fn main() {
    // Entry points (CodeWin64X86 / CodeMac*) are added for the target by the
    // helper, which also embeds the PiPL as a binary resource on Windows.
    ae_plugin_build::build("EffectMain", || {
        vec![
            Property::Kind(PIPLType::AEEffect),
            // Display name: may change between versions.
            Property::Name("AeMinimal"),
            Property::Category("ONMK Template"),
            Property::AE_PiPL_Version { major: 2, minor: 0 },
            Property::AE_Effect_Spec_Version {
                major: 13,
                minor: 28,
            },
            Property::AE_Effect_Version {
                version: 0,
                subversion: 1,
                bugversion: 0,
                stage: Stage::Develop,
                build: 1,
            },
            Property::AE_Effect_Info_Flags(0),
            // 16 bpc requires DeepColorAware, 32 bpc FloatColorAware. Without
            // them AE silently converts the layer to 8 bpc for this effect.
            Property::AE_Effect_Global_OutFlags(OutFlags::DeepColorAware),
            Property::AE_Effect_Global_OutFlags_2(
                OutFlags2::SupportsSmartRender | OutFlags2::FloatColorAware,
            ),
            // Match name: identifies the effect in saved projects. NEVER change
            // it after release; see docs/parameter-compatibility.md.
            Property::AE_Effect_Match_Name("ONMK AeMinimal"),
            Property::AE_Reserved_Info(10),
            Property::AE_Effect_Support_URL(
                "https://github.com/onmokoworks/onmk-aeplugin-rust-template",
            ),
        ]
    });
}
