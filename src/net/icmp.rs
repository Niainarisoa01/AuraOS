//! ============================================================================
//! Internet Control Message Protocol (ICMP) Layer — Ping & Diagnostics
//! ============================================================================
//!
//! Implements ICMP Echo Requests and Echo Replies for network connectivity testing
//! and latency benchmarking (Ping).

#![allow(dead_code)]

use alloc::vec::Vec;
use crate::net::ipv4::calculate_checksum;

pub const ICMP_TYPE_ECHO_REPLY: u8 = 0;
pub const ICMP_TYPE_ECHO_REQUEST: u8 = 8;
pub const ICMP_HEADER_LEN: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IcmpHeader {
    pub msg_type: u8,
    pub code: u8,
    pub checksum: u16,
    pub identifier: u16,
    pub sequence_number: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IcmpPacket {
    pub header: IcmpHeader,
    pub payload: Vec<u8>,
}

impl IcmpPacket {
    /// Parses an ICMP packet from an IPv4 payload.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < ICMP_HEADER_LEN {
            return None;
        }

        let msg_type = data[0];
        let code = data[1];
        let checksum = u16::from_be_bytes([data[2], data[3]]);
        let identifier = u16::from_be_bytes([data[4], data[5]]);
        let sequence_number = u16::from_be_bytes([data[6], data[7]]);

        let payload = data[ICMP_HEADER_LEN..].to_vec();

        Some(IcmpPacket {
            header: IcmpHeader {
                msg_type,
                code,
                checksum,
                identifier,
                sequence_number,
            },
            payload,
        })
    }

    /// Serializes the ICMP packet with a newly computed 16-bit checksum.
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(ICMP_HEADER_LEN + self.payload.len());
        buf.push(self.header.msg_type);
        buf.push(self.header.code);
        buf.push(0); // Zero checksum initially
        buf.push(0);
        buf.extend_from_slice(&self.header.identifier.to_be_bytes());
        buf.extend_from_slice(&self.header.sequence_number.to_be_bytes());
        buf.extend_from_slice(&self.payload);

        let checksum = calculate_checksum(&buf);
        buf[2..4].copy_from_slice(&checksum.to_be_bytes());

        buf
    }
}

/// Builds an ICMP Echo Request packet (Ping).
pub fn build_echo_request(identifier: u16, sequence_number: u16, payload: &[u8]) -> Vec<u8> {
    IcmpPacket {
        header: IcmpHeader {
            msg_type: ICMP_TYPE_ECHO_REQUEST,
            code: 0,
            checksum: 0,
            identifier,
            sequence_number,
        },
        payload: payload.to_vec(),
    }.serialize()
}

/// Builds an ICMP Echo Reply answering an Echo Request.
pub fn build_echo_reply(identifier: u16, sequence_number: u16, payload: &[u8]) -> Vec<u8> {
    IcmpPacket {
        header: IcmpHeader {
            msg_type: ICMP_TYPE_ECHO_REPLY,
            code: 0,
            checksum: 0,
            identifier,
            sequence_number,
        },
        payload: payload.to_vec(),
    }.serialize()
}
