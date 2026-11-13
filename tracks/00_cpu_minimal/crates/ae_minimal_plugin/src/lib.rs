//! AeMinimal: the recommended starting point for a CPU effect.
//!
//! - SmartFX (SmartPreRender / SmartRender) plus legacy Render.
//! - 8 / 16 / 32 bpc. Depth comes from the World Suite pixel format.
//! - Every world is validated (stride, alignment, NULL, overflow) before any
//!   pixel access; every checked-out layer is checked in on error and panic.
//! - Straight alpha at the boundary; alpha-weighted processing inside.
//!
//! The AE glue lives here; the testable render logic is in `effect.rs`.

// Both lints fire inside the expansion of `ae::define_effect!`.
#![allow(clippy::drop_non_drop, clippy::question_mark)]

mod effect;

use ae_plugin_kit::ae::{read_layer, write_layer};
use ae_plugin_kit::with_checkouts;
use after_effects as ae;
use effect::{Image, Mode, Settings, MAX_RADIUS};

/// Parameter identifiers.
///
/// COMPATIBILITY: AE stores parameter values by *index* (the order of
/// `params.add` calls). After a release, never reorder, remove or change the
/// type of a parameter. Append new parameters at the end. Display names and
/// defaults may change; a new default only affects newly applied instances.
/// See docs/parameter-compatibility.md.
#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    /// index 1
    Mode,
    /// index 2
    Amount,
    /// index 3
    Radius,
    /// index 4
    BlendLayer,
}

/// AE parameter index of `Params::BlendLayer` (index 0 is the input layer).
const PARAM_BLEND_LAYER: i32 = 4;
/// Our own SmartFX checkout id for the blend layer (0 is the input).
const BLEND_CHECKOUT_ID: i32 = 1;

#[derive(Default)]
struct Plugin;

ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: ae::InData,
        _out_data: ae::OutData,
    ) -> Result<(), ae::Error> {
        params.add(
            Params::Mode,
            "Mode",
            ae::PopupDef::setup(|f| {
                // Popup values are 1-based. Append options only at the end.
                f.set_options(&Mode::LABELS);
                f.set_default(Mode::DEFAULT as i32);
            }),
        )?;
        params.add(
            Params::Amount,
            "Amount",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(100.0);
                f.set_precision(1);
                f.set_display_flags(ae::ValueDisplayFlag::PERCENT);
            }),
        )?;
        params.add(
            Params::Radius,
            "Blur Radius",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(MAX_RADIUS as i32);
                f.set_slider_min(0);
                f.set_slider_max(64);
                f.set_default(4);
            }),
        )?;
        params.add(
            Params::BlendLayer,
            "Blend Layer",
            ae::LayerDef::setup(|_| {}),
        )?;
        Ok(())
    }

    fn handle_command(
        &mut self,
        cmd: ae::Command,
        in_data: ae::InData,
        mut out_data: ae::OutData,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        match cmd {
            ae::Command::About => {
                out_data.set_return_msg(
                    "AeMinimal v0.1\rCPU SmartFX starter: 8/16/32 bpc, straight alpha.\rhttps://github.com/onmokoworks/onmk-aeplugin-rust-template",
                );
            }
            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => render_legacy(params, &in_data, &in_layer, &mut out_layer)?,
            ae::Command::SmartPreRender { mut extra } => smart_pre_render(&in_data, &mut extra)?,
            ae::Command::SmartRender { extra } => smart_render(&extra, params)?,
            // Not advertised (no SupportsGpuRenderF32); refuse defensively.
            ae::Command::SmartRenderGpu { .. } => return Err(ae::Error::Generic),
            _ => {}
        }
        Ok(())
    }
}

fn settings(params: &ae::Parameters<Params>) -> Result<Settings, ae::Error> {
    Ok(Settings {
        mode: Mode::from_popup(params.get(Params::Mode)?.as_popup()?.value()),
        amount: (params.get(Params::Amount)?.as_float_slider()?.value() / 100.0) as f32,
        radius: params
            .get(Params::Radius)?
            .as_slider()?
            .value()
            .clamp(0, MAX_RADIUS as i32) as u32,
    })
}

fn image_from_layer(layer: &ae::Layer) -> Result<Image, ae::Error> {
    let (layout, pixels) = read_layer(layer)?;
    Ok(Image::new(layout.width, layout.height, pixels))
}

fn write_result(result: &Image, out: &mut ae::Layer) -> Result<(), ae::Error> {
    // The output world may differ in size from the input; align top-left.
    let fitted = result.fit(out.width(), out.height());
    write_layer(out, &fitted.pixels).map(|_| ())
}

fn render_legacy(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    in_layer: &ae::Layer,
    out_layer: &mut ae::Layer,
) -> Result<(), ae::Error> {
    let s = settings(params)?;
    let input = image_from_layer(in_layer)?;
    let blend = if s.mode == Mode::BlendLayer {
        legacy_blend_layer(params, in_data)?
    } else {
        None
    };
    write_result(&effect::render(s, &input, blend.as_ref()), out_layer)
}

/// Legacy Render path: check out the layer parameter at the current time. The
/// checkout is released when `checkout` is dropped. An empty layer parameter
/// is not an error.
fn legacy_blend_layer(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
) -> Result<Option<Image>, ae::Error> {
    let Ok(checkout) = params.checkout_at(
        Params::BlendLayer,
        Some(in_data.current_time()),
        Some(in_data.time_step()),
        Some(in_data.time_scale()),
    ) else {
        return Ok(None);
    };
    let layer_def = checkout.as_layer()?;
    match layer_def.value() {
        Some(layer) => image_from_layer(&layer).map(Some),
        None => Ok(None),
    }
}

fn smart_pre_render(
    in_data: &ae::InData,
    extra: &mut ae::pf::PreRenderExtra,
) -> Result<(), ae::Error> {
    let req = extra.output_request();
    let cb = extra.callbacks();
    let input = cb.checkout_layer(
        0,
        0,
        &req,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    )?;

    // This effect does not grow the layer: the blur is clipped to the input
    // bounds. An effect that extends outside must enlarge the request and
    // result rects by its radius here.
    let req_rect: ae::Rect = req.rect.into();
    let mut res: ae::Rect = input.result_rect.into();
    let mut max_res: ae::Rect = input.max_result_rect.into();
    res.left = res.left.max(req_rect.left);
    res.top = res.top.max(req_rect.top);
    res.right = res.right.min(req_rect.right);
    res.bottom = res.bottom.min(req_rect.bottom);
    max_res.left = max_res.left.max(req_rect.left);
    max_res.top = max_res.top.max(req_rect.top);
    max_res.right = max_res.right.min(req_rect.right);
    max_res.bottom = max_res.bottom.min(req_rect.bottom);
    extra.set_result_rect(res);
    extra.set_max_result_rect(max_res);

    // Optional second layer. Failure (no layer chosen) is not fatal.
    let _ = cb.checkout_layer(
        PARAM_BLEND_LAYER,
        BLEND_CHECKOUT_ID,
        &req,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    );
    Ok(())
}

fn smart_render(
    extra: &ae::pf::SmartRenderExtra,
    params: &ae::Parameters<Params>,
) -> Result<(), ae::Error> {
    let cb = extra.callbacks();
    // Every layer checked out through `t` is checked in again on success,
    // error and panic; a panic becomes `Error::Generic` after cleanup.
    with_checkouts(&cb, ae::Error::Generic, |t| {
        let s = settings(params)?;
        let input_world = t.checkout_required(0, ae::Error::Generic)?;
        let input = image_from_layer(&input_world)?;
        let blend = if s.mode == Mode::BlendLayer {
            match t.checkout(BLEND_CHECKOUT_ID as u32) {
                Ok(Some(world)) => Some(image_from_layer(&world)?),
                _ => None,
            }
        } else {
            None
        };
        let result = effect::render(s, &input, blend.as_ref());
        let mut output = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
        write_result(&result, &mut output)
    })
}
