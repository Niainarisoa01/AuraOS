//! ============================================================================
//! AuraOS Native Network Stack (Ethernet, ARP, IPv4, ICMP Ping, UDP)
//! ============================================================================
//!
//! Pure `#![no_std]` Rust implementation of foundational networking protocols.
//! Interacts directly with the Intel e1000 Gigabit Ethernet controller.

#![allow(dead_code)]

pub mod ethernet;
pub mod arp;
pub mod ipv4;
pub mod icmp;
pub mod udp;

use alloc::vec::Vec;
use crate::sync::Spinlock;
use self::ethernet::{EthernetFrame, EthernetHeader, ETHERTYPE_ARP, ETHERTYPE_IPV4, BROADCAST_MAC};
use self::arp::{ArpPacket, ArpTable, ARP_OP_REQUEST, build_arp_reply};
use self::ipv4::{Ipv4Packet, IPV4_PROTO_ICMP, IPV4_PROTO_UDP};
use self::icmp::{IcmpPacket, ICMP_TYPE_ECHO_REQUEST, ICMP_TYPE_ECHO_REPLY, build_echo_reply, build_echo_request};
use self::udp::UdpPacket;

/// Central Network Management State.
pub struct NetworkManager {
    pub mac: [u8; 6],
    pub ip: [u8; 4],
    pub subnet: [u8; 4],
    pub gateway: [u8; 4],
    pub dns: [u8; 4],
    pub arp_table: ArpTable,

    pub is_enabled: bool,
    pub pings_sent: u64,
    pub pings_received: u64,
    pub last_ping_seq: u16,
    pub last_ping_sent_tick: u64,
    pub last_ping_rtt_ms: Option<u64>,

    pub packets_rx: u64,
    pub packets_tx: u64,
    pub bytes_rx: u64,
    pub bytes_tx: u64,
}

impl NetworkManager {
    pub const fn new() -> Self {
        NetworkManager {
            mac: [0x52, 0x54, 0x00, 0x12, 0x34, 0x56], // Default QEMU guest MAC
            ip: [10, 0, 2, 15],                         // Default QEMU guest IP
            subnet: [255, 255, 255, 0],
            gateway: [10, 0, 2, 2],                     // Default QEMU router IP
            dns: [10, 0, 2, 3],                         // Default QEMU DNS
            arp_table: ArpTable::new(),
            is_enabled: false,
            pings_sent: 0,
            pings_received: 0,
            last_ping_seq: 0,
            last_ping_sent_tick: 0,
            last_ping_rtt_ms: None,
            packets_rx: 0,
            packets_tx: 0,
            bytes_rx: 0,
            bytes_tx: 0,
        }
    }

    /// Initializes the network stack and underlying Intel e1000 driver.
    pub fn init(&mut self) -> bool {
        let nic_ok = crate::drivers::e1000::init();
        if nic_ok {
            let nic = crate::drivers::e1000::E1000_DEVICE.lock();
            self.mac = nic.mac;
            self.is_enabled = true;

            // Pre-seed ARP table with QEMU default router MAC
            self.arp_table.insert([10, 0, 2, 2], [0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
            self.arp_table.insert([10, 0, 2, 3], [0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);

            crate::serial_println!(
                "[Network] Stack initialized. IP: {}.{}.{}.{}, MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                self.ip[0], self.ip[1], self.ip[2], self.ip[3],
                self.mac[0], self.mac[1], self.mac[2], self.mac[3], self.mac[4], self.mac[5]
            );
        } else {
            crate::serial_println!("[Network] e1000 hardware not present; network stack in loopback mode.");
        }
        self.is_enabled
    }

    /// Transmits an Ethernet II frame directly through the hardware NIC.
    pub fn send_frame(&mut self, dest_mac: [u8; 6], ethertype: u16, payload: &[u8]) -> bool {
        let frame = EthernetFrame {
            header: EthernetHeader {
                dest_mac,
                src_mac: self.mac,
                ethertype,
            },
            payload: payload.to_vec(),
        };

        let raw = frame.serialize();
        let mut nic = crate::drivers::e1000::E1000_DEVICE.lock();
        let ok = nic.send_packet(&raw);
        if ok {
            self.packets_tx += 1;
            self.bytes_tx += raw.len() as u64;
        }
        ok
    }

    /// Resolves destination MAC address via ARP and transmits an IPv4 packet.
    pub fn send_ipv4(&mut self, dest_ip: [u8; 4], protocol: u8, payload: Vec<u8>) -> bool {
        let is_broadcast = dest_ip == [255, 255, 255, 255];
        let is_same_subnet = (dest_ip[0] == self.ip[0]) && (dest_ip[1] == self.ip[1]) && (dest_ip[2] == self.ip[2]);

        let target_ip_for_arp = if is_broadcast {
            dest_ip
        } else if is_same_subnet {
            dest_ip
        } else {
            self.gateway
        };

        let dest_mac = if is_broadcast {
            BROADCAST_MAC
        } else if let Some(mac) = self.arp_table.lookup(&target_ip_for_arp) {
            mac
        } else {
            // Send ARP request to find MAC address
            let arp_req = arp::build_arp_request(self.mac, self.ip, target_ip_for_arp);
            self.send_frame(BROADCAST_MAC, ETHERTYPE_ARP, &arp_req);
            // Fallback to gateway or broadcast
            BROADCAST_MAC
        };

        let packet = Ipv4Packet::new(self.ip, dest_ip, protocol, payload);
        let raw_ip = packet.serialize();
        self.send_frame(dest_mac, ETHERTYPE_IPV4, &raw_ip)
    }

    /// Sends an ICMP Echo Request (Ping).
    pub fn send_ping(&mut self, target_ip: [u8; 4]) -> bool {
        self.last_ping_seq = self.last_ping_seq.wrapping_add(1);
        let seq = self.last_ping_seq;
        self.last_ping_sent_tick = crate::arch::idt::ticks();
        self.last_ping_rtt_ms = None;

        let payload = b"AuraOS-Ping-Packet-2026";
        let icmp_data = build_echo_request(0x1337, seq, payload);
        self.pings_sent += 1;

        crate::serial_println!(
            "[PING] Transmitting ICMP Echo Request to {}.{}.{}.{} (seq={})",
            target_ip[0], target_ip[1], target_ip[2], target_ip[3], seq
        );

        self.send_ipv4(target_ip, IPV4_PROTO_ICMP, icmp_data)
    }

    /// Sends a UDP datagram to the specified destination IP and port.
    pub fn send_udp(&mut self, dest_ip: [u8; 4], src_port: u16, dest_port: u16, data: &[u8]) -> bool {
        let udp_pkt = UdpPacket::new(self.ip, dest_ip, src_port, dest_port, data.to_vec());
        let raw_udp = udp_pkt.serialize();
        self.send_ipv4(dest_ip, IPV4_PROTO_UDP, raw_udp)
    }

    /// Polls the network hardware for incoming packets and processes them.
    pub fn poll(&mut self) {
        let packet = {
            let mut nic = crate::drivers::e1000::E1000_DEVICE.lock();
            nic.receive_packet()
        };

        if let Some(raw) = packet {
            self.packets_rx += 1;
            self.bytes_rx += raw.len() as u64;

            if let Some(frame) = EthernetFrame::parse(&raw) {
                match frame.header.ethertype {
                    ETHERTYPE_ARP => {
                        self.handle_arp(&frame.payload);
                    }
                    ETHERTYPE_IPV4 => {
                        self.handle_ipv4(&frame.payload);
                    }
                    _ => {}
                }
            }
        }
    }

    /// Processes an incoming ARP packet.
    fn handle_arp(&mut self, payload: &[u8]) {
        if let Some(arp) = ArpPacket::parse(payload) {
            // Update ARP table with sender info
            self.arp_table.insert(arp.sender_ip, arp.sender_mac);

            if arp.oper == ARP_OP_REQUEST && arp.target_ip == self.ip {
                // Reply to ARP request
                let reply_payload = build_arp_reply(self.mac, self.ip, arp.sender_mac, arp.sender_ip);
                self.send_frame(arp.sender_mac, ETHERTYPE_ARP, &reply_payload);
                crate::serial_println!(
                    "[ARP] Replied to {}.{}.{}.{} at {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                    arp.sender_ip[0], arp.sender_ip[1], arp.sender_ip[2], arp.sender_ip[3],
                    arp.sender_mac[0], arp.sender_mac[1], arp.sender_mac[2], arp.sender_mac[3], arp.sender_mac[4], arp.sender_mac[5]
                );
            }
        }
    }

    /// Processes an incoming IPv4 packet.
    fn handle_ipv4(&mut self, payload: &[u8]) {
        if let Some(ip_pkt) = Ipv4Packet::parse(payload) {
            match ip_pkt.header.protocol {
                IPV4_PROTO_ICMP => {
                    self.handle_icmp(&ip_pkt.header, &ip_pkt.payload);
                }
                IPV4_PROTO_UDP => {
                    self.handle_udp(&ip_pkt.header, &ip_pkt.payload);
                }
                _ => {}
            }
        }
    }

    /// Processes an incoming ICMP packet.
    fn handle_icmp(&mut self, ip_hdr: &ipv4::Ipv4Header, payload: &[u8]) {
        if let Some(icmp) = IcmpPacket::parse(payload) {
            match icmp.header.msg_type {
                ICMP_TYPE_ECHO_REQUEST => {
                    // Automatically answer with Echo Reply
                    let reply_payload = build_echo_reply(icmp.header.identifier, icmp.header.sequence_number, &icmp.payload);
                    self.send_ipv4(ip_hdr.source_ip, IPV4_PROTO_ICMP, reply_payload);
                    crate::serial_println!(
                        "[ICMP] Replied to Echo Request from {}.{}.{}.{}",
                        ip_hdr.source_ip[0], ip_hdr.source_ip[1], ip_hdr.source_ip[2], ip_hdr.source_ip[3]
                    );
                }
                ICMP_TYPE_ECHO_REPLY => {
                    self.pings_received += 1;
                    let now = crate::arch::idt::ticks();
                    let ticks_diff = now.saturating_sub(self.last_ping_sent_tick);
                    let ms = ticks_diff * 10; // PIT IRQ0 is 100 Hz (10 ms per tick)
                    self.last_ping_rtt_ms = Some(ms);

                    crate::serial_println!(
                        "[ICMP] Received Echo Reply from {}.{}.{}.{} (seq={}, rtt={} ms)",
                        ip_hdr.source_ip[0], ip_hdr.source_ip[1], ip_hdr.source_ip[2], ip_hdr.source_ip[3],
                        icmp.header.sequence_number, ms
                    );
                }
                _ => {}
            }
        }
    }

    /// Processes an incoming UDP datagram.
    fn handle_udp(&mut self, ip_hdr: &ipv4::Ipv4Header, payload: &[u8]) {
        if let Some(udp) = UdpPacket::parse(payload) {
            crate::serial_println!(
                "[UDP] Datagram from {}.{}.{}.{}:{} -> Port {} (len={})",
                ip_hdr.source_ip[0], ip_hdr.source_ip[1], ip_hdr.source_ip[2], ip_hdr.source_ip[3],
                udp.header.src_port, udp.header.dest_port, udp.payload.len()
            );
        }
    }
}

/// Global Network Manager protected by Spinlock.
pub static NETWORK: Spinlock<NetworkManager> = Spinlock::new(NetworkManager::new());

/// Initializes the global networking subsystem.
pub fn init() -> bool {
    let mut net = NETWORK.lock();
    net.init()
}

/// Periodically called to process incoming network packets.
pub fn poll() {
    let mut net = NETWORK.lock();
    net.poll();
}
