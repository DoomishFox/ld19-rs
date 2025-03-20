use std::thread;
use futures::stream::StreamExt;
use ld19::decoder::Packet;
use tokio::runtime::Runtime;
use tokio_serial::{SerialPort, SerialPortBuilderExt};
use tokio_util::codec::Decoder;
use winit::{dpi::PhysicalSize, event_loop::{ControlFlow, EventLoop, EventLoopProxy}};

mod ld19;
mod window;
use window::*;

const WIDTH: u32 = 800;
const HEIGHT: u32 = 800;


#[tokio::main]
async fn main() {
    //env_logger::init();   
    let event_loop = EventLoop::<UserEvent>::with_user_event().build().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut state = State::with_size(PhysicalSize::new(WIDTH as f64, HEIGHT as f64));
    let mut surface = Surface::new(WIDTH, HEIGHT);
    surface.init();
    state.surface = Some(surface);
    
    let proxy = event_loop.create_proxy();
    let _receive_thread_handle = thread::Builder::new()
        .name(String::from("lidar"))
        .spawn(move || write_to_surface(proxy))
        .expect("[lidar] listen thread failed!");

    let _runtime = event_loop.run_app(&mut state);
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

fn write_to_surface(event_loop: EventLoopProxy<UserEvent>) {
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
            let _ = event_loop.send_event(UserEvent::DrawPointBuffer(draw_points));
        }
        //dt.write_png("lidar.png").expect("cant write output!");
    })
}

// like write_to_surface but doesnt rely on a serial device. good for testing
fn _write_to_surface_dummy(event_loop: EventLoopProxy<UserEvent>) {
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
        let _ = event_loop.send_event(UserEvent::DrawPointBuffer(vec![draw_point]));
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}