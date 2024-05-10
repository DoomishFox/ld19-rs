use futures::stream::StreamExt;
use tokio_serial::{SerialPort, SerialPortBuilderExt};
use tokio_util::codec::Decoder;

mod ld19;

struct Point {
    angle: f32,
    distance: u16,
    confidence: u8,
    x: f64,
    y: f64,
}

#[tokio::main]
async fn main() {
    println!("starting...");
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

    let mut reader = ld19::decoder::LidarCodec.framed(serial);
    while let Some(reader_result) = reader.next().await {
        println!("received data: {:?}", reader_result);
    }
}
