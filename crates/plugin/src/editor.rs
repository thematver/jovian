//! The plugin window: a baseview child view that shows frames rendered by `jovian_gui::Ui`
//! through softbuffer (a CALayer on macOS).

use crate::params::{Holder, JovianParams, UiState};
use crate::{GuiMidi, Shared};
use baseview::{Event as BvEvent, EventStatus, MouseButton, MouseEvent, ScrollDelta, Window, WindowEvent, WindowHandler, WindowOpenOptions, WindowScalePolicy};
use jovian_core::dsp::Meters;
use jovian_core::preset::PatchState;
use jovian_gui::canvas::Canvas;
use jovian_gui::layout::{self, Page};
use jovian_gui::{Button, Event, Host, Key, Mods, Ui};
use keyboard_types::{Key as KtKey, KeyState, Modifiers};
use nih_plug::prelude::*;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use std::any::Any;
use std::cell::Cell;
use std::num::NonZeroU32;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tiny_skia::Pixmap;

const RESIZE_TIMEOUT: Duration = Duration::from_millis(750);

#[derive(Clone, Copy)]
struct ResizeRequest {
    target: (u32, u32),
    previous_zoom: u32,
    started: Instant,
}

#[derive(Debug, PartialEq, Eq)]
enum ResizeOutcome {
    Pending,
    Applied,
    TimedOut,
}

impl ResizeRequest {
    fn outcome(&self, logical: (f64, f64), visible: (f64, f64), now: Instant) -> ResizeOutcome {
        let (w, h) = (self.target.0 as f64, self.target.1 as f64);
        if (logical.0 - w).abs() <= 1.0 && (logical.1 - h).abs() <= 1.0 && visible.0 >= w - 1.0 && visible.1 >= h - 1.0 {
            ResizeOutcome::Applied
        } else if now.duration_since(self.started) >= RESIZE_TIMEOUT {
            ResizeOutcome::TimedOut
        } else {
            ResizeOutcome::Pending
        }
    }
}

fn view_scale(logical_width: f64) -> f32 {
    if logical_width.is_finite() && logical_width > 0.0 { (logical_width / layout::W as f64) as f32 } else { 1.0 }
}

fn zoom_for_size(logical: (f64, f64)) -> Option<u32> {
    jovian_gui::ui::ZOOMS.into_iter().find(|zoom| {
        let (w, h) = size_for_zoom(*zoom);
        (logical.0 - w as f64).abs() <= 1.0 && (logical.1 - h as f64).abs() <= 1.0
    })
}

struct NativeGeometry {
    logical: (f64, f64),
    physical: (u32, u32),
    visible: (f64, f64),
}

fn native_geometry(window: &Window<'_>) -> Option<NativeGeometry> {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc::runtime::Object;
        use objc::{msg_send, sel, sel_impl};
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct Point { x: f64, y: f64 }
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct Size { width: f64, height: f64 }
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct Rect { origin: Point, size: Size }

        let RawWindowHandle::AppKit(h) = window.raw_window_handle() else { return None };
        let view = h.ns_view as *mut Object;
        if view.is_null() { return None; }
        let bounds: Rect = msg_send![view, bounds];
        let visible: Rect = msg_send![view, visibleRect];
        let win: *mut Object = msg_send![view, window];
        let backing: f64 = if win.is_null() { 1.0 } else { msg_send![win, backingScaleFactor] };
        let (w, h) = (bounds.size.width, bounds.size.height);
        if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 || !backing.is_finite() || backing <= 0.0 { return None; }
        Some(NativeGeometry {
            logical: (w, h),
            physical: ((w * backing).round() as u32, (h * backing).round() as u32),
            visible: (visible.size.width.min(w).max(0.0), visible.size.height.min(h).max(0.0)),
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
        None
    }
}

pub struct JovianEditor {
    pub params: Arc<JovianParams>,
    pub shared: Arc<Shared>,
    pub open: Arc<AtomicBool>,
}

pub fn size_for_zoom(zoom: u32) -> (u32, u32) {
    let z = jovian_gui::ui::valid_zoom(zoom) as f32 / 100.0;
    ((layout::W * z).round() as u32, (layout::H * z).round() as u32)
}

impl Editor for JovianEditor {
    fn spawn(&self, parent: ParentWindowHandle, context: Arc<dyn GuiContext>) -> Box<dyn Any + Send> {
        let ui_state = self.params.ui.read().clone();
        let (w, h) = size_for_zoom(ui_state.zoom);
        let params = self.params.clone();
        let shared = self.shared.clone();
        let window = Window::open_parented(
            &ParentAdapter(parent),
            WindowOpenOptions {
                title: "Jovian-8".into(),
                size: baseview::Size::new(w as f64, h as f64),
                scale: WindowScalePolicy::SystemScaleFactor,
            },
            move |window: &mut Window<'_>| EditorWindow::new(window, context, params, shared, ui_state),
        );
        self.open.store(true, Ordering::Release);
        Box::new(EditorHandle { window, open: self.open.clone() })
    }

    fn size(&self) -> (u32, u32) {
        size_for_zoom(self.params.ui.read().zoom)
    }

    fn set_scale_factor(&self, _factor: f32) -> bool {
        // macOS: baseview follows the backing scale of the window itself
        false
    }

    fn param_value_changed(&self, _id: &str, _normalized_value: f32) {}
    fn param_modulation_changed(&self, _id: &str, _modulation_offset: f32) {}
    fn param_values_changed(&self) {}
}

struct EditorHandle {
    window: baseview::WindowHandle,
    open: Arc<AtomicBool>,
}

// The window handle holds raw pointers; it is only used from the GUI thread.
unsafe impl Send for EditorHandle {}

impl Drop for EditorHandle {
    fn drop(&mut self) {
        self.open.store(false, Ordering::Release);
        self.window.close();
    }
}

/// The plugin side of `jovian_gui::Host`.
pub struct PluginHost {
    ctx: Arc<dyn GuiContext>,
    params: Arc<JovianParams>,
    shared: Arc<Shared>,
    /// A size the editor asked for (applied by the window after the event).
    pending_resize: Cell<Option<(u32, u32)>>,
}

impl PluginHost {
    fn with_param(&self, addr: usize, f: impl FnOnce(&ParamSetter, &Holder)) {
        if let Some(h) = self.params.list.get(addr) {
            let setter = ParamSetter::new(self.ctx.as_ref());
            f(&setter, h);
        }
    }
}

impl Host for PluginHost {
    fn get(&self, addr: usize) -> f32 {
        self.params.list.get(addr).map(|h| h.unmodulated()).unwrap_or(0.0)
    }

    fn begin(&self, addr: usize) {
        self.with_param(addr, |s, h| match h {
            Holder::Float(p) => s.begin_set_parameter(p),
            Holder::Int(p) => s.begin_set_parameter(p),
        });
    }

    fn set(&self, addr: usize, v: f32) {
        self.with_param(addr, |s, h| match h {
            Holder::Float(p) => s.set_parameter_normalized(p, v),
            Holder::Int(p) => s.set_parameter_normalized(p, v),
        });
    }

    fn end(&self, addr: usize) {
        self.with_param(addr, |s, h| match h {
            Holder::Float(p) => s.end_set_parameter(p),
            Holder::Int(p) => s.end_set_parameter(p),
        });
    }

    fn load_patch(&self, patch: &PatchState) {
        let state = self.params.state_for(patch);
        self.ctx.set_state(state);
    }

    fn current_patch(&self) -> PatchState {
        self.params.current_patch()
    }

    fn rename(&self, name: &str, bank: &str) {
        let mut p = self.params.patch.write();
        p.name = name.to_string();
        p.pack = bank.to_string();
    }

    fn patch_info(&self) -> (String, String, u64) {
        let p = self.params.patch.read();
        (p.name.clone(), p.pack.clone(), self.params.generation.load(Ordering::Acquire))
    }

    fn meters(&self) -> Meters {
        self.shared.meters()
    }

    fn tempo_bpm(&self) -> f64 {
        self.shared.tempo_bpm.load(Ordering::Relaxed) as f64
    }

    fn compressor_reduction(&self, slot: usize) -> f32 {
        self.shared.comp_reduction.get(slot).map(|a| a.load(Ordering::Relaxed)).unwrap_or(0.0)
    }

    fn midi(&self, status: u8, d1: u8, d2: u8) {
        self.shared.send_gui_midi(GuiMidi(status, d1, d2));
    }

    fn resize(&self, w: u32, h: u32) -> bool {
        self.pending_resize.set(Some((w, h)));
        true
    }
}

struct EditorWindow {
    ui: Ui,
    host: PluginHost,
    _sb_context: softbuffer::Context<SbHandle>,
    surface: softbuffer::Surface<SbHandle, SbHandle>,
    frame: Option<Pixmap>,
    physical: (u32, u32),
    logical: (f64, f64),
    visible: (f64, f64),
    resize_request: Option<ResizeRequest>,
    force: bool,
    cursor: jovian_gui::Cursor,
}

impl EditorWindow {
    fn new(window: &mut Window<'_>, ctx: Arc<dyn GuiContext>, params: Arc<JovianParams>, shared: Arc<Shared>, ui_state: UiState) -> Self {
        let target = sb_handle(window);
        let sb_context = softbuffer::Context::new(target.clone()).expect("softbuffer context");
        let surface = softbuffer::Surface::new(&sb_context, target).expect("softbuffer surface");
        let ui = Ui::new(Page::from_index(ui_state.page), ui_state.zoom);
        let (w, h) = ui.size();
        EditorWindow {
            ui,
            host: PluginHost { ctx, params, shared, pending_resize: Cell::new(None) },
            _sb_context: sb_context,
            surface,
            frame: None,
            physical: (w * 2, h * 2),
            logical: (w as f64, h as f64),
            visible: (w as f64, h as f64),
            resize_request: None,
            force: true,
            cursor: jovian_gui::Cursor::Default,
        }
    }

    fn zoom_factor(&self) -> f32 {
        // Rendering follows the actual view, which may differ from a requested zoom
        // while the host resizes asynchronously or declines to change its container.
        view_scale(self.logical.0)
    }

    fn refresh_geometry(&mut self, window: &Window<'_>) {
        // baseview's macOS view reports backing-scale changes but does not always
        // emit Resized when setFrameSize changes only the logical dimensions.
        if let Some(g) = native_geometry(window) {
            self.visible = g.visible;
            if self.logical != g.logical || self.physical != g.physical {
                self.logical = g.logical;
                self.physical = g.physical;
                self.ui.invalidate_background();
                self.force = true;
                if self.resize_request.is_none() {
                    if let Some(zoom) = zoom_for_size(self.logical) {
                        self.ui.zoom = zoom;
                        self.host.params.ui.write().zoom = zoom;
                    }
                }
            }
        }
    }

    fn restore_zoom(&mut self, window: &mut Window<'_>, zoom: u32, status: &str) {
        self.resize_request = None;
        self.ui.cancel_interaction(&self.host);
        self.ui.zoom = zoom;
        self.host.params.ui.write().zoom = zoom;
        let (w, h) = size_for_zoom(zoom);
        window.resize(baseview::Size::new(w as f64, h as f64));
        self.refresh_geometry(window);
        self.ui.invalidate_background();
        self.ui.set_status(status);
        self.force = true;
    }

    fn reconcile_resize(&mut self, window: &mut Window<'_>) {
        if let Some(request) = self.resize_request {
            match request.outcome(self.logical, self.visible, Instant::now()) {
                ResizeOutcome::Applied => self.resize_request = None,
                ResizeOutcome::TimedOut => self.restore_zoom(window, request.previous_zoom, "The host did not apply the requested zoom"),
                ResizeOutcome::Pending => {}
            }
        }
    }

    /// The pointer position read from the window system (baseview's button events carry none,
    /// so a click without a preceding move would otherwise land at a stale position).
    fn pointer(&self, window: &Window) -> Option<(f32, f32)> {
        #[cfg(target_os = "macos")]
        unsafe {
            use objc::runtime::Object;
            use objc::{class, msg_send, sel, sel_impl};
            #[repr(C)]
            #[derive(Clone, Copy)]
            struct NSPoint {
                x: f64,
                y: f64,
            }
            let RawWindowHandle::AppKit(h) = window.raw_window_handle() else { return None };
            let view = h.ns_view as *mut Object;
            if view.is_null() {
                return None;
            }
            let win: *mut Object = msg_send![view, window];
            if win.is_null() {
                return None;
            }
            // prefer the location of the event being handled (also right for synthetic events)
            let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            let ev: *mut Object = msg_send![app, currentEvent];
            let ev_win: *mut Object = if ev.is_null() { std::ptr::null_mut() } else { msg_send![ev, window] };
            let p: NSPoint = if !ev.is_null() && ev_win == win {
                msg_send![ev, locationInWindow]
            } else {
                msg_send![win, mouseLocationOutsideOfEventStream]
            };
            let nil: *mut Object = std::ptr::null_mut();
            // the baseview view is flipped: y grows downwards from the top-left corner
            let q: NSPoint = msg_send![view, convertPoint: p fromView: nil];
            let z = self.zoom_factor() as f64;
            Some(((q.x / z) as f32, (q.y / z) as f32))
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = window;
            None
        }
    }

    fn to_ui(&self, p: baseview::Point) -> (f32, f32) {
        let z = self.zoom_factor();
        (p.x as f32 / z, p.y as f32 / z)
    }

    /// Sets the mouse cursor (baseview does not implement this on macOS).
    fn update_cursor(&mut self) {
        let c = self.ui.cursor();
        if c == self.cursor {
            return;
        }
        self.cursor = c;
        #[cfg(target_os = "macos")]
        unsafe {
            use objc::runtime::Object;
            use objc::{class, msg_send, sel, sel_impl};
            let cur: *mut Object = match c {
                jovian_gui::Cursor::Default => msg_send![class!(NSCursor), arrowCursor],
                jovian_gui::Cursor::Hand => msg_send![class!(NSCursor), pointingHandCursor],
                jovian_gui::Cursor::ResizeV => msg_send![class!(NSCursor), resizeUpDownCursor],
                jovian_gui::Cursor::Text => msg_send![class!(NSCursor), IBeamCursor],
            };
            if !cur.is_null() {
                let _: () = msg_send![cur, set];
            }
        }
    }

    fn apply_pending(&mut self, window: &mut Window) {
        if let Some((w, h)) = self.host.pending_resize.take() {
            let previous_zoom = self.resize_request.map(|r| r.previous_zoom).unwrap_or_else(|| self.host.params.ui.read().zoom);
            self.resize_request = Some(ResizeRequest { target: (w, h), previous_zoom, started: Instant::now() });
            {
                let mut u = self.host.params.ui.write();
                u.zoom = self.ui.zoom;
            }
            if self.host.ctx.request_resize() {
                window.resize(baseview::Size::new(w as f64, h as f64));
            } else {
                self.restore_zoom(window, previous_zoom, "The host does not allow resizing");
            }
            self.ui.invalidate_background();
            self.force = true;
        }
        let page = self.ui.page.index();
        if self.host.params.ui.read().page != page {
            self.host.params.ui.write().page = page;
        }
    }
}

fn mods(m: &Modifiers) -> Mods {
    Mods {
        shift: m.contains(Modifiers::SHIFT),
        alt: m.contains(Modifiers::ALT),
        cmd: m.contains(Modifiers::META),
        ctrl: m.contains(Modifiers::CONTROL),
    }
}

impl WindowHandler for EditorWindow {
    fn on_frame(&mut self, window: &mut Window) {
        self.refresh_geometry(window);
        self.reconcile_resize(window);
        let dirty = self.ui.tick(&self.host);
        if !dirty && !self.force {
            return;
        }
        self.force = false;
        let (pw, ph) = self.physical;
        if pw == 0 || ph == 0 {
            return;
        }
        if self.frame.as_ref().map(|f| (f.width(), f.height())) != Some((pw, ph)) {
            self.frame = Pixmap::new(pw, ph);
            self.ui.invalidate_background();
        }
        let Some(frame) = self.frame.as_mut() else { return };
        let scale = pw as f32 / layout::W;
        {
            let fonts: *const jovian_gui::text::Fonts = &self.ui.fonts;
            // SAFETY: the fonts are never mutated while the ui renders; the glyph cache uses
            // interior mutability and is only touched from this thread.
            let fonts = unsafe { &*fonts };
            let mut c = Canvas::new(frame.as_mut(), scale, true, fonts);
            self.ui.render(&self.host, &mut c);
        }
        let (Some(w), Some(h)) = (NonZeroU32::new(pw), NonZeroU32::new(ph)) else { return };
        if self.surface.resize(w, h).is_err() {
            return;
        }
        if let Ok(mut buf) = self.surface.buffer_mut() {
            let src = frame.data();
            let dst: &mut [u32] = &mut buf;
            // SAFETY: same length in bytes (w * h * 4); u32 has no invalid bit patterns.
            let dst_bytes = unsafe { std::slice::from_raw_parts_mut(dst.as_mut_ptr() as *mut u8, dst.len() * 4) };
            if dst_bytes.len() == src.len() {
                dst_bytes.copy_from_slice(src);
            }
            let _ = buf.present();
        }
    }

    fn on_event(&mut self, window: &mut Window, event: BvEvent) -> EventStatus {
        self.refresh_geometry(window);
        let status = match event {
            BvEvent::Mouse(m) => {
                let ev = match m {
                    MouseEvent::CursorMoved { position, modifiers } => {
                        let (x, y) = self.to_ui(position);
                        Some(Event::Move { x, y, mods: mods(&modifiers) })
                    }
                    MouseEvent::ButtonPressed { button, modifiers } => {
                        window.focus();
                        if let Some((x, y)) = self.pointer(window) {
                            self.ui.event(&self.host, Event::Move { x, y, mods: mods(&modifiers) });
                        }
                        let (x, y) = self.ui.mouse;
                        let b = if button == MouseButton::Right { Button::Right } else { Button::Left };
                        Some(Event::Down { x, y, button: b, mods: mods(&modifiers) })
                    }
                    MouseEvent::ButtonReleased { button, modifiers } => {
                        let (x, y) = self.ui.mouse;
                        let b = if button == MouseButton::Right { Button::Right } else { Button::Left };
                        Some(Event::Up { x, y, button: b, mods: mods(&modifiers) })
                    }
                    MouseEvent::WheelScrolled { delta, modifiers } => {
                        let (dx, dy) = match delta {
                            ScrollDelta::Lines { x, y } => (x, y),
                            ScrollDelta::Pixels { x, y } => (x / 16.0, y / 16.0),
                        };
                        Some(Event::Wheel { dx, dy, mods: mods(&modifiers) })
                    }
                    MouseEvent::CursorLeft => Some(Event::Leave),
                    MouseEvent::DragDropped { data: baseview::DropData::Files(files), .. } => {
                        for f in files {
                            jovian_gui::pages::library::import_path(&mut self.ui, &self.host, &f);
                        }
                        None
                    }
                    _ => None,
                };
                match ev {
                    Some(e) => {
                        self.ui.event(&self.host, e);
                        EventStatus::Captured
                    }
                    None => EventStatus::Ignored,
                }
            }
            BvEvent::Keyboard(k) => {
                if k.state != KeyState::Down {
                    return EventStatus::Ignored;
                }
                let m = mods(&k.modifiers);
                let ev = match &k.key {
                    KtKey::Character(s) if !m.cmd && !m.ctrl => Some(Event::Text(s.clone())),
                    KtKey::Enter => Some(Event::Key { key: Key::Enter, mods: m }),
                    KtKey::Escape => Some(Event::Key { key: Key::Escape, mods: m }),
                    KtKey::Backspace => Some(Event::Key { key: Key::Backspace, mods: m }),
                    KtKey::Delete => Some(Event::Key { key: Key::Delete, mods: m }),
                    KtKey::ArrowLeft => Some(Event::Key { key: Key::Left, mods: m }),
                    KtKey::ArrowRight => Some(Event::Key { key: Key::Right, mods: m }),
                    KtKey::ArrowUp => Some(Event::Key { key: Key::Up, mods: m }),
                    KtKey::ArrowDown => Some(Event::Key { key: Key::Down, mods: m }),
                    KtKey::Home => Some(Event::Key { key: Key::Home, mods: m }),
                    KtKey::End => Some(Event::Key { key: Key::End, mods: m }),
                    KtKey::Tab => Some(Event::Key { key: Key::Tab, mods: m }),
                    KtKey::PageUp => Some(Event::Key { key: Key::PageUp, mods: m }),
                    KtKey::PageDown => Some(Event::Key { key: Key::PageDown, mods: m }),
                    _ => None,
                };
                match ev.map(|e| self.ui.event(&self.host, e).captured) {
                    Some(true) => EventStatus::Captured,
                    _ => EventStatus::Ignored,
                }
            }
            BvEvent::Window(WindowEvent::Resized(info)) => {
                let p = info.physical_size();
                self.physical = (p.width, p.height);
                let l = info.logical_size();
                self.logical = (l.width, l.height);
                self.visible = self.logical;
                self.force = true;
                EventStatus::Captured
            }
            BvEvent::Window(WindowEvent::Unfocused | WindowEvent::WillClose) => {
                self.ui.cancel_interaction(&self.host);
                self.force = true;
                EventStatus::Ignored
            }
            BvEvent::Window(_) => EventStatus::Ignored,
        };
        self.apply_pending(window);
        self.update_cursor();
        status
    }
}

impl Drop for EditorWindow {
    fn drop(&mut self) {
        // Some hosts destroy the child view without delivering WillClose.
        self.ui.cancel_interaction(&self.host);
    }
}

/// baseview uses raw-window-handle 0.5 like NIH-plug, but its own copy of the type.
struct ParentAdapter(ParentWindowHandle);

unsafe impl HasRawWindowHandle for ParentAdapter {
    fn raw_window_handle(&self) -> RawWindowHandle {
        match self.0 {
            ParentWindowHandle::AppKitNsView(ns_view) => {
                let mut h = raw_window_handle::AppKitWindowHandle::empty();
                h.ns_view = ns_view;
                RawWindowHandle::AppKit(h)
            }
            ParentWindowHandle::Win32Hwnd(hwnd) => {
                let mut h = raw_window_handle::Win32WindowHandle::empty();
                h.hwnd = hwnd;
                RawWindowHandle::Win32(h)
            }
            ParentWindowHandle::X11Window(window) => {
                let mut h = raw_window_handle::XcbWindowHandle::empty();
                h.window = window;
                RawWindowHandle::Xcb(h)
            }
        }
    }
}

/// softbuffer uses raw-window-handle 0.6.
#[derive(Clone)]
struct SbHandle {
    display: raw_window_handle_06::RawDisplayHandle,
    window: raw_window_handle_06::RawWindowHandle,
}

impl raw_window_handle_06::HasDisplayHandle for SbHandle {
    fn display_handle(&self) -> Result<raw_window_handle_06::DisplayHandle<'_>, raw_window_handle_06::HandleError> {
        Ok(unsafe { raw_window_handle_06::DisplayHandle::borrow_raw(self.display) })
    }
}

impl raw_window_handle_06::HasWindowHandle for SbHandle {
    fn window_handle(&self) -> Result<raw_window_handle_06::WindowHandle<'_>, raw_window_handle_06::HandleError> {
        Ok(unsafe { raw_window_handle_06::WindowHandle::borrow_raw(self.window) })
    }
}

fn sb_handle(window: &Window<'_>) -> SbHandle {
    let window_handle = match window.raw_window_handle() {
        RawWindowHandle::AppKit(h) => raw_window_handle_06::RawWindowHandle::AppKit(raw_window_handle_06::AppKitWindowHandle::new(
            NonNull::new(h.ns_view).expect("ns_view"),
        )),
        RawWindowHandle::Win32(h) => {
            let mut w = raw_window_handle_06::Win32WindowHandle::new(std::num::NonZeroIsize::new(h.hwnd as isize).expect("hwnd"));
            w.hinstance = std::num::NonZeroIsize::new(h.hinstance as isize);
            raw_window_handle_06::RawWindowHandle::Win32(w)
        }
        _ => panic!("unsupported window system"),
    };
    let display = match window_handle {
        raw_window_handle_06::RawWindowHandle::AppKit(_) => {
            raw_window_handle_06::RawDisplayHandle::AppKit(raw_window_handle_06::AppKitDisplayHandle::new())
        }
        _ => raw_window_handle_06::RawDisplayHandle::Windows(raw_window_handle_06::WindowsDisplayHandle::new()),
    };
    SbHandle { display, window: window_handle }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_scale_follows_actual_view_during_pending_zoom() {
        // A 115% request does not change where controls are drawn in a view still at 100%.
        let old = size_for_zoom(100);
        assert_eq!(view_scale(old.0 as f64), 1.0);
        let requested = size_for_zoom(115);
        assert!((view_scale(requested.0 as f64) - 1.15).abs() < 0.001);
        assert_eq!(zoom_for_size((old.0 as f64, old.1 as f64)), Some(100));
    }

    #[test]
    fn resize_waits_for_async_host_before_reverting() {
        let started = Instant::now();
        let request = ResizeRequest { target: size_for_zoom(115), previous_zoom: 100, started };
        let old = (layout::W as f64, layout::H as f64);
        assert_eq!(request.outcome(old, old, started + Duration::from_millis(100)), ResizeOutcome::Pending);
        let target = (request.target.0 as f64, request.target.1 as f64);
        assert_eq!(request.outcome(target, target, started + Duration::from_millis(300)), ResizeOutcome::Applied);
        assert_eq!(request.outcome(old, old, started + RESIZE_TIMEOUT), ResizeOutcome::TimedOut);
    }

    #[test]
    fn larger_child_clipped_by_fixed_host_does_not_confirm_zoom() {
        let started = Instant::now();
        let request = ResizeRequest { target: size_for_zoom(115), previous_zoom: 100, started };
        let child = (request.target.0 as f64, request.target.1 as f64);
        let visible = (layout::W as f64, layout::H as f64);
        assert_eq!(request.outcome(child, visible, started + Duration::from_millis(100)), ResizeOutcome::Pending);
        assert_eq!(request.outcome(child, visible, started + RESIZE_TIMEOUT), ResizeOutcome::TimedOut);
    }
}
