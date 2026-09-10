//! ============================================================================
//! Internet Protocol version 4 (IPv4) Layer
//! ============================================================================
//!
//! Handles 20-byte IPv4 packet headers, RFC 1071 checksum calculation,
//! protocol demultiplexing (ICMP, UDP, TCP), and packet serialization.

#![allow(dead_code)]

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

pub const IPV4_PROTO_ICMP: u8 = 1;
pub const IPV4_PROTO_TCP: u8 = 6;
pub const IPV4_PROTO_UDP: u8 = 17;

pub const IPV4_MIN_HEADER_LEN: usize = 20;

/// Formats a 4-byte IPv4 address as a standard dotted decimal string.
pub fn format_ipv4(ip: &[u8; 4]) -> String {
    format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
}

/// Parses a dotted-decimal string (e.g. "10.0.2.15") into a 4-byte IPv4 address.
pub fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
    let mut parts = s.split('.');
    let a: u8 = parts.next()?.parse().ok()?;
    let b: u8 = parts.next()?.parse().ok()?;
    let c: u8 = parts.next()?.parse().ok()?;
    let d: u8 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some([a, b, c, d])
}

/// Computes the 16-bit Internet Checksum according to RFC 1071.
pub fn calculate_checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut i = 0;
    while i + 1 < data.len() {
        let word = u16::from_be_bytes([data[i], data[i + 1]]);
        sum = sum.wrapping_add(word as u32);
        i += 2;
    }
    if i < data.len() {
        let word = (data[i] as u32) << 8;
        sum = sum.wrapping_add(word);
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ipv4Header {
    pub version_ihl: u8,
    pub tos: u8,
    pub total_length: u16,
    pub identification: u16,
    pub flags_fragment: u16,
    pub ttl: u8,
    pub protocol: u8,
    pub checksum: u16,
    pub source_ip: [u8; 4],
    pub dest_ip: [u8; 4],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ipv4Packet {
    pub header: Ipv4Header,
    pub payload: Vec<u8>,
}

impl Ipv4Packet {
    /// Parses an IPv4 packet from raw bytes and validates its checksum.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < IPV4_MIN_HEADER_LEN {
            return None;
        }

        let version_ihl = data[0];
        let ihl = (version_ihl & 0x0F) as usize * 4;
        if data.len() < ihl || ihl < IPV4_MIN_HEADER_LEN {
            return None;
        }

        let total_length = u16::from_be_bytes([data[2], data[3]]) as usize;
        if data.len() < total_length {
            return None;
        }

        // Validate header checksum
        if calculate_checksum(&data[0..ihl]) != 0 {
            return None;
        }

        let tos = data[1];
        let identification = u16::from_be_bytes([data[4], data[5]]);
        let flags_fragment = u16::from_be_bytes([data[6], data[7]]);
        let ttl = data[8];
        let protocol = data[9];
        let checksum = u16::from_be_bytes([data[10], data[11]]);

        let mut source_ip = [0u8; 4];
        let mut dest_ip = [0u8; 4];
        source_ip.copy_from_slice(&data[12..16]);
        dest_ip.copy_from_slice(&data[16..20]);

        let payload = data[ihl..total_length].to_vec();

        Some(Ipv4Packet {
            header: Ipv4Header {
                version_ihl,
                tos,
                total_length: total_length as u16,
                identification,
                flags_fragment,
                ttl,
                protocol,
                checksum,
                source_ip,
                dest_ip,
            },
            payload,
        })
    }

    /// Creates a new IPv4 packet and automatically calculates the header checksum.
    pub fn new(source_ip: [u8; 4], dest_ip: [u8; 4], protocol: u8, payload: Vec<u8>) -> Self {
        let total_length = (IPV4_MIN_HEADER_LEN + payload.len()) as u16;
        let mut header = Ipv4Header {
            version_ihl: 0x45, // Version 4, IHL 5 (20 bytes)
            tos: 0,
            total_length,
            identification: 0x1337,
            flags_fragment: 0x4000, // Don't fragment
            ttl: 64,
            protocol,
            checksum: 0,
            source_ip,
            dest_ip,
        };

        // Compute checksum over 20-byte header
        let mut raw_hdr = [0u8; IPV4_MIN_HEADER_LEN];
        raw_hdr[0] = header.version_ihl;
        raw_hdr[1] = header.tos;
        raw_hdr[2..4].copy_from_slice(&header.total_length.to_be_bytes());
        raw_hdr[4..6].copy_from_slice(&header.identification.to_be_bytes());
        raw_hdr[6..8].copy_from_slice(&header.flags_fragment.to_be_bytes());
        raw_hdr[8] = header.ttl;
        raw_hdr[9] = header.protocol;
        raw_hdr[10..12].copy_from_slice(&[0, 0]); // Zero checksum for calculation
        raw_hdr[12..16].copy_from_slice(&header.source_ip);
        raw_hdr[16..20].copy_from_slice(&header.dest_ip);

        header.checksum = calculate_checksum(&raw_hdr);

        Ipv4Packet { header, payload }
    }

    /// Serializes the IPv4 packet into raw bytes.
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(IPV4_MIN_HEADER_LEN + self.payload.len());
        buf.push(self.header.version_ihl);
        buf.push(self.header.tos);
        buf.extend_from_slice(&self.header.total_length.to_be_bytes());
        buf.extend_from_slice(&self.header.identification.to_be_bytes());
        buf.extend_from_slice(&self.header.flags_fragment.to_be_bytes());
        buf.push(self.header.ttl);
        buf.push(self.header.protocol);
        buf.extend_from_slice(&self.header.checksum.to_be_bytes());
        buf.extend_from_slice(&self.header.source_ip);
        buf.extend_from_slice(&self.header.dest_ip);
        buf.extend_from_slice(&self.payload);
        buf
    }
}
