//! ============================================================================
//! ATA / IDE Hard Disk PIO Mode Storage Driver
//! ============================================================================
//!
//! Implements 28-bit LBA PIO mode disk read and write operations on the
//! Primary ATA / IDE bus (ports 0x1F0 - 0x1F7).
//!
//! Standard Sector Size: 512 bytes.

use crate::arch::io::{inb, inw, io_wait, outb, outw};

const ATA_DATA_PORT: u16 = 0x1F0;
const ATA_SECTOR_COUNT: u16 = 0x1F2;
const ATA_LBA_LOW: u16 = 0x1F3;
const ATA_LBA_MID: u16 = 0x1F4;
const ATA_LBA_HIGH: u16 = 0x1F5;
const ATA_DRIVE_SELECT: u16 = 0x1F6;
const ATA_COMMAND_STATUS: u16 = 0x1F7;

#[allow(dead_code)]
const ATA_CMD_READ_SECTORS: u8 = 0x20;
#[allow(dead_code)]
const ATA_CMD_WRITE_SECTORS: u8 = 0x30;
#[allow(dead_code)]
const ATA_CMD_CACHE_FLUSH: u8 = 0xE7;

const ATA_STATUS_BSY: u8 = 0x80; // Drive busy
const ATA_STATUS_DRQ: u8 = 0x08; // Data Request ready
const ATA_STATUS_ERR: u8 = 0x01; // Error

pub const SECTOR_SIZE: usize = 512;

/// Waits for the drive to clear the BSY flag and assert the DRQ flag.
fn wait_drive_ready() -> Result<(), &'static str> {
    for _ in 0..100_000 {
        let status = unsafe { inb(ATA_COMMAND_STATUS) };
        if status == 0xFF {
            return Err("No ATA drive connected (floating bus 0xFF)");
        }
        if (status & ATA_STATUS_ERR) != 0 {
            return Err("ATA hardware error status flag set");
        }
        if (status & ATA_STATUS_BSY) == 0 && (status & ATA_STATUS_DRQ) != 0 {
            return Ok(());
        }
        unsafe { io_wait() };
    }
    Err("ATA drive timeout waiting for DRQ")
}

/// Waits until the drive is no longer busy.
fn wait_drive_not_busy() -> Result<(), &'static str> {
    for _ in 0..100_000 {
        let status = unsafe { inb(ATA_COMMAND_STATUS) };
        if status == 0xFF {
            return Err("No ATA drive connected (floating bus 0xFF)");
        }
        if (status & ATA_STATUS_BSY) == 0 {
            return Ok(());
        }
        unsafe { io_wait() };
    }
    Err("ATA drive timeout waiting for BSY to clear")
}

/// Reads a single 512-byte sector from the primary master hard drive using 28-bit LBA.
#[allow(dead_code)]
pub fn read_sector(lba: u32, buffer: &mut [u8; SECTOR_SIZE]) -> Result<(), &'static str> {
    if lba > 0x0FFF_FFFF {
        return Err("LBA exceeds 28-bit addressing limit");
    }

    unsafe {
        wait_drive_not_busy()?;

        // Select Master drive (0xE0) + top 4 bits of LBA
        outb(ATA_DRIVE_SELECT, 0xE0 | (((lba >> 24) & 0x0F) as u8));
        io_wait();

        // Transfer 1 sector
        outb(ATA_SECTOR_COUNT, 1);
        outb(ATA_LBA_LOW, lba as u8);
        outb(ATA_LBA_MID, (lba >> 8) as u8);
        outb(ATA_LBA_HIGH, (lba >> 16) as u8);

        // Issue READ SECTORS command
        outb(ATA_COMMAND_STATUS, ATA_CMD_READ_SECTORS);

        // Wait until drive is ready to transfer data
        wait_drive_ready()?;

        // Read 256 16-bit words (512 bytes)
        for i in 0..256 {
            let word = inw(ATA_DATA_PORT);
            buffer[i * 2] = (word & 0xFF) as u8;
            buffer[i * 2 + 1] = ((word >> 8) & 0xFF) as u8;
        }
    }

    Ok(())
}

/// Writes a single 512-byte sector to the primary master hard drive using 28-bit LBA.
#[allow(dead_code)]
pub fn write_sector(lba: u32, buffer: &[u8; SECTOR_SIZE]) -> Result<(), &'static str> {
    if lba > 0x0FFF_FFFF {
        return Err("LBA exceeds 28-bit addressing limit");
    }

    unsafe {
        wait_drive_not_busy()?;

        // Select Master drive (0xE0) + top 4 bits of LBA
        outb(ATA_DRIVE_SELECT, 0xE0 | (((lba >> 24) & 0x0F) as u8));
        io_wait();

        // Transfer 1 sector
        outb(ATA_SECTOR_COUNT, 1);
        outb(ATA_LBA_LOW, lba as u8);
        outb(ATA_LBA_MID, (lba >> 8) as u8);
        outb(ATA_LBA_HIGH, (lba >> 16) as u8);

        // Issue WRITE SECTORS command
        outb(ATA_COMMAND_STATUS, ATA_CMD_WRITE_SECTORS);

        // Wait until drive is ready to receive data
        wait_drive_ready()?;

        // Write 256 16-bit words (512 bytes)
        for i in 0..256 {
            let word = (buffer[i * 2] as u16) | ((buffer[i * 2 + 1] as u16) << 8);
            outw(ATA_DATA_PORT, word);
        }

        // Issue CACHE FLUSH to commit data to non-volatile media
        outb(ATA_COMMAND_STATUS, ATA_CMD_CACHE_FLUSH);
        wait_drive_not_busy()?;
    }

    Ok(())
}
