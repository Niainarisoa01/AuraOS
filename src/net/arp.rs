//! ============================================================================
//! Address Resolution Protocol (ARP) Layer
//! ============================================================================
//!
//! Resolves IPv4 addresses to physical Ethernet MAC addresses.
//! Maintains an in-memory dynamic ARP cache.

#![allow(dead_code)]

use alloc::vec::Vec;

pub const ARP_HTYPE_ETHERNET: u16 = 1;
pub const ARP_PTYPE_IPV4: u16 = 0x0800;
pub const ARP_HLEN_ETHERNET: u8 = 6;
pub const ARP_PLEN_IPV4: u8 = 4;

pub const ARP_OP_REQUEST: u16 = 1;
pub const ARP_OP_REPLY: u16 = 2;
pub const ARP_PACKET_LEN: usize = 28;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArpPacket {
    pub htype: u16,
    pub ptype: u16,
    pub hlen: u8,
    pub plen: u8,
    pub oper: u16,
    pub sender_mac: [u8; 6],
    pub sender_ip: [u8; 4],
    pub target_mac: [u8; 6],
    pub target_ip: [u8; 4],
}

impl ArpPacket {
    /// Parses an ARP packet from raw payload bytes.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < ARP_PACKET_LEN {
            return None;
        }

        let htype = u16::from_be_bytes([data[0], data[1]]);
        let ptype = u16::from_be_bytes([data[2], data[3]]);
        let hlen = data[4];
        let plen = data[5];
        let oper = u16::from_be_bytes([data[6], data[7]]);

        let mut sender_mac = [0u8; 6];
        let mut sender_ip = [0u8; 4];
        let mut target_mac = [0u8; 6];
        let mut target_ip = [0u8; 4];

        sender_mac.copy_from_slice(&data[8..14]);
        sender_ip.copy_from_slice(&data[14..18]);
        target_mac.copy_from_slice(&data[18..24]);
        target_ip.copy_from_slice(&data[24..28]);

        Some(ArpPacket {
            htype,
            ptype,
            hlen,
            plen,
            oper,
            sender_mac,
            sender_ip,
            target_mac,
            target_ip,
        })
    }

    /// Serializes the ARP packet into 28 bytes.
    pub fn serialize(&self) -> [u8; ARP_PACKET_LEN] {
        let mut buf = [0u8; ARP_PACKET_LEN];
        buf[0..2].copy_from_slice(&self.htype.to_be_bytes());
        buf[2..4].copy_from_slice(&self.ptype.to_be_bytes());
        buf[4] = self.hlen;
        buf[5] = self.plen;
        buf[6..8].copy_from_slice(&self.oper.to_be_bytes());
        buf[8..14].copy_from_slice(&self.sender_mac);
        buf[14..18].copy_from_slice(&self.sender_ip);
        buf[18..24].copy_from_slice(&self.target_mac);
        buf[24..28].copy_from_slice(&self.target_ip);
        buf
    }
}

/// Builds an ARP Request packet asking "Who has target_ip? Tell sender_ip".
pub fn build_arp_request(sender_mac: [u8; 6], sender_ip: [u8; 4], target_ip: [u8; 4]) -> [u8; ARP_PACKET_LEN] {
    ArpPacket {
        htype: ARP_HTYPE_ETHERNET,
        ptype: ARP_PTYPE_IPV4,
        hlen: ARP_HLEN_ETHERNET,
        plen: ARP_PLEN_IPV4,
        oper: ARP_OP_REQUEST,
        sender_mac,
        sender_ip,
        target_mac: [0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        target_ip,
    }.serialize()
}

/// Builds an ARP Reply packet answering "sender_ip is at sender_mac".
pub fn build_arp_reply(
    sender_mac: [u8; 6],
    sender_ip: [u8; 4],
    target_mac: [u8; 6],
    target_ip: [u8; 4],
) -> [u8; ARP_PACKET_LEN] {
    ArpPacket {
        htype: ARP_HTYPE_ETHERNET,
        ptype: ARP_PTYPE_IPV4,
        hlen: ARP_HLEN_ETHERNET,
        plen: ARP_PLEN_IPV4,
        oper: ARP_OP_REPLY,
        sender_mac,
        sender_ip,
        target_mac,
        target_ip,
    }.serialize()
}

/// Dynamic ARP cache table entry.
#[derive(Debug, Clone, Copy)]
pub struct ArpEntry {
    pub ip: [u8; 4],
    pub mac: [u8; 6],
    pub timestamp_tick: u64,
}

/// In-memory table caching IP to MAC translations.
pub struct ArpTable {
    pub entries: Vec<ArpEntry>,
}

impl ArpTable {
    pub const fn new() -> Self {
        ArpTable {
            entries: Vec::new(),
        }
    }

    /// Looks up a MAC address corresponding to an IPv4 address.
    pub fn lookup(&self, ip: &[u8; 4]) -> Option<[u8; 6]> {
        for entry in &self.entries {
            if entry.ip == *ip {
                return Some(entry.mac);
            }
        }
        None
    }

    /// Inserts or updates an IP to MAC translation in the table.
    pub fn insert(&mut self, ip: [u8; 4], mac: [u8; 6]) {
        let now = crate::arch::idt::ticks();
        for entry in self.entries.iter_mut() {
            if entry.ip == ip {
                entry.mac = mac;
                entry.timestamp_tick = now;
                return;
            }
        }
        self.entries.push(ArpEntry {
            ip,
            mac,
            timestamp_tick: now,
        });
    }
}
