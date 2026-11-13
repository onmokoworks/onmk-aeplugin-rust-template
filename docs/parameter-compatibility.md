# Parameter and Identity Compatibility

[日本語](./parameter-compatibility.ja.md)

Saved AE projects store an effect by its **match name** and its parameter
values by **index** (the order of `params.add` calls, starting at 1; index 0 is
the input layer). Expressions, presets (`.ffx`) and scripts also refer to
parameters by index or by name. Once a plug-in has been used in a saved
project, treat the following as a public API.

## Never change after release

| Item | Why |
|---|---|
| Match name (`AE_Effect_Match_Name`) | It is the effect's identity. A new match name is a different effect: old projects show it as missing. |
| Order of parameters | Values are restored by index; reordering feeds old values into the wrong controls. |
| Removing a parameter | Shifts every later index. Hide it instead (keep the slot, stop using it). |
| Type of a parameter | A slider that becomes a popup or checkbox restores garbage or fails to load. |
| Meaning of existing popup values | Popup values are 1-based numbers, not labels. |
| Two plug-ins with the same match name | AE loads only one of them; the installer refuses this situation. |

## Allowed

- **Append** new parameters at the end. Old projects get the new parameter's
  default. Add a comment such as `// Appended to preserve indices.` next to it.
- Change **display names** (the `params.add` label, `Property::Name`,
  `Property::Category`). Scripts that look parameters up by name may need an
  update, so mention it in release notes.
- Change **defaults**. A new default only affects newly applied instances;
  saved projects keep their stored value. If the old default must be preserved
  for old projects, add a version flag parameter or use sequence data.
- Widen slider ranges. Narrowing them can clamp stored values.
- **Append** popup options. Keep the existing options in place with the same
  numbers and update the option count (`set_options` with the longer list).
  Rename an option only if its meaning is unchanged.

## Popups

- Values are **1-based**: the first option is 1. The default passed to
  `set_default` is also 1-based and must be within `1..=options.len()`.
- Pick a default that is safe on any layer (for example the "normal" mode, not
  a destructive one).
- Decode unknown values defensively. A project saved by a newer version can
  contain an option number this build does not know; fall back to the default
  instead of failing the render (`Mode::from_popup` in AeMinimal).
- Keep the label list in one constant so the count and default stay
  consistent (`Mode::LABELS`, `Mode::DEFAULT`).

## Layer parameters and SmartFX checkout ids

- A layer parameter's AE index is used in `SmartPreRender`'s `checkout_layer`.
  Keep that index in a named constant next to the parameter list
  (`PARAM_BLEND_LAYER` in AeMinimal), and update it only together with an
  append, never by reordering.
- Checkout ids are your own choice; they only need to match between
  `SmartPreRender` and `SmartRender`.
- An empty layer parameter is normal; treat a failed checkout as "no layer".

## PiPL stability

- Match name, entry point name and PiPL kind must stay the same for the
  lifetime of the effect. Bump `AE_Effect_Version` for releases.
- Do not change global out flags casually: AE caches them, and changing
  flags such as `DeepColorAware` / `FloatColorAware` /
  `SupportsSmartRender` changes how AE renders existing projects. AE checks
  that the PiPL and the `GlobalSetup` flags agree; the template derives both
  from `build.rs` so they cannot drift.
- Non-ASCII names are embedded byte for byte on Windows (see
  `crates/ae_plugin_build`).

## Checklist for a release

1. Diff the `params.add` sequence against the last release: only appends?
2. Same match name? Same parameter types?
3. New popup options only at the end; default within range?
4. Open a project saved with the previous release and check every value.
