//! Mouse look and fire.
//!
//! Locked-cursor aiming cannot use the cursor position: once the pointer is
//! grabbed it stops moving, so `pointer.delta()` stays zero. Desktop builds
//! get an unfiltered device delta. On macOS that delta is dropped by the
//! window toolkit after a lock (it treats the cursor as having left), so the
//! system mouse delta is read directly.

use eframe::egui;

pub struct MouseSample {
    pub dx: f32,
    pub dy: f32,
    pub fire: bool,
    pub jet: bool,
}

// Unit tests use injected events, not global OS button state. CoreGraphics can
// block when parallel test threads query it without a native application loop.
#[cfg(all(test, target_os = "macos"))]
pub fn sample(ctx: &egui::Context, playing: bool) -> MouseSample {
    ctx.input(|i| MouseSample { dx:0.0, dy:0.0,
        fire:playing && i.pointer.primary_down(), jet:playing && i.pointer.secondary_down() })
}

#[cfg(not(all(test, target_os = "macos")))]
pub fn sample(ctx: &egui::Context, playing: bool) -> MouseSample {
    #[cfg(target_os = "macos")]
    {
        let _ = ctx;
        let (dx, dy) = mac_delta();
        let fire = playing && mac_left_down();
        let jet = playing && mac_right_down();
        return MouseSample { dx, dy, fire, jet };
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = playing;
        other_sample(ctx)
    }
}

#[cfg(not(target_os = "macos"))]
fn other_sample(ctx: &egui::Context) -> MouseSample {
    let (dx, dy, fire, jet) = ctx.input(|i| {
        let motion = i.pointer.motion();
        let (dx, dy) = if let Some(motion) = motion {
            (motion.x, motion.y)
        } else {
            let d = i.pointer.delta();
            (d.x, d.y)
        };
        let web = web_delta();
        (
            dx + web.x,
            dy + web.y,
            i.pointer.button_down(egui::PointerButton::Primary),
            i.pointer.button_down(egui::PointerButton::Secondary),
        )
    });
    MouseSample { dx, dy, fire, jet }
}

#[cfg(all(not(target_os = "macos"), not(target_arch = "wasm32")))]
fn web_delta() -> egui::Vec2 {
    egui::Vec2::ZERO
}

#[cfg(target_arch = "wasm32")]
fn web_delta() -> egui::Vec2 {
    wasm_take_delta()
}

#[cfg(all(target_os = "macos", not(test)))]
fn mac_delta() -> (f32, f32) {
    let mut dx = 0i32;
    let mut dy = 0i32;
    unsafe {
        CGGetLastMouseDelta(&mut dx, &mut dy);
    }
    (dx as f32, dy as f32)
}

#[cfg(all(target_os = "macos", not(test)))]
fn mac_left_down() -> bool {
    unsafe { CGEventSourceButtonState(1, 0) }
}

#[cfg(all(target_os = "macos", not(test)))]
fn mac_right_down() -> bool {
    unsafe { CGEventSourceButtonState(1, 1) }
}

#[cfg(all(target_os = "macos", not(test)))]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGGetLastMouseDelta(dx: *mut i32, dy: *mut i32);
    fn CGEventSourceButtonState(state_id: i32, button: u32) -> bool;
}

#[cfg(target_arch = "wasm32")]
mod wasm_mouse {
    use std::sync::atomic::{AtomicI32, Ordering};

    use eframe::egui::Vec2;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    static DX: AtomicI32 = AtomicI32::new(0);
    static DY: AtomicI32 = AtomicI32::new(0);
    static INSTALLED: AtomicI32 = AtomicI32::new(0);

    pub fn take() -> Vec2 {
        install();
        Vec2::new(
            DX.swap(0, Ordering::Relaxed) as f32,
            DY.swap(0, Ordering::Relaxed) as f32,
        )
    }

    fn install() {
        if INSTALLED.swap(1, Ordering::Relaxed) == 1 {
            return;
        }
        let Some(window) = web_sys::window() else {
            return;
        };
        let Some(document) = window.document() else {
            return;
        };
        let Ok(target) = document.dyn_into::<web_sys::EventTarget>() else {
            return;
        };
        let closure = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(|event: web_sys::MouseEvent| {
            DX.fetch_add(event.movement_x(), Ordering::Relaxed);
            DY.fetch_add(event.movement_y(), Ordering::Relaxed);
        });
        let _ = target
            .add_event_listener_with_callback("mousemove", closure.as_ref().unchecked_ref());
        closure.forget();
    }
}

#[cfg(target_arch = "wasm32")]
fn wasm_take_delta() -> eframe::egui::Vec2 {
    wasm_mouse::take()
}
