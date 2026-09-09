//! ============================================================================
//! IPC — Inter-Process Communication & Message Passing
//! ============================================================================
//!
//! Provides synchronous and asynchronous message queuing between processes.
//! This forms the fundamental communication backbone for AuraOS's microkernel
//! architecture, enabling isolated Ring 3 services and user tasks to exchange
//! structured payloads securely.

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use crate::sync::Spinlock;

/// Maximum payload size per IPC message (bytes).
pub const IPC_PAYLOAD_MAX: usize = 64;

/// Maximum queued messages per process mailbox.
pub const MAILBOX_CAPACITY: usize = 32;

/// A fixed-size message passed between processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct Message {
    /// Process ID of the sender
    pub sender: usize,
    /// Target Process ID
    pub target: usize,
    /// Application-defined message type / command ID
    pub msg_type: u32,
    /// Message payload data
    pub payload: [u8; IPC_PAYLOAD_MAX],
    /// Actual payload length
    pub length: u32,
}

impl Message {
    /// Creates an empty message.
    #[allow(dead_code)]
    pub const fn empty() -> Self {
        Message {
            sender: 0,
            target: 0,
            msg_type: 0,
            payload: [0; IPC_PAYLOAD_MAX],
            length: 0,
        }
    }

    /// Constructs a message from sender, target, type, and payload slice.
    pub fn new(sender: usize, target: usize, msg_type: u32, data: &[u8]) -> Self {
        let mut payload = [0u8; IPC_PAYLOAD_MAX];
        let length = core::cmp::min(data.len(), IPC_PAYLOAD_MAX);
        payload[..length].copy_from_slice(&data[..length]);
        Message {
            sender,
            target,
            msg_type,
            payload,
            length: length as u32,
        }
    }
}

/// Global IPC Message Router managing process mailboxes.
pub struct IpcRouter {
    mailboxes: Vec<(usize, VecDeque<Message>)>,
    pub total_sent: u64,
    pub total_delivered: u64,
}

impl IpcRouter {
    pub const fn new() -> Self {
        IpcRouter {
            mailboxes: Vec::new(),
            total_sent: 0,
            total_delivered: 0,
        }
    }

    /// Finds or creates a mailbox for the given target PID.
    fn get_or_create_mailbox(&mut self, pid: usize) -> &mut VecDeque<Message> {
        let pos = self.mailboxes.iter().position(|(id, _)| *id == pid);
        if let Some(idx) = pos {
            &mut self.mailboxes[idx].1
        } else {
            self.mailboxes.push((pid, VecDeque::with_capacity(MAILBOX_CAPACITY)));
            let last_idx = self.mailboxes.len() - 1;
            &mut self.mailboxes[last_idx].1
        }
    }

    /// Sends a message to `target_pid`.
    /// Returns true if queued, false if target mailbox is full.
    pub fn send(&mut self, msg: Message) -> bool {
        let mailbox = self.get_or_create_mailbox(msg.target);
        if mailbox.len() >= MAILBOX_CAPACITY {
            return false;
        }
        mailbox.push_back(msg);
        self.total_sent += 1;
        true
    }

    /// Retrieves the next available message for `my_pid` (FIFO).
    pub fn receive(&mut self, my_pid: usize) -> Option<Message> {
        let pos = self.mailboxes.iter().position(|(id, _)| *id == my_pid);
        if let Some(idx) = pos {
            let msg = self.mailboxes[idx].1.pop_front();
            if msg.is_some() {
                self.total_delivered += 1;
            }
            msg
        } else {
            None
        }
    }

    /// Returns the number of messages waiting in `pid`'s mailbox.
    #[allow(dead_code)]
    pub fn pending_count(&self, pid: usize) -> usize {
        self.mailboxes
            .iter()
            .find(|(id, _)| *id == pid)
            .map(|(_, q)| q.len())
            .unwrap_or(0)
    }

    /// Returns total active mailboxes.
    #[allow(dead_code)]
    pub fn active_mailboxes(&self) -> usize {
        self.mailboxes.len()
    }
}

/// Global thread-safe IPC Message Router singleton.
pub static IPC_ROUTER: Spinlock<IpcRouter> = Spinlock::new(IpcRouter::new());

/// Sends a message to a target process.
pub fn send_message(sender: usize, target: usize, msg_type: u32, data: &[u8]) -> bool {
    let msg = Message::new(sender, target, msg_type, data);
    IPC_ROUTER.lock().send(msg)
}

/// Receives a message for the calling process.
pub fn receive_message(my_pid: usize) -> Option<Message> {
    IPC_ROUTER.lock().receive(my_pid)
}

/// Returns IPC operational metrics: (total_sent, total_delivered, active_mailboxes).
#[allow(dead_code)]
pub fn stats() -> (u64, u64, usize) {
    let router = IPC_ROUTER.lock();
    (router.total_sent, router.total_delivered, router.active_mailboxes())
}
