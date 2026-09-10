//! ============================================================================
//! User Datagram Protocol (UDP) Layer
//! ============================================================================
//!
//! Provides connectionless, lightweight datagram communication over IPv4.

#![allow(dead_code)]

use alloc::vec::Vec;
use crate::net::ipv4::calculate_checksum;

pub const UDP_HEADER_LEN: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UdpHeader {
    pub src_port: u16,
    pub dest_port: u16,
    pub length: u16,
    pub checksum: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UdpPacket {
    pub header: UdpHeader,
    pub payload: Vec<u8>,
}

impl UdpPacket {
    /// Parses a UDP datagram from an IPv4 payload.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < UDP_HEADER_LEN {
            return None;
        }

        let src_port = u16::from_be_bytes([data[0], data[1]]);
        let dest_port = u16::from_be_bytes([data[2], data[3]]);
        let length = u16::from_be_bytes([data[4], data[5]]) as usize;
        let checksum = u16::from_be_bytes([data[6], data[7]]);

        if data.len() < length || length < UDP_HEADER_LEN {
            return None;
        }

        let payload = data[UDP_HEADER_LEN..length].to_vec();

        Some(UdpPacket {
            header: UdpHeader {
                src_port,
                dest_port,
                length: length as u16,
                checksum,
            },
            payload,
        })
    }

    /// Creates and serializes a new UDP datagram with optional IPv4 pseudo-header checksum.
    pub fn new(src_ip: [u8; 4], dest_ip: [u8; 4], src_port: u16, dest_port: u16, payload: Vec<u8>) -> Self {
        let length = (UDP_HEADER_LEN + payload.len()) as u16;

        // Build pseudo-header for UDP checksum
        let mut pseudo = Vec::with_capacity(12 + UDP_HEADER_LEN + payload.len());
        pseudo.extend_from_slice(&src_ip);
        pseudo.extend_from_slice(&dest_ip);
        pseudo.push(0);  // Reserved zero byte
        pseudo.push(17); // Protocol 17 = UDP
        pseudo.extend_from_slice(&length.to_be_bytes());

        // UDP Header (checksum initially 0)
        pseudo.extend_from_slice(&src_port.to_be_bytes());
        pseudo.extend_from_slice(&dest_port.to_be_bytes());
        pseudo.extend_from_slice(&length.to_be_bytes());
        pseudo.extend_from_slice(&[0, 0]);
        pseudo.extend_from_slice(&payload);

        let mut checksum = calculate_checksum(&pseudo);
        if checksum == 0 {
            checksum = 0xFFFF; // RFC 768: transmitted zero checksum as 0xFFFF
        }

        UdpPacket {
            header: UdpHeader {
                src_port,
                dest_port,
                length,
                checksum,
            },
            payload,
        }
    }

    /// Serializes the UDP datagram into raw bytes.
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(UDP_HEADER_LEN + self.payload.len());
        buf.extend_from_slice(&self.header.src_port.to_be_bytes());
        buf.extend_from_slice(&self.header.dest_port.to_be_bytes());
        buf.extend_from_slice(&self.header.length.to_be_bytes());
        buf.extend_from_slice(&self.header.checksum.to_be_bytes());
        buf.extend_from_slice(&self.payload);
        buf
    }
}
