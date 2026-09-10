//! ============================================================================
//! Ethernet II Frame Protocol Layer
//! ============================================================================
//!
//! Handles 14-byte Ethernet II frames:
//!   - Destination MAC address (6 bytes)
//!   - Source MAC address (6 bytes)
//!   - EtherType (2 bytes)
//!   - Payload (46 to 1500 bytes MTU)

#![allow(dead_code)]

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

pub const ETHERTYPE_IPV4: u16 = 0x0800;
pub const ETHERTYPE_ARP: u16 = 0x0806;
pub const ETHERTYPE_IPV6: u16 = 0x86DD;

pub const ETHERNET_HEADER_LEN: usize = 14;
pub const BROADCAST_MAC: [u8; 6] = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

/// Formats a 6-byte MAC address as a standard colon-separated hex string.
pub fn format_mac(mac: &[u8; 6]) -> String {
    format!("{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EthernetHeader {
    pub dest_mac: [u8; 6],
    pub src_mac: [u8; 6],
    pub ethertype: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthernetFrame {
    pub header: EthernetHeader,
    pub payload: Vec<u8>,
}

impl EthernetFrame {
    /// Parses raw packet bytes into an Ethernet II frame.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < ETHERNET_HEADER_LEN {
            return None;
        }

        let mut dest_mac = [0u8; 6];
        let mut src_mac = [0u8; 6];
        dest_mac.copy_from_slice(&data[0..6]);
        src_mac.copy_from_slice(&data[6..12]);
        let ethertype = u16::from_be_bytes([data[12], data[13]]);

        let payload = data[ETHERNET_HEADER_LEN..].to_vec();

        Some(EthernetFrame {
            header: EthernetHeader {
                dest_mac,
                src_mac,
                ethertype,
            },
            payload,
        })
    }

    /// Serializes the frame into raw bytes ready for NIC DMA transmission.
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(ETHERNET_HEADER_LEN + self.payload.len());
        buf.extend_from_slice(&self.header.dest_mac);
        buf.extend_from_slice(&self.header.src_mac);
        buf.extend_from_slice(&self.header.ethertype.to_be_bytes());
        buf.extend_from_slice(&self.payload);

        // Ethernet requires minimum frame length of 60 bytes (excluding FCS)
        while buf.len() < 60 {
            buf.push(0);
        }

        buf
    }
}
