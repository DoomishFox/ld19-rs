// ld19
use core::fmt;
use std::io;
use tokio_util::{
    bytes::{Buf, BytesMut},
    codec::Decoder,
};

// just naming this for easier readability
const U16_LEN: usize = 2;

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

#[derive(Debug)]
pub enum ParseError {
    InvalidHeader,
    InvalidPredata,
    InvalidPayloadLength,
    InvalidPayload,
    InvalidPostdata,
    DescribedLengthMismatch,
    // other io errors
    Io(io::Error),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::InvalidHeader => write!(f, "invalid header"),
            ParseError::InvalidPredata => write!(f, "invalid predata"),
            ParseError::InvalidPayloadLength => write!(f, "invalid payload length"),
            ParseError::InvalidPayload => write!(f, "invalid payload"),
            ParseError::InvalidPostdata => write!(f, "invalid postdata"),
            ParseError::DescribedLengthMismatch => write!(f, "described length mismatch"),
            ParseError::Io(e) => write!(f, "{}", e),
        }
    }
}

impl From<io::Error> for ParseError {
    fn from(e: io::Error) -> ParseError {
        ParseError::Io(e)
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug)]
pub struct Header {
    header: u8,
    ver_len: u8,
}

impl fmt::Display for Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Header {{ indicator: {}, version: {}, length (payload count): {} }}",
            Self::INDICATOR,
            self.version(),
            self.payload_count(),
        )
    }
}

impl Header {
    const BYTES: usize = 2;
    const INDICATOR: u8 = 0x54;
    const VER_LEN_DEFAULT: u8 = 0x2c;

    /// attempt to parse slice using the header indicator value. if indicator
    /// value is not present at the start of the slice, an InvalidHeader
    /// ParseError will result.
    fn try_parse(bytes: &[u8]) -> Result<Self, ParseError> {
        if bytes[0] != Self::INDICATOR {
            return Err(ParseError::InvalidHeader);
        }
        Ok(Self {
            header: Self::INDICATOR,
            ver_len: bytes[1],
        })
    }
    /// attempt to parse slice using both the header indicator value and the
    /// default ver_len (0x2c) so as to avoid rare edge cases. only possible
    /// when ver_len is static. if indicator value is not present at the start
    /// of the slice, or ver_len is not the default value, an InvalidHeader
    /// ParseError will result.
    fn try_parse_strict(bytes: &[u8]) -> Result<Self, ParseError> {
        if bytes.len() < Self::BYTES {
            return Err(ParseError::InvalidHeader);
        }
        // length check since we're check 2 values
        if bytes[0] != Self::INDICATOR {
            return Err(ParseError::InvalidHeader);
        }
        if bytes[1] != Self::VER_LEN_DEFAULT {
            return Err(ParseError::InvalidHeader);
        }
        Ok(Self {
            header: Self::INDICATOR,
            ver_len: bytes[1],
        })
    }
    fn version(&self) -> usize {
        (self.ver_len >> 5) as usize
    }
    fn payload_count(&self) -> usize {
        (self.ver_len & 0b00011111) as usize
    }
    fn payload_bytes(&self) -> usize {
        self.payload_count() * Payload::BYTES
    }
    fn described_bytes(&self) -> usize {
        // header bytes + payload bytes + rest of packet bytes (speed,
        // timestamp, crc, etc.)
        Self::BYTES + self.payload_bytes() + 9
    }
}

#[derive(Debug)]
pub struct Payload {
    pub distance: u16,
    pub intensity: u8,
}

impl fmt::Display for Payload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Payload {{ distance: {}, intensity: {} }}",
            self.distance, self.intensity,
        )
    }
}

impl Payload {
    const BYTES: usize = 3;

    fn try_from_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
        if bytes.len() < Self::BYTES {
            return Err(ParseError::InvalidPayloadLength);
        }
        Ok(Self::from_bytes(bytes))
    }
    fn from_bytes(bytes: &[u8]) -> Self {
        let distance = u16::from_le_bytes(bytes[..U16_LEN].try_into().unwrap()).clone();
        let intensity = bytes[U16_LEN].clone();

        Self {
            distance,
            intensity,
        }
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

impl fmt::Display for Packet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Packet timestamped (ms): {} \
            {{ \
            version: {}, \
            degrees per second: {}, \
            starting angle: {}, \
            ending angle: {}, \
            data points: [",
            self.timestamp,
            self.header.version(),
            self.speed,
            self.start_angle,
            self.end_angle,
        )?;
        let mut first = true;
        for payload in &self.data {
            if !first {
                write!(f, ", {}", payload)?;
            } else {
                write!(f, "{}", payload)?;
            }
            first = false
        }
        write!(f, "], crc: {} }}", self.crc)
    }
}

impl Packet {
    const IDX_SPEED: usize = 2;
    const IDX_START_ANGLE: usize = 4;
    const IDX_PAYLOAD: usize = 6;
    const OFST_TIMESTAMP: usize = 2;
    const OFST_CRC: usize = 4;

    fn try_from_described_bytes(header: Header, bytes: &[u8]) -> Result<Self, ParseError> {
        if bytes.len() < header.described_bytes() {
            return Err(ParseError::DescribedLengthMismatch);
        }
        Ok(Self::from_described_bytes(header, bytes))
    }
    fn from_described_bytes(header: Header, bytes: &[u8]) -> Self {
        // not sure why i did the extend_from_slice thing before, i think it
        // was because otherwise i ended up with references stored in the
        // resultant Packet? clone() should fix that tho...
        //let mut b = Vec::<u8>::new();
        //b.extend_from_slice(bytes);
        let speed = u16::from_le_bytes(
            bytes[Self::IDX_SPEED..Self::IDX_SPEED + U16_LEN]
                .try_into()
                .unwrap(),
        )
        .clone();
        let start_angle = u16::from_le_bytes(
            bytes[Self::IDX_START_ANGLE..Self::IDX_START_ANGLE + U16_LEN]
                .try_into()
                .unwrap(),
        )
        .clone();

        let mut data = Vec::<Payload>::new();
        for i in 0..(header.payload_count()) {
            data.push(Payload::from_bytes(
                bytes[Self::IDX_PAYLOAD + (i * Payload::BYTES)
                    ..Self::IDX_PAYLOAD + (i * Payload::BYTES) + Payload::BYTES]
                    .try_into()
                    .unwrap(),
            ));
        }
        let idx_payload_end: usize = Self::IDX_PAYLOAD + (data.len() * Payload::BYTES);

        let end_angle = u16::from_le_bytes(
            bytes[idx_payload_end..idx_payload_end + U16_LEN]
                .try_into()
                .unwrap(),
        )
        .clone();
        let timestamp = u16::from_le_bytes(
            bytes[idx_payload_end + Self::OFST_TIMESTAMP
                ..idx_payload_end + Self::OFST_TIMESTAMP + U16_LEN]
                .try_into()
                .unwrap(),
        )
        .clone();
        let crc = bytes[idx_payload_end + Self::OFST_CRC].clone();

        Self {
            header,
            speed,
            start_angle,
            data,
            end_angle,
            timestamp,
            crc,
        }
    }
    fn try_get_crc_from_described_bytes(header: &Header, bytes: &[u8]) -> Result<u8, ParseError> {
        if bytes.len() < header.described_bytes() {
            return Err(ParseError::DescribedLengthMismatch);
        }
        Ok(Self::get_crc_from_described_bytes(header, bytes))
    }
    fn get_crc_from_described_bytes(header: &Header, bytes: &[u8]) -> u8 {
        let idx_payload_end: usize = Self::IDX_PAYLOAD + header.payload_bytes();
        bytes[idx_payload_end + Self::OFST_CRC].clone()
        // lmao, technically i could also just do
        //bytes[header.described_bytes() - 1].clone()
    }
    fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.push(self.header.header);
        bytes.push(self.header.ver_len);
        bytes.extend(self.speed.to_le_bytes());
        bytes.extend(self.start_angle.to_le_bytes());
        for payload in &self.data {
            bytes.extend(payload.distance.to_le_bytes());
            bytes.push(payload.intensity);
        }
        bytes.extend(self.end_angle.to_le_bytes());
        bytes.extend(self.timestamp.to_le_bytes());
        bytes.push(self.crc);
        return bytes;
    }
    fn length_in_bytes(&self) -> usize {
        Header::BYTES + self.data.len() + 9
    }
}

pub struct LidarCodec;

impl Decoder for LidarCodec {
    type Item = Packet;
    type Error = ParseError;
    // everything is in little endian btw
    // check serial_data_format.txt for format details but also its in the
    // DataPacket and DataPayload structs
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let current_len = src.len();
        if current_len < Header::BYTES {
            // reserve enough for at least a header
            src.reserve(Header::BYTES - current_len);
            return Ok(None);
        }
        // check the header now
        if let Ok(header) = Header::try_parse(&src[..2]) {
            if current_len < header.described_bytes() {
                // packet not big enough so reserve it
                src.reserve(header.described_bytes() - current_len);
                return Ok(None);
            }
            let packet_data = src.split_to(header.described_bytes());

            // we verify the crc before parsing to make sure we received the
            // data we should have. if not, there's no need to parse it. since
            // the crc calculation does not include the crc value, we can
            // decrease the packet_data length to ignore it
            if Packet::get_crc_from_described_bytes(&header, &packet_data)
            != calc_crc(&packet_data, header.described_bytes() - 1)
            {
                // here is a great spot to log a warning if i ever get around
                // to implementing logging
                return Ok(None);
            }

            // length check has already been satisfied so no need for try_*
            // functions, we can just shove the data into shape
            return Ok(Some(Packet::from_described_bytes(header, &packet_data)));
        } else {
            // no header was parsed, split off the first byte loop and try the next one
            let _ = src.advance(1);
        }
        Ok(None)
    }
}
