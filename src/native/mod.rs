use crate::{view::View, Platform};
use accesskit::Point;
use gl::types::*;
use glutin::{
    config::{ConfigTemplateBuilder, GlConfig},
    context::{
        ContextApi, ContextAttributesBuilder, NotCurrentGlContextSurfaceAccessor,
        PossiblyCurrentContext,
    },
    display::{GetGlDisplay, GlDisplay},
    prelude::GlSurface,
    surface::{Surface as GlutinSurface, SurfaceAttributesBuilder, WindowSurface},
};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasRawWindowHandle;
use skia_safe::{
    gpu::{self, gl::FramebufferInfo, BackendRenderTarget, SurfaceOrigin},
    Color, Color4f, ColorType, Font, FontStyle, Paint, Surface, TextBlob, Typeface,
};
use slotmap::{DefaultKey, SlotMap};
use std::{
    ffi::CString,
    marker::PhantomData,
    num::NonZeroU32,
    time::{Duration, Instant},
};
use taffy::{
    prelude::Size,
    style::{FlexDirection, Style},
    style_helpers::TaffyMaxContent,
    Taffy,
};
use winit::{
    event::{ElementState, Event as WinitEvent, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::{Window, WindowBuilder},
};

mod canvas;
pub use canvas::{canvas, Canvas};

pub fn text<E>(content: String) -> impl View<Native<E>> {
    canvas(move |layout, canvas| {
        let typeface = Typeface::new("Arial", FontStyle::default()).unwrap();
        let font = Font::new(typeface, Some(100.));
        let blob = TextBlob::new(&content, &font).unwrap();
        canvas.draw_text_blob(
            &blob,
            (layout.location.x + 100., layout.location.y + 100.),
            &Paint::new(Color4f::new(1., 0., 0., 1.), None),
        );
    })
}

pub trait Element {
    fn paint(&mut self, taffy: &Taffy, canvas: &mut skia_safe::Canvas);
}

// Guarantee the drop order inside the FnMut closure. `Window` _must_ be dropped after
// `DirectContext`.
//
// https://github.com/rust-skia/rust-skia/issues/476
pub struct Native<E> {
    surface: Surface,
    gl_surface: GlutinSurface<WindowSurface>,
    gr_context: skia_safe::gpu::DirectContext,
    gl_context: PossiblyCurrentContext,
    window: Window,
    _marker: PhantomData<E>,
    elements: SlotMap<DefaultKey, Box<dyn Element>>,
    stack: Vec<DefaultKey>,
    layout_keys: SlotMap<DefaultKey, DefaultKey>,
    taffy: Taffy,
    layout_stack: Vec<DefaultKey>,
}

impl<E> Platform for Native<E> {
    type Event = E;

    fn advance(&mut self) {
        todo!()
    }
}

#[derive(Clone, Debug)]
pub enum Event {
    MouseMove { pos: Point },
}

pub fn run<T, V, E>(
    mut state: T,
    _update: impl Fn(&mut T, E) + 'static,
    mut make_view: impl FnMut(&T) -> V + 'static,
) where
    T: 'static,
    V: View<Native<E>> + 'static,
    V::State: 'static,
    E: 'static,
{
    let el = EventLoopBuilder::with_user_event().build();

    let winit_window_builder = WindowBuilder::new().with_title("concoct");

    let template = ConfigTemplateBuilder::new()
        .with_alpha_size(8)
        .with_transparency(true);

    let display_builder = DisplayBuilder::new().with_window_builder(Some(winit_window_builder));
    let (window, gl_config) = display_builder
        .build(&el, template, |configs| {
            // Find the config with the minimum number of samples. Usually Skia takes care of
            // anti-aliasing and may not be able to create appropriate Surfaces for samples > 0.
            // See https://github.com/rust-skia/rust-skia/issues/782
            // And https://github.com/rust-skia/rust-skia/issues/764
            configs
                .reduce(|accum, config| {
                    let transparency_check = config.supports_transparency().unwrap_or(false)
                        & !accum.supports_transparency().unwrap_or(false);

                    if transparency_check || config.num_samples() < accum.num_samples() {
                        config
                    } else {
                        accum
                    }
                })
                .unwrap()
        })
        .unwrap();
    println!("Picked a config with {} samples", gl_config.num_samples());
    let mut window = window.expect("Could not create window with OpenGL context");
    let raw_window_handle = window.raw_window_handle();

    // The context creation part. It can be created before surface and that's how
    // it's expected in multithreaded + multiwindow operation mode, since you
    // can send NotCurrentContext, but not Surface.
    let context_attributes = ContextAttributesBuilder::new().build(Some(raw_window_handle));

    // Since glutin by default tries to create OpenGL core context, which may not be
    // present we should try gles.
    let fallback_context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::Gles(None))
        .build(Some(raw_window_handle));
    let not_current_gl_context = unsafe {
        gl_config
            .display()
            .create_context(&gl_config, &context_attributes)
            .unwrap_or_else(|_| {
                gl_config
                    .display()
                    .create_context(&gl_config, &fallback_context_attributes)
                    .expect("failed to create context")
            })
    };

    let (width, height): (u32, u32) = window.inner_size().into();

    let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        raw_window_handle,
        NonZeroU32::new(width).unwrap(),
        NonZeroU32::new(height).unwrap(),
    );

    let gl_surface = unsafe {
        gl_config
            .display()
            .create_window_surface(&gl_config, &attrs)
            .expect("Could not create gl window surface")
    };

    let gl_context = not_current_gl_context
        .make_current(&gl_surface)
        .expect("Could not make GL context current when setting up skia renderer");

    gl::load_with(|s| {
        gl_config
            .display()
            .get_proc_address(CString::new(s).unwrap().as_c_str())
    });
    let interface = skia_safe::gpu::gl::Interface::new_load_with(|name| {
        if name == "eglGetCurrentDisplay" {
            return std::ptr::null();
        }
        gl_config
            .display()
            .get_proc_address(CString::new(name).unwrap().as_c_str())
    })
    .expect("Could not create interface");

    let mut gr_context = skia_safe::gpu::DirectContext::new_gl(Some(interface), None)
        .expect("Could not create direct context");

    let fb_info = {
        let mut fboid: GLint = 0;
        unsafe { gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fboid) };

        FramebufferInfo {
            fboid: fboid.try_into().unwrap(),
            format: skia_safe::gpu::gl::Format::RGBA8.into(),
            ..Default::default()
        }
    };

    window.set_inner_size(winit::dpi::Size::new(winit::dpi::LogicalSize::new(
        1024.0, 1024.0,
    )));

    fn create_surface(
        window: &mut Window,
        fb_info: FramebufferInfo,
        gr_context: &mut skia_safe::gpu::DirectContext,
        num_samples: usize,
        stencil_size: usize,
    ) -> Surface {
        let size = window.inner_size();
        let size = (
            size.width.try_into().expect("Could not convert width"),
            size.height.try_into().expect("Could not convert height"),
        );
        let backend_render_target =
            BackendRenderTarget::new_gl(size, num_samples, stencil_size, fb_info);

        gpu::surfaces::wrap_backend_render_target(
            gr_context,
            &backend_render_target,
            SurfaceOrigin::BottomLeft,
            ColorType::RGBA8888,
            None,
            None,
        )
        .expect("Could not create skia surface")
    }
    let num_samples = gl_config.num_samples() as usize;
    let stencil_size = gl_config.stencil_size() as usize;

    let surface = create_surface(
        &mut window,
        fb_info,
        &mut gr_context,
        num_samples,
        stencil_size,
    );

    let mut frame = 0usize;

    let mut env = Native {
        surface,
        gl_surface,
        gl_context,
        gr_context,
        window,
        _marker: PhantomData,
        elements: SlotMap::new(),
        stack: Vec::new(),
        layout_keys: SlotMap::new(),
        taffy: Taffy::new(),
        layout_stack: Vec::new(),
    };
    let mut previous_frame_start = Instant::now();

    let view = make_view(&mut state);
    let mut view_state = view.build(&mut env);

    let mut layout = Style::default();
    layout.size = Size::from_points(1000., 1000.);
    layout.flex_direction = FlexDirection::Row;

    let root = env
        .taffy
        .new_with_children(layout, &env.layout_stack)
        .unwrap();
    taffy::compute_layout(&mut env.taffy, root, Size::MAX_CONTENT).unwrap();

    el.run(move |event, _, control_flow| {
        let frame_start = Instant::now();
        let mut draw_frame = false;

        #[allow(deprecated)]
        match event {
            WinitEvent::UserEvent(()) => {}
            WinitEvent::LoopDestroyed => {}
            WinitEvent::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                WindowEvent::Resized(physical_size) => {
                    env.surface = create_surface(
                        &mut env.window,
                        fb_info,
                        &mut env.gr_context,
                        num_samples,
                        stencil_size,
                    );
                    /* First resize the opengl drawable */
                    let (width, height): (u32, u32) = physical_size.into();

                    env.gl_surface.resize(
                        &env.gl_context,
                        NonZeroU32::new(width.max(1)).unwrap(),
                        NonZeroU32::new(height.max(1)).unwrap(),
                    );
                }
                WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            virtual_keycode,
                            modifiers,
                            ..
                        },
                    ..
                } => {
                    if modifiers.logo() {
                        if let Some(VirtualKeyCode::Q) = virtual_keycode {
                            *control_flow = ControlFlow::Exit;
                        }
                    }
                    frame = frame.saturating_sub(10);
                    env.window.request_redraw();
                }
                WindowEvent::CursorMoved {
                    device_id: _,
                    position: _,
                    modifiers: _,
                } => {}
                WindowEvent::MouseInput {
                    state: element_state,
                    ..
                } => if element_state == ElementState::Pressed {},
                _ => (),
            },
            WinitEvent::RedrawRequested(_) => {
                draw_frame = true;
            }
            _ => (),
        }
        let expected_frame_length_seconds = 1.0 / 20.0;
        let frame_duration = Duration::from_secs_f32(expected_frame_length_seconds);

        if frame_start - previous_frame_start > frame_duration {
            draw_frame = true;
            previous_frame_start = frame_start;
        }
        if draw_frame {
            frame += 1;
            let canvas = env.surface.canvas();
            canvas.clear(Color::WHITE);

            let tree = make_view(&mut state);
            tree.rebuild(&mut env, &mut view_state);

            for key in &env.stack {
                let root = env.elements.get_mut(*key).unwrap();
                root.paint(&env.taffy, env.surface.canvas());
            }

            env.gr_context.flush_and_submit();
            env.gl_surface.swap_buffers(&env.gl_context).unwrap();
        }

        *control_flow = ControlFlow::WaitUntil(previous_frame_start + frame_duration)
    });
}