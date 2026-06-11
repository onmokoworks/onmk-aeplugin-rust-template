use pipl::*;

fn main() {
    pipl::plugin_build(vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name("AeGpuLab"),
        Property::Category("ONMK Lab"),
        #[cfg(target_os = "windows")]
        Property::CodeWin64X86("EffectMain"),
        #[cfg(target_os = "macos")]
        Property::CodeMacIntel64("EffectMain"),
        #[cfg(target_os = "macos")]
        Property::CodeMacARM64("EffectMain"),
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
        Property::AE_Effect_Global_OutFlags(OutFlags::empty()),
        Property::AE_Effect_Global_OutFlags_2(OutFlags2::SupportsSmartRender),
        Property::AE_Effect_Match_Name("ONMK AeGpuLab"),
        Property::AE_Reserved_Info(10),
        Property::AE_Effect_Support_URL("https://github.com/onmokoworks/AeRustGpuLab"),
    ]);
}
