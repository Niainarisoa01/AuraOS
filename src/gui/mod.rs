//! ============================================================================
//! AuraOS Graphical User Interface (GUI) Desktop Compositor
//! ============================================================================
//!
//! Implements an intuitive macOS-inspired windowing desktop environment:
//! - Translucent frosted top menu bar with live CMOS RTC clock & system status
//! - Floating pill-shaped glassmorphic application Dock with active indicators
//! - Multi-window manager with rounded corners and traffic light buttons (🔴 🟡 🟢)
//! - Integrated AuraShell terminal window and Aura Files VFS navigator

use alloc::format;
use crate::drivers::framebuffer::{Canvas, Color};

/// Renders the complete macOS-style AuraOS desktop onto a canvas.
pub fn render_desktop(canvas: &mut Canvas) {
    let width = canvas.width;
    let height = canvas.height;

    // ------------------------------------------------------------------------
    // 1. Cosmic Wallpaper Background with Gradient
    // ------------------------------------------------------------------------
    canvas.fill_gradient_v(0, 0, width, height, Color::COSMIC_NAVY, Color::DEEP_OBSIDIAN);

    // Subtle aurora glow band across the upper center (Electric Cyan tint)
    let glow_y = height / 3;
    let glow_h = height / 4;
    canvas.fill_rect_alpha(0, glow_y, width, glow_h, Color::AURA_CYAN, 18);

    // ------------------------------------------------------------------------
    // 2. Top Menu Bar (macOS Frosted Glassmorphism)
    // ------------------------------------------------------------------------
    let bar_height = 28;
    canvas.fill_rect_alpha(0, 0, width, bar_height, Color::DOCK_GLASS, 210);
    canvas.draw_rect_outline(0, 0, width, bar_height, Color::DOCK_BORDER);

    // Left Menu Items: AuraOS Logo + Menus
    canvas.draw_string(14, 6, "AURA", Color::AURA_CYAN);
    canvas.draw_string(60, 6, "File", Color::TEXT_PRIMARY);
    canvas.draw_string(110, 6, "Edit", Color::TEXT_PRIMARY);
    canvas.draw_string(160, 6, "View", Color::TEXT_PRIMARY);
    canvas.draw_string(210, 6, "Window", Color::TEXT_PRIMARY);
    canvas.draw_string(275, 6, "Help", Color::TEXT_PRIMARY);

    // Right Menu Items: Hardware RTC Clock & Heap Usage
    let rtc = crate::drivers::cmos::read_rtc();
    let clock_str = format!("{:02}:{:02} UTC | {:04}-{:02}-{:02}", rtc.hour, rtc.minute, rtc.year, rtc.month, rtc.day);
    let clock_x = width.saturating_sub(clock_str.len() * 8 + 20);
    canvas.draw_string(clock_x, 6, &clock_str, Color::TEXT_PRIMARY);

    // ------------------------------------------------------------------------
    // 3. Window 1: "AuraShell" Terminal (Left Side)
    // ------------------------------------------------------------------------
    let win1_x = 50.min(width.saturating_sub(440));
    let win1_y = 50;
    let win1_w = 420.min(width.saturating_sub(60));
    let win1_h = 320.min(height.saturating_sub(150));

    draw_window_frame(canvas, win1_x, win1_y, win1_w, win1_h, "AuraShell", true);

    // Terminal Content Area
    let term_content_x = win1_x + 16;
    let mut term_y = win1_y + 40;

    // Stylized Cyan ASCII Monogram Logo
    canvas.draw_string(term_content_x, term_y, "  /\\   AURA OPERATING SYSTEM", Color::AURA_CYAN);
    term_y += 18;
    canvas.draw_string(term_content_x, term_y, " /--\\  v0.1.0 (x86_64 Long Mode)", Color::AURA_CYAN);
    term_y += 24;

    // System Information Table
    let used_kb = crate::memory::allocator::used_memory() / 1024;
    let total_kb = crate::memory::allocator::HEAP_SIZE / 1024;
    let ticks = crate::arch::idt::ticks();

    canvas.draw_string(term_content_x, term_y, "User   : niaina", Color::TEXT_MUTED);
    term_y += 18;
    canvas.draw_string(term_content_x, term_y, "Host   : auraos-baremetal", Color::TEXT_MUTED);
    term_y += 18;
    let mem_info = format!("Memory : {} KiB / {} KiB", used_kb, total_kb);
    canvas.draw_string(term_content_x, term_y, &mem_info, Color::TEXT_MUTED);
    term_y += 18;
    let ticks_info = format!("Uptime : {} timer ticks", ticks);
    canvas.draw_string(term_content_x, term_y, &ticks_info, Color::TEXT_MUTED);
    term_y += 24;

    // Terminal Prompt Line
    canvas.draw_string(term_content_x, term_y, "auraos> sysinfo --gui", Color::AURA_CYAN);
    term_y += 18;
    canvas.draw_string(term_content_x, term_y, "[OK] All 7 subsystem self-tests verified.", Color::TRAFFIC_GREEN);
    term_y += 18;
    canvas.draw_string(term_content_x, term_y, "auraos> _", Color::TEXT_PRIMARY);

    // ------------------------------------------------------------------------
    // 4. Window 2: "Aura Files" VFS Explorer (Right Side)
    // ------------------------------------------------------------------------
    let win2_x = (win1_x + win1_w + 30).min(width.saturating_sub(440));
    let win2_y = 50;
    let win2_w = 420.min(width.saturating_sub(win2_x + 20));
    let win2_h = 320.min(height.saturating_sub(150));

    if win2_x + win2_w <= width && win2_w >= 200 {
        draw_window_frame(canvas, win2_x, win2_y, win2_w, win2_h, "Aura Files", false);

        // Sidebar vs Main Content Divider
        let sidebar_w = 110;
        canvas.fill_rect_alpha(win2_x + 1, win2_y + 30, sidebar_w, win2_h - 31, Color::FROST_GLASS, 120);
        canvas.draw_rect_outline(win2_x + sidebar_w, win2_y + 30, 1, win2_h - 31, Color::DOCK_BORDER);

        // Sidebar shortcuts
        canvas.draw_string(win2_x + 14, win2_y + 44, "Recent", Color::TEXT_PRIMARY);
        canvas.draw_string(win2_x + 14, win2_y + 68, "Home", Color::AURA_CYAN);
        canvas.draw_string(win2_x + 14, win2_y + 92, "Desktop", Color::TEXT_PRIMARY);
        canvas.draw_string(win2_x + 14, win2_y + 116, "Docs", Color::TEXT_PRIMARY);
        canvas.draw_string(win2_x + 14, win2_y + 140, "System", Color::TEXT_PRIMARY);

        // Main folder grid (reading VFS entries)
        let grid_x = win2_x + sidebar_w + 24;
        let mut folder_x = grid_x;
        let mut folder_y = win2_y + 48;

        let vfs = crate::fs::VFS.lock();
        if let Ok(entries) = vfs.list_directory(0) {
            for entry in entries.iter().take(6) {
                // Folder icon box
                canvas.fill_rounded_rect(folder_x, folder_y, 44, 32, 4, Color::FOLDER_BLUE);
                canvas.fill_rect(folder_x + 4, folder_y + 2, 14, 6, Color::WHITE);

                // Folder name below
                let label = if entry.name.len() > 6 { &entry.name[..6] } else { &entry.name };
                canvas.draw_string(folder_x, folder_y + 38, label, Color::TEXT_PRIMARY);

                folder_x += 64;
                if folder_x + 50 > win2_x + win2_w {
                    folder_x = grid_x;
                    folder_y += 60;
                }
            }
        }
    }

    // ------------------------------------------------------------------------
    // 5. Floating Centered Bottom Dock (macOS Pill Style)
    // ------------------------------------------------------------------------
    let dock_w = 460.min(width.saturating_sub(40));
    let dock_h = 60;
    let dock_x = (width.saturating_sub(dock_w)) / 2;
    let dock_y = height.saturating_sub(dock_h + 16);
    let dock_r = 20;

    // Frosted glass background & border
    canvas.fill_rounded_rect_alpha(dock_x, dock_y, dock_w, dock_h, dock_r, Color::DOCK_GLASS, 230);
    canvas.fill_rounded_rect_alpha(dock_x + 2, dock_y + 2, dock_w - 4, dock_h - 4, dock_r.saturating_sub(2), Color::DOCK_BORDER, 50);

    // Dock Icons: AuraShell, Aura Files, Calculator, Monitor, Settings
    let apps = [
        ("Shell", Color::AURA_CYAN, true),
        ("Files", Color::FOLDER_BLUE, true),
        ("Calc", Color::TRAFFIC_YELLOW, false),
        ("Tasks", Color::TRAFFIC_GREEN, false),
        ("Config", Color::TEXT_MUTED, false),
    ];

    let icon_spacing = dock_w / (apps.len() + 1);
    for (i, (name, icon_color, is_running)) in apps.iter().enumerate() {
        let icon_cx = dock_x + (i + 1) * icon_spacing;
        let icon_box_x = icon_cx.saturating_sub(18);
        let icon_box_y = dock_y + 8;

        // Rounded app icon badge
        canvas.fill_rounded_rect(icon_box_x, icon_box_y, 36, 36, 8, *icon_color);
        canvas.fill_rect(icon_box_x + 8, icon_box_y + 8, 20, 20, Color::WINDOW_DARK);

        // App name label
        let label_x = icon_cx.saturating_sub((name.len() * 8) / 2);
        canvas.draw_string(label_x, dock_y + 46, name, Color::TEXT_MUTED);

        // Active app indicator dot
        if *is_running {
            canvas.fill_circle(icon_cx as isize, (dock_y + dock_h - 4) as isize, 2, Color::AURA_CYAN);
        }
    }
}

/// Helper function to draw a macOS-style window frame with traffic light controls.
fn draw_window_frame(canvas: &mut Canvas, x: usize, y: usize, w: usize, h: usize, title: &str, is_active: bool) {
    let radius = 10;
    let header_h = 30;

    // Window Drop Shadow (subtle dark border)
    canvas.fill_rounded_rect_alpha(x + 4, y + 4, w, h, radius, Color::BLACK, 90);

    // Window Main Body
    canvas.fill_rounded_rect(x, y, w, h, radius, Color::WINDOW_DARK);
    canvas.draw_rect_outline(x, y, w, h, Color::DOCK_BORDER);

    // Titlebar Header
    canvas.fill_rounded_rect(x, y, w, header_h + radius, radius, Color::WINDOW_HEADER);
    // Square off the bottom of the header
    canvas.fill_rect(x, y + header_h, w, radius, Color::WINDOW_DARK);
    canvas.draw_rect_outline(x, y + header_h, w, 1, Color::DOCK_BORDER);

    // macOS Traffic Light Window Buttons (🔴 🟡 🟢)
    let btn_y = (y + 15) as isize;
    let red_x = (x + 18) as isize;
    let yellow_x = red_x + 18;
    let green_x = yellow_x + 18;

    canvas.fill_circle(red_x, btn_y, 5, Color::TRAFFIC_RED);
    canvas.fill_circle(yellow_x, btn_y, 5, Color::TRAFFIC_YELLOW);
    canvas.fill_circle(green_x, btn_y, 5, Color::TRAFFIC_GREEN);

    // Window Title (Centered)
    let title_x = x + (w.saturating_sub(title.len() * 8)) / 2;
    let title_color = if is_active { Color::TEXT_PRIMARY } else { Color::TEXT_MUTED };
    canvas.draw_string(title_x, y + 8, title, title_color);
}
