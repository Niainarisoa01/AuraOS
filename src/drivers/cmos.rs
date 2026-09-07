//! ============================================================================
//! CMOS Real-Time Clock (RTC) Driver
//! ============================================================================
//!
//! Reads the current hardware calendar date and time from the motherboard's
//! battery-backed CMOS chip (Motorola MC146818 standard).
//!
//! Accessed via I/O ports:
//!   - 0x70: CMOS Index / Address Register (Selects RTC register to read/write)
//!   - 0x71: CMOS Data Register

use crate::arch::io::{inb, outb};

const CMOS_INDEX: u16 = 0x70;
const CMOS_DATA: u16 = 0x71;

/// Represents a calendar date and time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl DateTime {
    #[allow(dead_code)]
    pub const fn zero() -> Self {
        DateTime {
            year: 0,
            month: 0,
            day: 0,
            hour: 0,
            minute: 0,
            second: 0,
        }
    }
}

/// Reads a single byte from the specified CMOS register index.
fn read_cmos_register(reg: u8) -> u8 {
    unsafe {
        // Bit 7 in port 0x70 disables NMI; we keep NMI enabled by leaving bit 7 as 0
        outb(CMOS_INDEX, reg);
        inb(CMOS_DATA)
    }
}

/// Checks if an RTC update is currently in progress (bit 7 of Status Register A).
/// Reading registers during an update can yield corrupted or inconsistent values.
fn is_update_in_progress() -> bool {
    (read_cmos_register(0x0A) & 0x80) != 0
}

/// Converts a Binary Coded Decimal (BCD) byte to binary if needed.
fn bcd_to_binary(val: u8) -> u8 {
    ((val >> 4) * 10) + (val & 0x0F)
}

/// Reads the raw date and time values from CMOS.
fn read_raw_rtc() -> DateTime {
    // Wait until the RTC is not updating, with safety timeout
    let mut timeout = 10_000;
    while is_update_in_progress() && timeout > 0 {
        core::hint::spin_loop();
        timeout -= 1;
    }

    let mut second = read_cmos_register(0x00);
    let mut minute = read_cmos_register(0x02);
    let mut hour = read_cmos_register(0x04);
    let mut day = read_cmos_register(0x07);
    let mut month = read_cmos_register(0x08);
    let mut year = read_cmos_register(0x09) as u16;
    let century = read_cmos_register(0x32) as u16;

    let register_b = read_cmos_register(0x0B);

    // Convert from BCD to binary if BCD mode is enabled (bit 2 is 0)
    let is_bcd = (register_b & 0x04) == 0;
    if is_bcd {
        second = bcd_to_binary(second);
        minute = bcd_to_binary(minute);
        hour = bcd_to_binary(hour & 0x7F) | (hour & 0x80);
        day = bcd_to_binary(day);
        month = bcd_to_binary(month);
        year = bcd_to_binary(year as u8) as u16;
    }

    // Convert 12-hour format to 24-hour format if needed (bit 1 is 0 for 12-hour mode)
    let is_24hr = (register_b & 0x02) != 0;
    if !is_24hr {
        let is_pm = (hour & 0x80) != 0;
        let raw_hour = hour & 0x7F;
        hour = match (is_pm, raw_hour) {
            (false, 12) => 0,          // 12 AM -> 00:00
            (true, 12) => 12,          // 12 PM -> 12:00
            (true, h) => (h + 12) % 24,// 1 PM..11 PM -> 13:00..23:00
            (false, h) => h % 24,      // 1 AM..11 AM -> 01:00..11:00
        };
    }

    // Calculate complete 4-digit year
    // Note: CMOS register 0x32 (century) is non-standard and may return garbage
    // on some hardware. We only trust values in the plausible range 19..21.
    let full_year = if century > 0 {
        let actual_century = if is_bcd { bcd_to_binary(century as u8) as u16 } else { century };
        if actual_century >= 19 && actual_century <= 21 {
            actual_century * 100 + year
        } else {
            // Century register returned implausible value, use default
            2000 + year
        }
    } else {
        // Modern default: assumes 21st century (2000s)
        2000 + year
    };

    DateTime {
        year: full_year,
        month,
        day,
        hour,
        minute,
        second,
    }
}

/// Reads the real-time clock, performing consecutive reads to guard against
/// rollover glitches during a second transition.
pub fn read_rtc() -> DateTime {
    let mut last = read_raw_rtc();
    for _ in 0..5 {
        let current = read_raw_rtc();
        if current == last {
            return current;
        }
        last = current;
    }
    last
}
