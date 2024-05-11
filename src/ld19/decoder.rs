// ld19
use std::io::Error;
use tokio_util::{
    bytes::{Buf, BytesMut},
    codec::Decoder,
};

const CRC_TABLE: [u8; 256] = [
    0x00, 0x4d, 0x9a, 0xd7, 0x79, 0x34, 0xe3, 0xae, 0xf2, 0xbf, 0x68, 0x25, 0x8b, 0xc6, 0x11, 0x5c,
    0xa9, 0xe4, 0x33, 0x7e, 0xd0, 0x9d, 0x4a, 0x07, 0x5b, 0x16, 0xc1, 0x8c, 0x22, 0x6f, 0xb8, 0xf5,
    0x1f, 0x52, 0x85, 0xc8, 0x66, 0x2b, 0xfc, 0xb1, 0xed, 0xa0, 0x77, 0x3a, 0x94, 0xd9, 0x0e, 0x43,
    0xb6, 0xfb, 0x2c, 0x61, 0xcf, 0x82, 0x55, 0x18, 0x44, 0x09, 0xde, 0x93, 0x3d, 0x70, 0xa7, 0xea,
    0x3e, 0x73, 0xa4, 0xe9, 0x47, 0x0a, 0xdd, 0x90, 0xcc, 0x81, 0x56, 0x1b, 0xb5, 0xf8, 0x2f, 0x62,
    0x97, 0xda, 0x0d, 0x40, 0xee, 0xa3, 0x74, 0x39, 0x65, 0x28, 0xff, 0xb2, 0x1c, 0x51, 0x86, 0xcb,
    0x21, 0x6c, 0xbb, 0xf6, 0x58, 0x15, 0xc2, 0x8f, 0xd3, 0x9e, 0x49, 0x04, 0xaa, 0xe7, 0x30, 0x7d,
    0x88, 0xc5, 0x12, 0x5f, 0xf1, 0xbc, 0x6b, 0x26, 0x7a, 0x37, 0xe0, 0xad, 0x03, 0x4e, 0x99, 0xd4,
    0x7c, 0x31, 0xe6, 0xab, 0x05, 0x48, 0x9f, 0xd2, 0x8e, 0xc3, 0x14, 0x59, 0xf7, 0xba, 0x6d, 0x20,
    0xd5, 0x98, 0x4f, 0x02, 0xac, 0xe1, 0x36, 0x7b, 0x27, 0x6a, 0xbd, 0xf0, 0x5e, 0x13, 0xc4, 0x89,
    0x63, 0x2e, 0xf9, 0xb4, 0x1a, 0x57, 0x80, 0xcd, 0x91, 0xdc, 0x0b, 0x46, 0xe8, 0xa5, 0x72, 0x3f,
    0xca, 0x87, 0x50, 0x1d, 0xb3, 0xfe, 0x29, 0x64, 0x38, 0x75, 0xa2, 0xef, 0x41, 0x0c, 0xdb, 0x96,
    0x42, 0x0f, 0xd8, 0x95, 0x3b, 0x76, 0xa1, 0xec, 0xb0, 0xfd, 0x2a, 0x67, 0xc9, 0x84, 0x53, 0x1e,
    0xeb, 0xa6, 0x71, 0x3c, 0x92, 0xdf, 0x08, 0x45, 0x19, 0x54, 0x83, 0xce, 0x60, 0x2d, 0xfa, 0xb7,
    0x5d, 0x10, 0xc7, 0x8a, 0x24, 0x69, 0xbe, 0xf3, 0xaf, 0xe2, 0x35, 0x78, 0xd6, 0x9b, 0x4c, 0x01,
    0xf4, 0xb9, 0x6e, 0x23, 0x8d, 0xc0, 0x17, 0x5a, 0x06, 0x4b, 0x9c, 0xd1, 0x7f, 0x32, 0xe5, 0xa8,
];
fn calc_crc(bytes: &[u8], len: usize) -> u8 {
    bytes
        .iter()
        .take(len)
        .fold(0, |crc, b| CRC_TABLE[(crc ^ b) as usize])
}

const DATA_HEADER: u8 = 0x54;
const DATA_VER_LEN: u8 = 0x2c;
// datasheet claims there's always gonna be 12 ¯\_(ツ)_/¯
const PAYLOAD_COUNT: usize = 12;
const PAYLOAD_BYTES: usize = 3;
const HEADER_BYTES: usize = 2;
const PACKET_BYTES: usize = 47;

pub enum ParseError {
    InvalidHeader,
    InvalidPredata,
    InvalidPayloadLength,
    InvalidPayload,
    InvalidPostdata,
}

#[derive(Debug)]
pub struct Payload {
    pub distance: u16,
    pub intensity: u8,
}

#[derive(Debug)]
pub struct Header {
    header: u8,
    ver_len: u8,
}

impl Header {
    fn packet_bytes(&self) -> usize {
        PACKET_BYTES
    }
    fn payload_count(&self) -> usize {
        PAYLOAD_COUNT
    }
    fn payload_bytes(&self) -> usize {
        // technically i should check verlen for the DataPayload length,
        // but because the datasheet claims verlen is fixed we can assume there
        // is always 12 payloads
        PAYLOAD_COUNT * PAYLOAD_BYTES
    }
}

#[derive(Debug)]
pub struct Packet {
    pub header: Header,
    pub speed: u16,
    pub start_angle: u16,
    pub data: Vec<Payload>,
    pub end_angle: u16,
    pub timestamp: u16,
    crc: u8,
}

impl Packet {
    pub fn calculate_crc(&self) -> u8 {
        self.crc
    }
}

fn parse_header(slice: &[u8]) -> Option<Header> {
    let header = slice[0];
    if header != DATA_HEADER {
        return None;
    }
    let ver_len = slice[1];
    if ver_len != DATA_VER_LEN {
        return None;
    }
    return Some(Header { header, ver_len });
}

fn try_parse_descibed_packet(header: Header, slice: &[u8]) -> Result<Packet, ParseError> {
    // see what i should do is use payload_length to parse that many
    // DataPayloads, but i dont want to do that right now so im sticking
    // with just using the datasheet's claim of 12
    let mut b = Vec::<u8>::new();
    b.extend_from_slice(slice);
    //b.copy_from_slice(&slice[..header.packet_bytes()]);
    //println!("[debug] described packet slice: {:?} (len:{} )", b, b.len());
    let speed = match b[2..4].try_into() {
        Ok(arr) => u16::from_le_bytes(arr),
        Err(_) => return Err(ParseError::InvalidPredata),
    };
    let start_angle = match b[4..6].try_into() {
        Ok(arr) => u16::from_le_bytes(arr),
        Err(_) => return Err(ParseError::InvalidPredata),
    };
    // data: b[6..(6+(payload_count * PAYLOAD_BYTES))]
    let payload_count = header.payload_count();
    let data = try_parse_n_payloads(payload_count, slice)?;
    let payload_end_index = 6 + header.payload_bytes();
    let end_angle = match b[payload_end_index..(payload_end_index + 2)].try_into() {
        Ok(arr) => u16::from_le_bytes(arr),
        Err(_) => return Err(ParseError::InvalidPostdata),
    };
    let timestamp = match b[(payload_end_index + 2)..(payload_end_index + 4)].try_into() {
        Ok(arr) => u16::from_le_bytes(arr),
        Err(_) => return Err(ParseError::InvalidPostdata),
    };
    let crc = b[payload_end_index + 4];
    Ok(Packet {
        header,
        speed,
        start_angle,
        data,
        end_angle,
        timestamp,
        crc,
    })
}

fn try_parse_n_payloads(n: usize, slice: &[u8]) -> Result<Vec<Payload>, ParseError> {
    if slice.len() < n * PAYLOAD_BYTES {
        return Err(ParseError::InvalidPayloadLength);
    }
    let mut data = Vec::<Payload>::new();
    for i in 0..(n - 1) {
        let distance = match slice[(i * PAYLOAD_BYTES)..((i * PAYLOAD_BYTES) + 2)].try_into() {
            Ok(arr) => u16::from_le_bytes(arr),
            Err(_) => return Err(ParseError::InvalidPayload),
        };
        let intensity = slice[(i * PAYLOAD_BYTES) + 2];
        data.push(Payload {
            distance,
            intensity,
        })
    }
    Ok(data)
}

pub struct LidarCodec;

impl Decoder for LidarCodec {
    type Item = Packet;
    type Error = std::io::Error;
    // everything is in little endian btw
    // check serial_data_format.txt for format details but also its in the
    // DataPacket and DataPayload structs
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let current_len = src.len();
        if current_len < HEADER_BYTES {
            // reserve enough for at least a header
            src.reserve(HEADER_BYTES);
            return Ok(None);
        }
        // check the header now
        if let Some(header) = parse_header(&src[..2]) {
            if current_len < header.packet_bytes() {
                // packet not big enough so reserve it (plus buffer just in case)
                src.reserve(header.packet_bytes() - current_len);
                return Ok(None);
            }
            let packet_data = src.split_to(header.packet_bytes());
            return match try_parse_descibed_packet(header, packet_data.as_ref()) {
                Ok(packet) => {
                    // verify crc
                    Ok(Some(packet))
                }
                Err(ParseError::InvalidHeader) => Err(Error::other("invalid header")),
                Err(ParseError::InvalidPredata) => Err(Error::other("invalid predata")),
                Err(ParseError::InvalidPayloadLength) => {
                    Err(Error::other("invalid payload length"))
                }
                Err(ParseError::InvalidPayload) => Err(Error::other("invalid payload")),
                Err(ParseError::InvalidPostdata) => Err(Error::other("invalid postdata")),
            };
        } else {
            // no header was parsed, split off the first byte loop and try the next one
            let _ = src.advance(1);
        }
        Ok(None)
    }
}
