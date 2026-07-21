//! The mechanism: window ownership, the frame loop, and the services a host may use.
//!
//! **The borrow shape** (ui-01 §13.5 flagged the sketch as uncompilable): `Mech`
//! cannot both own the host and be handed to it. The resolution is a private
//! [`Driver`] owning `mech` and `host` as sibling fields — disjoint field borrows let
//! a callback take `&mut self.host` and `&mut self.mech` at once, which is the thing
//! the original sketch could not express.
//!
//! Window creation needs winit's `ActiveEventLoop`, which exists only inside a
//! callback. So [`Mech::open_window`] allocates the id, queues a request, and returns
//! immediately; the driver drains the queue after every callback. The host's API
//! stays synchronous and the event loop stays the only thing holding platform state.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{CursorGrabMode, WindowAttributes, WindowLevel};

use crate::error::MechError;
use crate::fill::PaintStat;
use crate::geometry::{Point, Rect, Size};
use crate::host::{Frame, Host, Notice};
use crate::input::{CursorShape, Modifier, Reason, TargetId, UiTime};
use crate::paint::Painter;
use crate::text::{FontStack, Shaped, TextStyle};
use crate::window::{Window, WindowId, WindowRole, WindowSpec};

mod route;

/// Work the host asked for that needs the event loop to carry out.
enum Request {
    Open(WindowId, WindowSpec),
}

/// Services offered to the host: open and close windows, mark them dirty, quit.
///
/// Holds every window, keyed. There is no "the window" anywhere in this crate.
pub struct Mech {
    window: BTreeMap<WindowId, Window>,
    /// winit's id is an opaque platform value; ours is stable and ordered. The map
    /// translates inbound events into our vocabulary at the boundary.
    by_winit: HashMap<winit::window::WindowId, WindowId>,
    /// Created lazily from the first window, then shared by every surface.
    context: Option<softbuffer::Context<Arc<winit::window::Window>>>,
    request: Vec<Request>,
    next_id: u32,
    exiting: bool,
    /// The font stack. System fallback is on for the application; tests build
    /// their own with it off, so golden masters cannot depend on what is installed.
    text: FontStack,
    /// Workspace-level interface scale (R-938); windows may override it.
    user_scale: f32,
    /// The target a press bound, and the window it belongs to. Every subsequent
    /// move and the release route here regardless of where the pointer goes —
    /// implicit capture, made structural.
    capture: Option<(WindowId, Option<TargetId>)>,
    /// The target the pointer is currently over, for enter/leave and wheel routing.
    hover: Option<(WindowId, Option<TargetId>)>,
    /// Who owns keyboard focus. Moves only on request, never as a side effect of
    /// activation (R-907).
    focus: Option<(WindowId, TargetId)>,
    /// Set while a relative drag is running: the window, and where to put the
    /// pointer back when it ends.
    relative: Option<(WindowId, winit::dpi::PhysicalPosition<f64>)>,
    pointer_at: Point,
    modifier: Modifier,
    cursor: CursorShape,
    started_at: Instant,
    /// What the last frame's shadow path cost. Reset per frame.
    stat: PaintStat,
    /// When to wake next, for time-driven appearance. The loop otherwise sleeps
    /// until input arrives, which is right for everything except animation.
    wake: Option<Instant>,
}

impl Mech {
    fn new() -> Mech {
        Mech {
            window: BTreeMap::new(),
            by_winit: HashMap::new(),
            context: None,
            request: Vec::new(),
            next_id: 1,
            exiting: false,
            text: FontStack::new(true),
            user_scale: 1.0,
            capture: None,
            hover: None,
            focus: None,
            relative: None,
            pointer_at: Point::default(),
            modifier: Modifier::default(),
            cursor: CursorShape::Default,
            started_at: Instant::now(),
            stat: PaintStat::default(),
            wake: None,
        }
    }

    /// What the last frame cost in the shadow path — the measurement that decides
    /// whether a shadow cache is worth building.
    pub fn paint_stat(&self) -> &PaintStat {
        &self.stat
    }

    /// Ask to be woken in `seconds`, once. Frames still happen only when something
    /// is dirty; this only ensures the loop is awake to notice.
    ///
    /// A control that flashes must ask for this while it flashes and stop asking
    /// when it stops — an application that always asks has a busy loop with extra
    /// steps.
    pub fn wake_after(&mut self, seconds: f64) {
        let at = Instant::now() + std::time::Duration::from_secs_f64(seconds.max(0.0));
        self.wake = Some(match self.wake {
            Some(existing) => existing.min(at),
            None => at,
        });
    }

    /// Seconds since start — the UI clock, for animation. Never the engine clock.
    pub fn now(&self) -> UiTime {
        UiTime(self.started_at.elapsed().as_secs_f64())
    }

    /// Shape and measure outside a frame — what layout asks before it can place
    /// anything. Inside a frame, use the painter's `shape`.
    pub fn shape(&mut self, text: &str, style: &TextStyle) -> Shaped {
        self.text.shape(text, style)
    }

    /// Open a window. The id is valid immediately; the platform window appears when
    /// the loop next drains requests, before any further host callback.
    pub fn open_window(&mut self, spec: WindowSpec) -> WindowId {
        let id = WindowId(self.next_id);
        self.next_id += 1;
        self.request.push(Request::Open(id, spec));
        id
    }

    /// Close a window. Closing the last one does not quit — that is policy, and
    /// policy belongs to the host.
    pub fn close_window(&mut self, id: WindowId) {
        if let Some(w) = self.window.remove(&id) {
            self.by_winit.remove(&w.winit.id());
        }
    }

    /// Ask the loop to exit after the current callback.
    pub fn exit(&mut self) {
        self.exiting = true;
    }

    /// Every open window, in creation order.
    pub fn window_id(&self) -> Vec<WindowId> {
        self.window.keys().copied().collect()
    }

    /// Physical pixels per logical pixel for this window, right now. Read it; never
    /// store it (ui-01 invariant 4).
    pub fn scale_factor(&self, id: WindowId) -> Option<f32> {
        self.window.get(&id).map(|w| w.scale)
    }

    /// Current logical size, or `None` if the window is gone.
    pub fn size(&self, id: WindowId) -> Option<Size> {
        let default = self.user_scale;
        self.window.get(&id).map(|w| w.logical(default))
    }

    /// The workspace interface scale (R-938). Composes with platform DPI rather
    /// than replacing it, so a window moved between displays still tracks its
    /// monitor.
    pub fn ui_scale(&self) -> f32 {
        self.user_scale
    }

    /// Set the workspace scale. Everything visible changes size, so every window
    /// becomes dirty.
    pub fn set_ui_scale(&mut self, scale: f32) {
        self.user_scale = scale.clamp(0.5, 4.0);
        for id in self.window_id() {
            self.mark_dirty_all(id);
        }
    }

    /// Override the scale for one window; `None` restores the workspace default.
    /// Per-window because a palette on a laptop panel and a main window on a large
    /// display want different answers.
    ///
    /// Applies to a window that exists. A window's *initial* scale belongs in its
    /// [`WindowSpec`] — calling this on an id whose window is still queued would
    /// silently do nothing.
    pub fn set_window_scale(&mut self, id: WindowId, scale: Option<f32>) {
        if let Some(w) = self.window.get_mut(&id) {
            w.user_scale = scale.map(|s| s.clamp(0.5, 4.0));
        }
        self.mark_dirty_all(id);
    }

    /// Who holds keyboard focus.
    pub fn focus(&self) -> Option<(WindowId, TargetId)> {
        self.focus
    }

    /// Move focus. Every transfer states why, so "nothing steals focus" (R-907) is
    /// an auditable property rather than a promise.
    pub fn set_focus(&mut self, to: Option<(WindowId, TargetId)>, _why: Reason) {
        self.focus = to;
    }

    /// Request a cursor shape for this frame.
    pub fn request_cursor(&mut self, shape: CursorShape) {
        self.cursor = shape;
    }

    /// Begin a relative drag: the pointer is hidden and locked in place, motion
    /// arrives as `PointerKind::Delta`, and the pointer is restored on `end`.
    ///
    /// **This is the only sanctioned exception to the hands-off clause** (R-907),
    /// and it exists here so no widget rolls its own pointer warping. Infinite knob
    /// drag is what it is for.
    pub fn begin_relative_drag(&mut self, id: WindowId) {
        let Some(w) = self.window.get(&id) else {
            return;
        };
        let origin = w
            .winit
            .inner_position()
            .ok()
            .map(|_| {
                winit::dpi::PhysicalPosition::new(
                    f64::from(self.pointer_at.x) * f64::from(w.scale),
                    f64::from(self.pointer_at.y) * f64::from(w.scale),
                )
            })
            .unwrap_or(winit::dpi::PhysicalPosition::new(0.0, 0.0));
        // Locked grab where the platform offers it; confined is the fallback, and
        // failing both simply means the drag is bounded rather than infinite.
        let _ = w
            .winit
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| w.winit.set_cursor_grab(CursorGrabMode::Confined));
        w.winit.set_cursor_visible(false);
        self.relative = Some((id, origin));
    }

    /// End a relative drag, restoring the pointer where it started.
    pub fn end_relative_drag(&mut self) {
        let Some((id, origin)) = self.relative.take() else {
            return;
        };
        if let Some(w) = self.window.get(&id) {
            let _ = w.winit.set_cursor_grab(CursorGrabMode::None);
            let _ = w.winit.set_cursor_position(origin);
            w.winit.set_cursor_visible(true);
        }
    }

    /// True while a relative drag is running.
    pub fn dragging(&self) -> bool {
        self.relative.is_some()
    }

    /// Mark part of a window as needing a frame. Painting happens on the loop's
    /// schedule, never synchronously — and a window nobody marks is never
    /// repainted, which is the property step 4 exists to guarantee.
    pub fn mark_dirty(&mut self, id: WindowId, area: Rect) {
        if let Some(w) = self.window.get_mut(&id) {
            w.dirty.add(area);
            if !w.dirty.empty() {
                w.winit.request_redraw();
            }
        }
    }

    /// Mark a whole window. Resizes and scale changes use this; widgets should not.
    pub fn mark_dirty_all(&mut self, id: WindowId) {
        let default = self.user_scale;
        if let Some(w) = self.window.get_mut(&id) {
            let all = w.bounds(default);
            w.dirty.add(all);
            w.winit.request_redraw();
        }
    }

    /// Carry out queued requests. Called by the driver, which has the event loop.
    fn drain(&mut self, el: &ActiveEventLoop) -> Result<(), MechError> {
        for req in std::mem::take(&mut self.request) {
            match req {
                Request::Open(id, spec) => self.create(el, id, spec)?,
            }
        }
        Ok(())
    }

    fn create(
        &mut self,
        el: &ActiveEventLoop,
        id: WindowId,
        spec: WindowSpec,
    ) -> Result<(), MechError> {
        let size = winit::dpi::LogicalSize::new(spec.size.w, spec.size.h);
        let mut attr = WindowAttributes::default()
            .with_title(spec.title)
            .with_inner_size(size)
            .with_resizable(spec.resizable);
        if spec.role == WindowRole::Palette {
            // A first approximation of palette behavior: floats, and does not want
            // to be treated as a document. Owner-window and no-activation semantics
            // are platform work, deferred with the rest of the palette story.
            attr = attr.with_window_level(WindowLevel::AlwaysOnTop);
        }
        let win = Arc::new(el.create_window(attr)?);

        // One softbuffer context for the process, created from the first window's
        // display; every surface afterwards shares it.
        if self.context.is_none() {
            self.context = Some(softbuffer::Context::new(win.clone())?);
        }
        let context = self.context.as_ref().expect("context created above");
        let surface = softbuffer::Surface::new(context, win.clone())?;

        let physical = win.inner_size();
        let mut window = Window {
            scale: win.scale_factor() as f32,
            physical: (physical.width, physical.height),
            winit: win.clone(),
            surface,
            user_scale: spec.scale,
            dirty: crate::dirty::Dirty::default(),
            pixmap: None,
        };
        window.resize_surface()?;
        let all = window.bounds(self.user_scale);
        window.dirty.add(all);

        self.by_winit.insert(win.id(), id);
        self.window.insert(id, window);
        win.request_redraw();
        Ok(())
    }
}

/// Owns the mechanism and the host as siblings, so callbacks can borrow both.
struct Driver<H: Host> {
    mech: Mech,
    host: H,
    started: bool,
    error: Option<MechError>,
}

impl<H: Host> Driver<H> {
    /// Run queued requests and honour an exit, after every callback.
    fn settle(&mut self, el: &ActiveEventLoop) {
        if self.error.is_none()
            && let Err(e) = self.mech.drain(el)
        {
            self.error = Some(e);
            self.mech.exiting = true;
        }
        if self.mech.exiting {
            el.exit();
        }
    }
}

impl<H: Host> ApplicationHandler for Driver<H> {
    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        self.host.tick(&mut self.mech);
        // Sleep until input, unless something asked to be woken sooner.
        match self.mech.wake.take() {
            Some(at) => el.set_control_flow(ControlFlow::WaitUntil(at)),
            None => el.set_control_flow(ControlFlow::Wait),
        }
    }

    fn resumed(&mut self, el: &ActiveEventLoop) {
        // `resumed` can fire more than once on mobile-shaped platforms; `start` is a
        // once-per-run event by contract.
        if !self.started {
            self.started = true;
            self.host.start(&mut self.mech);
        }
        self.settle(el);
    }

    fn device_event(&mut self, el: &ActiveEventLoop, _device: DeviceId, ev: DeviceEvent) {
        // Raw motion is the only honest source for infinite drag: it keeps
        // reporting after the pointer would have run into a screen edge.
        if let DeviceEvent::MouseMotion { delta } = ev
            && let Some((window, _)) = self.mech.relative
        {
            self.pointer_delta(window, delta.0 as f32, delta.1 as f32);
        }
        self.settle(el);
    }

    fn window_event(
        &mut self,
        el: &ActiveEventLoop,
        winit_id: winit::window::WindowId,
        ev: WindowEvent,
    ) {
        let Some(id) = self.mech.by_winit.get(&winit_id).copied() else {
            return; // event for a window we already closed
        };

        // Translate the platform event into our vocabulary, updating per-window
        // state first so the host always observes a consistent Mech.
        let notice = match ev {
            WindowEvent::CloseRequested => Some(Notice::CloseRequested),
            WindowEvent::Focused(f) => Some(Notice::FocusChanged(f)),
            WindowEvent::Resized(size) => {
                let default = self.mech.user_scale;
                if let Some(w) = self.mech.window.get_mut(&id) {
                    w.physical = (size.width, size.height);
                    let all = w.bounds(default);
                    w.dirty.add(all);
                    if let Err(e) = w.resize_surface() {
                        self.error = Some(e);
                        self.mech.exiting = true;
                    }
                }
                self.mech.size(id).map(Notice::Resized)
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let default = self.mech.user_scale;
                if let Some(w) = self.mech.window.get_mut(&id) {
                    w.scale = scale_factor as f32;
                    let all = w.bounds(default);
                    w.dirty.add(all);
                }
                Some(Notice::ScaleChanged(scale_factor as f32))
            }
            WindowEvent::RedrawRequested => {
                self.paint(id);
                None
            }
            WindowEvent::ModifiersChanged(m) => {
                let state = m.state();
                self.mech.modifier = Modifier {
                    shift: state.shift_key(),
                    ctrl: state.control_key(),
                    alt: state.alt_key(),
                    meta: state.super_key(),
                };
                None
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer_moved(id, position);
                None
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer_left();
                None
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.pointer_button(id, state, button);
                None
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.pointer_wheel(id, delta);
                None
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                // Synthetic events are the platform's bookkeeping for keys that
                // were already held when focus moved — not the user pressing
                // anything. Delivering them makes any key that opens or closes a
                // window immediately undo itself, because the new window's focus
                // change resynthesizes the press that opened it.
                if !is_synthetic {
                    self.key(id, &event);
                }
                None
            }
            _ => None,
        };

        if let Some(notice) = notice {
            self.host.notice(id, &notice, &mut self.mech);
        }
        // After dispatch, not after painting: a widget may request a cursor without
        // invalidating anything, and tying the request to a repaint would drop it.
        self.apply_cursor(id);
        self.settle(el);
    }
}

impl<H: Host> Driver<H> {
    /// Present one frame for a window, if it is dirty and has pixels.
    fn paint(&mut self, id: WindowId) {
        // Split the borrow explicitly: the host and the window map are siblings.
        let Driver { mech, host, .. } = self;
        // Field-split again: the window map and the font stack are siblings inside
        // Mech, and a frame needs both.
        let Mech {
            window,
            text,
            stat,
            user_scale,
            ..
        } = mech;
        stat.clear();
        let user_scale = *user_scale;
        let Some(window) = window.get_mut(&id) else {
            return;
        };
        if window.dirty.empty() {
            return; // nothing marked: no frame, no work — the whole point of step 4
        }
        let physical = window.physical;
        if physical.0 == 0 || physical.1 == 0 {
            return; // minimized: nothing to paint into, and not an error
        }
        let scale = window.effective_scale(user_scale);
        let size = window.logical(user_scale);
        let Some(pixmap) = window.pixmap.as_mut() else {
            return; // no paint buffer yet; the next resize allocates one
        };
        // Painting is clipped to the region's bounds, so a small mark costs a small
        // rasterization. The pixmap persists between frames, which is what makes
        // partial painting correct: everything outside the region is last frame's
        // pixels, still valid.
        let dirty = window.dirty.clone();
        let mut frame = Frame {
            size,
            scale,
            paint: Painter::new(pixmap, text, stat, scale, dirty.bound()),
            dirty: &dirty,
        };
        host.paint(id, &mut frame);

        let mut buffer = match window.surface.buffer_mut() {
            Ok(b) => b,
            Err(e) => {
                self.error = Some(e.into());
                self.mech.exiting = true;
                return;
            }
        };
        blit(pixmap, &mut buffer);
        // Presenting consumes the buffer; clearing dirty after it means a failed
        // present leaves the window dirty and it will be retried.
        if let Err(e) = buffer.present() {
            self.error = Some(e.into());
            self.mech.exiting = true;
            return;
        }
        if let Some(window) = self.mech.window.get_mut(&id) {
            window.dirty.clear();
        }
    }
}

impl<H: Host> Driver<H> {
    /// Apply the shape requested while handling an event, then reset. Requesting
    /// per event means a widget that stops asking gets the default back without
    /// having to remember to say so.
    fn apply_cursor(&mut self, id: WindowId) {
        let Some(w) = self.mech.window.get(&id) else {
            return;
        };
        // A relative drag owns the cursor; nothing else may fight it for control.
        if self.mech.relative.is_some() {
            return;
        }
        match self.mech.cursor {
            CursorShape::None => w.winit.set_cursor_visible(false),
            shape => {
                w.winit.set_cursor_visible(true);
                w.winit.set_cursor(winit_cursor(shape));
            }
        }
        self.mech.cursor = CursorShape::Default;
    }
}

fn winit_cursor(shape: CursorShape) -> winit::window::Cursor {
    use winit::window::CursorIcon;
    let icon = match shape {
        CursorShape::Default | CursorShape::None => CursorIcon::Default,
        CursorShape::Text => CursorIcon::Text,
        CursorShape::Hand => CursorIcon::Pointer,
        CursorShape::ResizeHorizontal => CursorIcon::EwResize,
        CursorShape::ResizeVertical => CursorIcon::NsResize,
        CursorShape::Crosshair => CursorIcon::Crosshair,
    };
    icon.into()
}

/// Copy the paint buffer into the window's back buffer.
///
/// tiny-skia stores **premultiplied RGBA8**; softbuffer wants `0RGB` in a `u32`.
/// The window is opaque, so alpha is dropped rather than unpremultiplied — anything
/// the host painted has already been composited against the frame it cleared, and a
/// translucent window is a platform feature we do not offer. If that ever changes,
/// this is the one function that has to learn about it.
fn blit(pixmap: &tiny_skia::Pixmap, out: &mut [u32]) {
    for (px, slot) in pixmap.data().chunks_exact(4).zip(out.iter_mut()) {
        *slot = (u32::from(px[0]) << 16) | (u32::from(px[1]) << 8) | u32::from(px[2]);
    }
}

/// Run the mechanism until the host exits. Returns the first fatal platform error.
///
/// The UI is single-threaded on the main thread and this call owns it; cross-thread
/// input arrives only through rings (ui-01 §7.7).
pub fn run<H: Host>(host: H) -> Result<(), MechError> {
    let el = EventLoop::new()?;
    // Wait rather than poll: frames happen when something is dirty, never on a
    // timer (ui-01 §13.4 — softbuffer has no vsync to align to).
    el.set_control_flow(ControlFlow::Wait);
    let mut driver = Driver {
        mech: Mech::new(),
        host,
        started: false,
        error: None,
    };
    el.run_app(&mut driver)?;
    match driver.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
