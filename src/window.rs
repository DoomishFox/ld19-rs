use pixels::{Pixels, SurfaceTexture};
use raqote::{DrawOptions, DrawTarget, PathBuilder, SolidSource, Source};
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, KeyEvent, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct DrawPoint {
    pub x: f32,
    pub y: f32,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum UserEvent {
    DrawPointBuffer(Vec<DrawPoint>),
}

#[derive(Default)]
pub struct State<'win> {
    // Use an `Option` to allow the window to not be available until the
    // application is properly running.
    pub size: PhysicalSize<f64>,
    pub window: Option<Arc<Window>>,
    pub framebuffer: Option<Pixels<'win>>,
    pub surface: Option<Surface>,
}

impl State<'_> {
    pub fn with_size(size: PhysicalSize<f64>) -> Self {
        Self {
            size: size,
            window: None,
            framebuffer: None,
            surface: None,
        }
    }
}

impl ApplicationHandler<UserEvent> for State<'_> {
    // This is a common indicator that you can create a window.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_size = self.size;
        let attributes = WindowAttributes::default()
            .with_resizable(false)
            .with_title("lidar")
            .with_inner_size(window_size);
        let window = Arc::new(event_loop.create_window(attributes).unwrap());
        let surface_texture = SurfaceTexture::new(
            window_size.width as u32,
            window_size.height as u32,
            window.clone(),
        );
        let pixels = Pixels::new(
            self.size.width as u32,
            self.size.height as u32,
            surface_texture,
        );
        self.framebuffer = Some(pixels.expect("Error initializing framebuffer!"));
        self.window = Some(window)
    }
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // `unwrap` is fine, the window will always be available when
        // receiving a window event.
        //let window = self.window.as_ref().unwrap();
        // Handle window event.
        match event {
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                // Redraw the application.
                //
                // It's preferable for applications that do not render continuously to render in
                // this event rather than in AboutToWait, since rendering in here allows
                // the program to gracefully handle redraws requested by the OS.

                //self.surface.as_mut().unwrap().draw(vec![]);

                // Draw.
                for (dst, &src) in self
                    .framebuffer
                    .as_mut()
                    .unwrap()
                    .frame_mut()
                    .chunks_exact_mut(4)
                    .zip(self.surface.as_ref().unwrap().frame().iter())
                {
                    dst[0] = (src >> 16) as u8;
                    dst[1] = (src >> 8) as u8;
                    dst[2] = src as u8;
                    dst[3] = (src >> 24) as u8;
                }

                if let Err(err) = self.framebuffer.as_ref().unwrap().render() {
                    println!("[render] pixels.render, {err}");
                    event_loop.exit();
                    return;
                }

                // Queue a RedrawRequested event.
                //
                // You only need to call this if you've determined that you need to redraw in
                // applications which do not always need to. Applications that redraw continuously
                // can render here instead.
                //self.window.as_ref().unwrap().request_redraw();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::KeyR),
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => {
                self.surface.as_mut().unwrap().init();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::Equal),
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => {
                self.surface.as_mut().unwrap().init();
                self.surface.as_mut().unwrap().draw_scale -= 1.0;
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::Minus),
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => {
                self.surface.as_mut().unwrap().init();
                self.surface.as_mut().unwrap().draw_scale += 1.0;
            }
            _ => (),
        }
    }
    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        _event: DeviceEvent,
    ) {
        // Handle window event.
    }
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        // Handle user event
        match event {
            UserEvent::DrawPointBuffer(buffer) => {
                //println!("recv draw event: {buffer:?}");
                self.surface.as_mut().unwrap().draw(buffer);
            } //_ => (),
        }
    }
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

#[allow(dead_code)]
pub struct Surface {
    // The main draw target
    dt: Option<DrawTarget>,
    draw_scale: f32,

    // Center
    cx: f32,
    cy: f32,

    // Background color
    r: f32,
    g: f32,
    b: f32,
}

impl Surface {
    /// Create a new Shapes
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            dt: Some(DrawTarget::new(width as i32, height as i32)),
            draw_scale: 1.0,
            cx: (width / 2) as f32,
            cy: (width / 2) as f32,
            r: 0.0,
            g: 0.0,
            b: 0.0,
        }
    }

    /// Gain access to the underlying pixels
    pub fn frame(&self) -> &[u32] {
        self.dt.as_ref().unwrap().get_data()
    }

    pub fn init(&mut self) {
        let dt = self.dt.as_mut().unwrap();

        dt.clear(SolidSource {
            r: self.r as u8,
            g: self.g as u8,
            b: self.b as u8,
            a: 0xff,
        });
    }

    /// Draw all of the shapes
    pub fn draw(&mut self, command_buffer: Vec<DrawPoint>) {
        let dt = self.dt.as_mut().unwrap();

        for command in command_buffer {
            let mut path = PathBuilder::new();
            //path.rect(command.x + self.cx, command.y + self.cy, 1.0, 1.0);
            path.rect(
                (command.x / self.draw_scale) + self.cx,
                (command.y / self.draw_scale) + self.cy,
                1.0,
                1.0,
            );
            let path = path.finish();
            dt.fill(
                &path,
                &Source::Solid(SolidSource {
                    r: command.r,
                    g: command.g,
                    b: command.b,
                    a: 0xff,
                }),
                &DrawOptions::new(),
            );
        }
    }
}
