use futures::stream::StreamExt;
use ld19::decoder::Packet;
use raqote::{DrawOptions, DrawTarget, PathBuilder, SolidSource, Source};
use tokio_serial::{SerialPort, SerialPortBuilderExt};
use tokio_util::codec::Decoder;

mod ld19;

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
    }
    dt.write_png("lidar.png").expect("cant write output!");
}
