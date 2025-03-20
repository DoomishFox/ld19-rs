use std::collections::HashMap;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use futures::channel::mpsc::{self, Receiver, Sender};
use futures::stream::StreamExt;
use ld19::decoder::Packet;
use pixels::{Pixels, SurfaceTexture};
use raqote::{DrawOptions, DrawTarget, PathBuilder, SolidSource, Source, StrokeStyle,};
use tokio::runtime::Runtime;
use tokio_serial::{SerialPort, SerialPortBuilderExt};
use tokio_util::codec::Decoder;

mod ld19;

const WIDTH: u32 = 800;
const HEIGHT: u32 = 800;

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{WindowEvent, DeviceEvent, DeviceId};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowAttributes, WindowId};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
struct DrawPoint {
    x: f32,
    y: f32,
    r: u8,
    g: u8,
    b: u8,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
enum LidarEvent {
    DrawPointBuffer(Vec<DrawPoint>),
}

#[derive(Default)]
struct State<'win> {
    // Use an `Option` to allow the window to not be available until the
    // application is properly running.
    window: Option<Arc<Window>>,
    framebuffer: Option<Pixels<'win>>,
    surface: Option<Surface>,
}

impl ApplicationHandler<LidarEvent> for State<'_> {
    // This is a common indicator that you can create a window.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_size = PhysicalSize::new(WIDTH as f64, HEIGHT as f64);
        let attributes = WindowAttributes::default()
            .with_resizable(false)
            .with_title("lidar")
            .with_inner_size(window_size);
        let window = Arc::new(event_loop.create_window(attributes).unwrap());
        let surface_texture = SurfaceTexture::new(window_size.width as u32, window_size.height as u32, window.clone());
        let pixels = Pixels::new(WIDTH, HEIGHT, surface_texture);
        self.framebuffer = Some(pixels.expect("Error initializing framebuffer!"));
        self.window = Some(window)
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        // `unwrap` is fine, the window will always be available when
        // receiving a window event.
        //let window = self.window.as_ref().unwrap();
        // Handle window event.
        match event {
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
            },
            WindowEvent::RedrawRequested => {
                // Redraw the application.
                //
                // It's preferable for applications that do not render continuously to render in
                // this event rather than in AboutToWait, since rendering in here allows
                // the program to gracefully handle redraws requested by the OS.

                //self.surface.as_mut().unwrap().draw(vec![]);

                // Draw.
                for (dst, &src) in self.framebuffer.as_mut().unwrap()
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
            _ => (),
        }
    }
    fn device_event(&mut self, event_loop: &ActiveEventLoop, device_id: DeviceId, event: DeviceEvent) {
        // Handle window event.
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: LidarEvent) {
        // Handle user event
        match event {
            LidarEvent::DrawPointBuffer(buffer) => {
                //println!("recv draw event: {buffer:?}");
                self.surface.as_mut().unwrap().draw(buffer);
            },
            _ => (),
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

struct Surface {
    // The main draw target
    dt: Option<DrawTarget>,

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
            path.rect(command.x, command.y, 2.0, 2.0);
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

#[derive(Debug)]
struct Point {
    angle: f32,
    distance: u32,
    confidence: u8,
}

fn parse(packet: Packet) -> Vec<Point> {
    let start_angle = packet.start_angle as f32 / 100.0;
    let end_angle = packet.end_angle as f32 / 100.0;
    // this does *something*, i think it has to do with angle rollovers? its in the
    // c++ lib so im including it
    let diff = (end_angle + 36000.0 - start_angle) % 36000.0;
    let step = diff / packet.data.len() as f32;
    let points = packet
        .data
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let angle = start_angle + (step * i as f32);
            Point {
                angle: if angle > 360.0 { angle - 360.0 } else { angle },
                distance: d.distance as u32,
                confidence: d.intensity,
            }
        })
        .collect();
    points
}

fn polar_to_cartesian(distance: u32, theta: f32) -> (f32, f32) {
    (
        distance as f32 * f32::cos(theta.to_radians()),
        distance as f32 * f32::sin(theta.to_radians()),
    )
}

#[tokio::main]
async fn main() {
    //env_logger::init();   

    let event_loop = EventLoop::<LidarEvent>::with_user_event().build().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut state = State::default();
    state.surface = Some(Surface::new(WIDTH, HEIGHT));
    
    let proxy = event_loop.create_proxy();
    let _receive_thread_handle = thread::Builder::new()
        .name(String::from("lidar"))
        .spawn(move || write_to_surface(proxy))
        .expect("[lidar] listen thread failed!");

    let _runtime = event_loop.run_app(&mut state);
    /*
    println!("setting up drawing surface...");
    let mut dt = DrawTarget::new(800, 800);
    dt.clear(SolidSource::from_unpremultiplied_argb(
        0xff, 0x00, 0x00, 0x00,
    ));

    println!("starting serial read...");
    let serial_builder = tokio_serial::new("/dev/serial0", 230_400)
        .data_bits(tokio_serial::DataBits::Eight)
        .stop_bits(tokio_serial::StopBits::One)
        .parity(tokio_serial::Parity::None)
        .flow_control(tokio_serial::FlowControl::None);
    let mut serial = serial_builder
        .open_native_async()
        .expect("Failed to open port");

    serial
        .set_exclusive(false)
        .expect("unable to set serial port exclusive to false");
    serial
        .read_data_set_ready()
        .expect("unable to set serial port read data ready");

    let mut i = 1000;
    let mut reader = ld19::decoder::LidarCodec.framed(serial);
    while let Some(packet) = reader.next().await {
        let points = parse(packet.expect("bad packet!"));
        //println!("received data: {:?}", points);

        for point in points.iter() {
            let (x, y) = polar_to_cartesian(point.distance / 10, point.angle);
            let mut path = PathBuilder::new();
            path.rect(x + 400.0, y + 400.0, 1.0, 1.0);
            let path = path.finish();
            let confidence = point.confidence as f32 / 200.0;
            let green = (255.0 * confidence) as u8;
            let red = 255 - green;
            dt.fill(
                &path,
                &Source::Solid(SolidSource {
                    r: red,
                    g: green,
                    b: 0x00,
                    a: 0xff,
                }),
                &DrawOptions::new(),
            );
            //println!("drawing at: {}, {}", x, y);
        }
        i = i - 1;
        if i == 0 {
            break;
        }
    };
    dt.write_png("lidar.png").expect("cant write output!");
    */
}

fn write_to_surface_dummy(event_loop: EventLoopProxy<LidarEvent>) {
    //println!("running dummy thread!");
    let mut counter = 0;
    let mut yc = 0;

    loop {
        if counter >= 800 {
            counter = 0;
            yc += 1;
        }

        let draw_point = DrawPoint {
            x: counter as f32,
            y: yc as f32,
            r: 0xFF,
            g: 0x00,
            b: 0x00,
        };

        counter += 1;

        //println!("sending event!");
        let _ = event_loop.send_event(LidarEvent::DrawPointBuffer(vec![draw_point]));
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

fn write_to_surface(event_loop: EventLoopProxy<LidarEvent>) {
    let rt = Runtime::new().expect("uh oh");

    // Spawn the root task
    rt.block_on(async {
        println!("starting serial read...");
        let serial_builder = tokio_serial::new("/dev/tty.usbserial-A904CUY3", 230_400)
            .data_bits(tokio_serial::DataBits::Eight)
            .stop_bits(tokio_serial::StopBits::One)
            .parity(tokio_serial::Parity::None)
            .flow_control(tokio_serial::FlowControl::None);
        let mut serial = serial_builder
            .open_native_async()
            .expect("Failed to open port");

        serial
            .set_exclusive(false)
            .expect("unable to set serial port exclusive to false");
        serial
            .read_data_set_ready()
            .expect("unable to set serial port read data ready");

        let mut reader = ld19::decoder::LidarCodec.framed(serial);
        println!("beginning await for sensor data...");
        while let Some(packet) = reader.next().await {
            let points = parse(packet.expect("bad packet!"));
            //println!("received data: {:?}", points);

            let draw_points: Vec<DrawPoint> = points.iter().map(|p| {
                let (x, y) = polar_to_cartesian(p.distance / 20, p.angle);
                let confidence = p.confidence as f32 / 200.0;
                let green = (255.0 * confidence) as u8;
                let red = 255 - green;
                //println!("drawing at: {}, {}", x, y);
                DrawPoint {
                    x: x + (WIDTH / 2) as f32,
                    y: y + (HEIGHT / 2) as f32,
                    r: red,
                    g: green,
                    b: 0x00,
                }
            }).collect();

            // write to buffer/send event
            //print!(".");
            //println!("[debug] {draw_points:?}");
            let _ = event_loop.send_event(LidarEvent::DrawPointBuffer(draw_points));
        }
        //dt.write_png("lidar.png").expect("cant write output!");
    })
}