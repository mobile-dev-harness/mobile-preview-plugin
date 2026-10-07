//! MPP video framing, version 1. This is not the scrcpy wire protocol.
//!
//! Each packet starts with a 28-byte header. Multi-byte integers are big-endian:
//! `MPP1` (4 bytes), kind (1 byte: configuration=0, key frame=1, delta frame=2),
//! three zero reserved bytes, generation (`u64`, offset 8), payload length
//! (`u32`, offset 16), and presentation timestamp in microseconds (`u64`,
//! offset 20). Generation must be nonzero.
//! A configuration payload contains width (`u32`), height (`u32`), and nonempty
//! H.264 Annex B configuration bytes. Frame payloads contain nonempty encoded
//! bytes. Dimensions must be in 1..=16384. Codec bytes are opaque to this module;
//! validation of H.264 contents belongs to the video decoder.
//!
//! The payload limit is inclusive, counts the configuration's dimension prefix,
//! and must be in 9..=16 MiB. Incremental parsing buffers at most one header and
//! one bounded payload; returned packets necessarily occupy additional memory.
//! Parsing errors invalidate the decoder, and no packets from a failed `push`
//! are returned. This module neither captures video nor provides a transport.

use crate::{Error, Result};

const HEADER_LEN: usize = 28;
const CONFIGURATION_PREFIX_LEN: usize = 8;
const MIN_PAYLOAD_LIMIT: usize = CONFIGURATION_PREFIX_LEN + 1;
const MAX_PAYLOAD_LIMIT: usize = 16 * 1024 * 1024;
const MAX_DIMENSION: u32 = 16_384;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packet {
    pub generation: u64,
    pub pts_us: u64,
    pub body: PacketBody,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PacketBody {
    Configuration {
        width: u32,
        height: u32,
        annex_b: Vec<u8>,
    },
    KeyFrame(Vec<u8>),
    DeltaFrame(Vec<u8>),
}

/// Encode one packet after validating all sizes and metadata.
pub fn encode(packet: &Packet, max_payload: usize) -> Result<Vec<u8>> {
    validate_limit(max_payload)?;
    validate_generation(packet.generation)?;
    let (kind, payload_len) = match &packet.body {
        PacketBody::Configuration {
            width,
            height,
            annex_b,
        } => {
            validate_dimensions(*width, *height)?;
            if annex_b.is_empty() {
                return Err(invalid("configuration bytes must not be empty"));
            }
            let len = annex_b
                .len()
                .checked_add(CONFIGURATION_PREFIX_LEN)
                .ok_or_else(|| invalid("configuration payload length overflow"))?;
            (0, len)
        }
        PacketBody::KeyFrame(bytes) => (1, bytes.len()),
        PacketBody::DeltaFrame(bytes) => (2, bytes.len()),
    };
    validate_payload_len(kind, payload_len, max_payload)?;
    let total_len = HEADER_LEN
        .checked_add(payload_len)
        .ok_or_else(|| invalid("packet length overflow"))?;
    let wire_len = u32::try_from(payload_len)
        .map_err(|_| invalid("payload length cannot be represented on the wire"))?;
    let mut encoded = Vec::with_capacity(total_len);
    encoded.extend_from_slice(b"MPP1");
    encoded.extend_from_slice(&[kind, 0, 0, 0]);
    encoded.extend_from_slice(&packet.generation.to_be_bytes());
    encoded.extend_from_slice(&wire_len.to_be_bytes());
    encoded.extend_from_slice(&packet.pts_us.to_be_bytes());
    match &packet.body {
        PacketBody::Configuration {
            width,
            height,
            annex_b,
        } => {
            encoded.extend_from_slice(&width.to_be_bytes());
            encoded.extend_from_slice(&height.to_be_bytes());
            encoded.extend_from_slice(annex_b);
        }
        PacketBody::KeyFrame(bytes) | PacketBody::DeltaFrame(bytes) => {
            encoded.extend_from_slice(bytes);
        }
    }
    Ok(encoded)
}

#[derive(Debug)]
pub struct Decoder {
    max_payload: usize,
    header_bytes: [u8; HEADER_LEN],
    header_len: usize,
    header: Option<Header>,
    payload: Vec<u8>,
    failed: bool,
}

#[derive(Clone, Copy, Debug)]
struct Header {
    kind: u8,
    generation: u64,
    payload_len: usize,
    pts_us: u64,
}

impl Decoder {
    pub fn new(max_payload: usize) -> Result<Self> {
        validate_limit(max_payload)?;
        Ok(Self {
            max_payload,
            header_bytes: [0; HEADER_LEN],
            header_len: 0,
            header: None,
            payload: Vec::new(),
            failed: false,
        })
    }

    /// Accept arbitrary chunks without buffering the entire input slice.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Packet>> {
        if self.failed {
            return Err(invalid("decoder is invalid after a previous parse error"));
        }
        let result = self.push_inner(bytes);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn push_inner(&mut self, mut bytes: &[u8]) -> Result<Vec<Packet>> {
        let mut packets = Vec::new();
        while !bytes.is_empty() {
            if self.header.is_none() {
                let count = (HEADER_LEN - self.header_len).min(bytes.len());
                self.header_bytes[self.header_len..self.header_len + count]
                    .copy_from_slice(&bytes[..count]);
                self.header_len += count;
                bytes = &bytes[count..];
                if self.header_len < HEADER_LEN {
                    break;
                }
                // Parse and bound the advertised length before reserving any payload memory.
                let header = parse_header(&self.header_bytes, self.max_payload)?;
                self.payload.reserve_exact(header.payload_len);
                self.header = Some(header);
            }

            let header = self.header.expect("a complete header was parsed above");
            let count = (header.payload_len - self.payload.len()).min(bytes.len());
            self.payload.extend_from_slice(&bytes[..count]);
            bytes = &bytes[count..];
            if self.payload.len() == header.payload_len {
                let body = parse_body(header.kind, std::mem::take(&mut self.payload))?;
                packets.push(Packet {
                    generation: header.generation,
                    pts_us: header.pts_us,
                    body,
                });
                self.header = None;
                self.header_len = 0;
            }
        }
        Ok(packets)
    }

    /// Validate end-of-stream; an incomplete final header or payload is an error.
    pub fn finish(self) -> Result<()> {
        if self.failed {
            return Err(invalid("decoder is invalid after a previous parse error"));
        }
        if self.header_len != 0 || self.header.is_some() {
            return Err(invalid("truncated video packet at end of stream"));
        }
        Ok(())
    }
}

fn parse_header(bytes: &[u8; HEADER_LEN], max_payload: usize) -> Result<Header> {
    if &bytes[..4] != b"MPP1" {
        return Err(invalid("unsupported video packet magic or version"));
    }
    let kind = bytes[4];
    if kind > 2 {
        return Err(invalid("unknown video packet kind"));
    }
    if bytes[5..8] != [0, 0, 0] {
        return Err(invalid("video packet reserved bytes must be zero"));
    }
    let generation = u64::from_be_bytes(bytes[8..16].try_into().expect("fixed generation slice"));
    validate_generation(generation)?;
    let payload_len = usize::try_from(read_u32(&bytes[16..20]))
        .map_err(|_| invalid("payload length cannot be represented on this platform"))?;
    validate_payload_len(kind, payload_len, max_payload)?;
    let pts_us = u64::from_be_bytes(bytes[20..28].try_into().expect("fixed timestamp slice"));
    Ok(Header {
        kind,
        generation,
        payload_len,
        pts_us,
    })
}

fn parse_body(kind: u8, mut bytes: Vec<u8>) -> Result<PacketBody> {
    match kind {
        0 => {
            let width = read_u32(&bytes[..4]);
            let height = read_u32(&bytes[4..8]);
            validate_dimensions(width, height)?;
            bytes.drain(..CONFIGURATION_PREFIX_LEN);
            Ok(PacketBody::Configuration {
                width,
                height,
                annex_b: bytes,
            })
        }
        1 => Ok(PacketBody::KeyFrame(bytes)),
        2 => Ok(PacketBody::DeltaFrame(bytes)),
        _ => Err(invalid("unknown video packet kind")),
    }
}

fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().expect("fixed u32 slice"))
}

fn validate_limit(max_payload: usize) -> Result<()> {
    if !(MIN_PAYLOAD_LIMIT..=MAX_PAYLOAD_LIMIT).contains(&max_payload) {
        return Err(invalid("max_payload must be in 9..=16777216 bytes"));
    }
    Ok(())
}

fn validate_generation(generation: u64) -> Result<()> {
    if generation == 0 {
        return Err(invalid("video packet generation must be nonzero"));
    }
    Ok(())
}

fn validate_dimensions(width: u32, height: u32) -> Result<()> {
    if !(1..=MAX_DIMENSION).contains(&width) || !(1..=MAX_DIMENSION).contains(&height) {
        return Err(invalid("video dimensions must be in 1..=16384"));
    }
    Ok(())
}

fn validate_payload_len(kind: u8, len: usize, max_payload: usize) -> Result<()> {
    if len > max_payload {
        return Err(invalid("video payload exceeds max_payload"));
    }
    let minimum = if kind == 0 { MIN_PAYLOAD_LIMIT } else { 1 };
    if len < minimum {
        return Err(invalid(
            "video payload is empty or missing configuration bytes",
        ));
    }
    Ok(())
}

fn invalid(message: &str) -> Error {
    Error::InvalidArgument {
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packets() -> Vec<Packet> {
        vec![
            Packet {
                generation: 7,
                pts_us: 0,
                body: PacketBody::Configuration {
                    width: 1080,
                    height: 1920,
                    annex_b: vec![0, 0, 0, 1, 0x67, 0x42],
                },
            },
            Packet {
                generation: 7,
                pts_us: 12_345,
                body: PacketBody::KeyFrame(vec![0, 0, 1, 0x65, 1, 2]),
            },
            Packet {
                generation: 7,
                pts_us: u64::MAX,
                body: PacketBody::DeltaFrame(vec![0, 0, 1, 0x41, 3]),
            },
        ]
    }

    fn stream(packets: &[Packet]) -> Vec<u8> {
        packets
            .iter()
            .flat_map(|packet| encode(packet, 64).unwrap())
            .collect()
    }

    #[test]
    fn wire_header_is_fixed_width_and_big_endian() {
        let packet = Packet {
            generation: 0x1234_5678_9abc_def0,
            pts_us: 0x0102_0304_0506_0708,
            body: PacketBody::KeyFrame(vec![0xff]),
        };
        assert_eq!(
            encode(&packet, 9).unwrap(),
            vec![
                b'M', b'P', b'P', b'1', 1, 0, 0, 0, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0,
                0, 0, 0, 1, 1, 2, 3, 4, 5, 6, 7, 8, 0xff,
            ]
        );
    }

    #[test]
    fn round_trip_all_packet_kinds_in_one_byte_chunks() {
        let expected = packets();
        let mut decoder = Decoder::new(64).unwrap();
        let mut actual = Vec::new();
        for byte in stream(&expected) {
            actual.extend(decoder.push(&[byte]).unwrap());
        }
        assert_eq!(actual, expected);
        decoder.finish().unwrap();
    }

    #[test]
    fn generations_beyond_u32_round_trip_without_truncation() {
        for generation in [u64::from(u32::MAX) + 1, u64::MAX] {
            let mut packet = packets().remove(1);
            packet.generation = generation;
            let bytes = encode(&packet, 64).unwrap();
            let mut decoder = Decoder::new(64).unwrap();
            assert_eq!(decoder.push(&bytes).unwrap(), vec![packet]);
            decoder.finish().unwrap();
        }
    }

    #[test]
    fn coalesced_packets_round_trip_at_every_split() {
        let expected = packets();
        let bytes = stream(&expected);
        for split in 0..=bytes.len() {
            let mut decoder = Decoder::new(64).unwrap();
            let mut actual = decoder.push(&bytes[..split]).unwrap();
            actual.extend(decoder.push(&bytes[split..]).unwrap());
            assert_eq!(actual, expected, "split {split}");
            decoder.finish().unwrap();
        }
    }

    #[test]
    fn accepts_empty_stream_and_empty_chunks() {
        let mut decoder = Decoder::new(9).unwrap();
        assert!(decoder.push(&[]).unwrap().is_empty());
        decoder.finish().unwrap();
    }

    #[test]
    fn rejects_malformed_headers_without_payload_allocation() {
        let valid = encode(&packets()[1], 64).unwrap();
        let mut corruptions = Vec::new();
        for (offset, value) in [(0, b'X'), (3, b'2'), (4, 3), (5, 1), (6, 1), (7, 1)] {
            let mut bytes = valid.clone();
            bytes[offset] = value;
            corruptions.push(bytes);
        }
        let mut zero_generation = valid.clone();
        zero_generation[8..16].fill(0);
        corruptions.push(zero_generation);
        for len in [0u32, 65, u32::MAX] {
            let mut bytes = valid.clone();
            bytes[16..20].copy_from_slice(&len.to_be_bytes());
            corruptions.push(bytes);
        }
        for bytes in corruptions {
            let mut decoder = Decoder::new(64).unwrap();
            assert!(decoder.push(&bytes).is_err());
            assert_eq!(decoder.payload.capacity(), 0);
            assert!(decoder.push(&valid).is_err());
            assert!(decoder.finish().is_err());
        }
    }

    #[test]
    fn rejects_configuration_without_codec_bytes() {
        for len in 0..=CONFIGURATION_PREFIX_LEN {
            let mut header = encode(&packets()[0], 64).unwrap();
            header[16..20].copy_from_slice(&(len as u32).to_be_bytes());
            let mut decoder = Decoder::new(64).unwrap();
            assert!(decoder.push(&header[..HEADER_LEN]).is_err());
            assert_eq!(decoder.payload.capacity(), 0);
        }
    }

    #[test]
    fn rejects_invalid_dimensions_in_encoder_and_decoder() {
        for (width, height) in [(0, 1), (1, 0), (16_385, 1), (1, 16_385), (u32::MAX, 1)] {
            let packet = Packet {
                generation: 1,
                pts_us: 0,
                body: PacketBody::Configuration {
                    width,
                    height,
                    annex_b: vec![1],
                },
            };
            assert!(encode(&packet, 64).is_err());
            let mut bytes = encode(&packets()[0], 64).unwrap();
            bytes[28..32].copy_from_slice(&width.to_be_bytes());
            bytes[32..36].copy_from_slice(&height.to_be_bytes());
            assert!(Decoder::new(64).unwrap().push(&bytes).is_err());
        }
    }

    #[test]
    fn rejects_empty_payloads_zero_generation_and_oversize_on_encode() {
        for body in [
            PacketBody::KeyFrame(Vec::new()),
            PacketBody::DeltaFrame(Vec::new()),
            PacketBody::Configuration {
                width: 1,
                height: 1,
                annex_b: Vec::new(),
            },
            PacketBody::KeyFrame(vec![1; 10]),
            PacketBody::Configuration {
                width: 1,
                height: 1,
                annex_b: vec![1; 2],
            },
        ] {
            assert!(
                encode(
                    &Packet {
                        generation: 1,
                        pts_us: 0,
                        body,
                    },
                    9,
                )
                .is_err()
            );
        }
        let mut packet = packets().remove(1);
        packet.generation = 0;
        assert!(encode(&packet, 64).is_err());
    }

    #[test]
    fn payload_limits_and_dimension_boundaries_are_inclusive() {
        let packet = Packet {
            generation: u64::MAX,
            pts_us: u64::MAX,
            body: PacketBody::Configuration {
                width: 1,
                height: 16_384,
                annex_b: vec![1],
            },
        };
        for limit in [0, 1, 8, MAX_PAYLOAD_LIMIT + 1, usize::MAX] {
            assert!(Decoder::new(limit).is_err());
            assert!(encode(&packet, limit).is_err());
        }
        for limit in [9, MAX_PAYLOAD_LIMIT] {
            let bytes = encode(&packet, limit).unwrap();
            let mut decoder = Decoder::new(limit).unwrap();
            assert_eq!(decoder.push(&bytes).unwrap(), vec![packet.clone()]);
            decoder.finish().unwrap();
        }
        let frame = Packet {
            generation: 1,
            pts_us: 0,
            body: PacketBody::DeltaFrame(vec![7; 9]),
        };
        let bytes = encode(&frame, 9).unwrap();
        assert_eq!(Decoder::new(9).unwrap().push(&bytes).unwrap(), vec![frame]);
    }

    #[test]
    fn finish_rejects_every_partial_header_and_payload() {
        for packet in packets() {
            let bytes = encode(&packet, 64).unwrap();
            for end in 1..bytes.len() {
                let mut decoder = Decoder::new(64).unwrap();
                assert!(decoder.push(&bytes[..end]).unwrap().is_empty());
                assert!(decoder.finish().is_err(), "truncated at {end}");
            }
        }
        let bytes = stream(&packets());
        let mut decoder = Decoder::new(64).unwrap();
        assert_eq!(decoder.push(&bytes[..bytes.len() - 1]).unwrap().len(), 2);
        assert!(decoder.finish().is_err());
    }

    #[test]
    fn large_input_does_not_become_an_internal_buffer() {
        let packet = Packet {
            generation: 1,
            pts_us: 0,
            body: PacketBody::KeyFrame(vec![1]),
        };
        let bytes = encode(&packet, 9).unwrap().repeat(10_000);
        let mut decoder = Decoder::new(9).unwrap();
        let decoded = decoder.push(&bytes).unwrap();
        assert_eq!(decoded.len(), 10_000);
        assert!(decoded.iter().all(|value| value == &packet));
        assert_eq!(decoder.payload.capacity(), 0);
        assert_eq!(decoder.header_len, 0);
        decoder.finish().unwrap();

        let mut bytes = vec![0xff; 1_000_000];
        bytes[..HEADER_LEN].copy_from_slice(&encode(&packet, 9).unwrap()[..HEADER_LEN]);
        bytes[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        let mut decoder = Decoder::new(9).unwrap();
        assert!(decoder.push(&bytes).is_err());
        assert_eq!(decoder.payload.capacity(), 0);
    }
}
