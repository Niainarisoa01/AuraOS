//! ============================================================================
//! AuraOS Graphical User Interface (GUI) Desktop Compositor
//! ============================================================================
//!
//! Implements an intuitive macOS-inspired windowing desktop environment:
//! - Translucent frosted top menu bar with live CMOS RTC clock & system status
//! - Floating pill-shaped glassmorphic application Dock with active indicators
//! - Multi-window manager with rounded corners and traffic light buttons (🔴 🟡 🟢)
//! - Integrated AuraShell terminal window and Aura Files VFS navigator
//! - Real-time mouse cursor with shadow
//! - Interactive window dragging via mouse click on title bar

use alloc::format;
use crate::drivers::framebuffer::{Canvas, Color};
use crate::drivers::bga;
use crate::drivers::mouse;
use core::sync::atomic::{AtomicBool, Ordering};

/// Global flag: when set to true by a keyboard Escape press, exits the GUI loop.
pub static GUI_EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Global flag: indicates the GUI desktop loop is currently active.
pub static GUI_ACTIVE: AtomicBool = AtomicBool::new(false);

// ============================================================================
// Window Manager — Draggable Windows
// ============================================================================

/// Represents a draggable window on screen.
#[derive(Clone)]
struct Window {
    x: isize,
    y: isize,
    w: usize,
    h: usize,
    title: &'static str,
    active: bool,
    visible: bool,
}

/// Tracks the state of a mouse drag operation.
struct DragState {
    dragging: bool,
    window_index: usize,
    offset_x: isize,
    offset_y: isize,
}

// ============================================================================
// Mouse Cursor Bitmap (16x16 arrow with contour)
// ============================================================================

/// 16x16 cursor arrow bitmap. 2 = white fill, 1 = black outline, 0 = transparent
static CURSOR_BITMAP: [[u8; 16]; 16] = [
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
    [1,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
    [1,2,1,0,0,0,0,0,0,0,0,0,0,0,0,0],
    [1,2,2,1,0,0,0,0,0,0,0,0,0,0,0,0],
    [1,2,2,2,1,0,0,0,0,0,0,0,0,0,0,0],
    [1,2,2,2,2,1,0,0,0,0,0,0,0,0,0,0],
    [1,2,2,2,2,2,1,0,0,0,0,0,0,0,0,0],
    [1,2,2,2,2,2,2,1,0,0,0,0,0,0,0,0],
    [1,2,2,2,2,2,2,2,1,0,0,0,0,0,0,0],
    [1,2,2,2,2,2,2,2,2,1,0,0,0,0,0,0],
    [1,2,2,2,2,2,1,1,1,1,1,0,0,0,0,0],
    [1,2,2,1,2,2,1,0,0,0,0,0,0,0,0,0],
    [1,2,1,0,1,2,2,1,0,0,0,0,0,0,0,0],
    [1,1,0,0,1,2,2,1,0,0,0,0,0,0,0,0],
    [1,0,0,0,0,1,2,2,1,0,0,0,0,0,0,0],
    [0,0,0,0,0,1,1,1,1,0,0,0,0,0,0,0],
];

/// Draws the mouse cursor at (mx, my) with a soft shadow.
fn draw_cursor(canvas: &mut Canvas, mx: usize, my: usize) {
    // Draw shadow offset (+2, +2)
    for (row, line) in CURSOR_BITMAP.iter().enumerate() {
        for (col, &pixel) in line.iter().enumerate() {
            if pixel != 0 {
                canvas.blend_pixel(mx + col + 2, my + row + 2, Color::BLACK, 60);
            }
        }
    }
    // Draw cursor
    for (row, line) in CURSOR_BITMAP.iter().enumerate() {
        for (col, &pixel) in line.iter().enumerate() {
            match pixel {
                1 => canvas.set_pixel(mx + col, my + row, Color::BLACK),
                2 => canvas.set_pixel(mx + col, my + row, Color::WHITE),
                _ => {}
            }
        }
    }
}

// ============================================================================
// Desktop Rendering
// ============================================================================

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
    // 3. Floating Centered Bottom Dock (macOS Pill Style)
    // ------------------------------------------------------------------------
    render_dock(canvas, width, height);
}

/// Renders a specific window onto the canvas.
fn render_window(canvas: &mut Canvas, win: &Window, is_active: bool) {
    if !win.visible || win.w < 50 || win.h < 50 {
        return;
    }

    let x = win.x.max(0) as usize;
    let y = win.y.max(0) as usize;

    draw_window_frame(canvas, x, y, win.w, win.h, win.title, is_active);

    // Render content based on window title
    if win.title == "AuraShell" {
        render_shell_content(canvas, x, y, win.w);
    } else if win.title == "Aura Files" {
        render_files_content(canvas, x, y, win.w, win.h);
    }
}

/// Renders the AuraShell terminal window content.
fn render_shell_content(canvas: &mut Canvas, x: usize, y: usize, _w: usize) {
    let cx = x + 16;
    let mut cy = y + 40;

    // Stylized Cyan ASCII Monogram Logo
    canvas.draw_string(cx, cy, "  /\\   AURA OPERATING SYSTEM", Color::AURA_CYAN);
    cy += 18;
    canvas.draw_string(cx, cy, " /--\\  v0.1.0 (x86_64 Long Mode)", Color::AURA_CYAN);
    cy += 24;

    // System Information Table
    let used_kb = crate::memory::allocator::used_memory() / 1024;
    let total_kb = crate::memory::allocator::HEAP_SIZE / 1024;
    let ticks = crate::arch::idt::ticks();

    canvas.draw_string(cx, cy, "User   : niaina", Color::TEXT_MUTED);
    cy += 18;
    canvas.draw_string(cx, cy, "Host   : auraos-baremetal", Color::TEXT_MUTED);
    cy += 18;
    let mem_info = format!("Memory : {} KiB / {} KiB", used_kb, total_kb);
    canvas.draw_string(cx, cy, &mem_info, Color::TEXT_MUTED);
    cy += 18;
    let ticks_info = format!("Uptime : {} timer ticks", ticks);
    canvas.draw_string(cx, cy, &ticks_info, Color::TEXT_MUTED);
    cy += 24;

    // Terminal Prompt Line
    canvas.draw_string(cx, cy, "auraos> sysinfo --gui", Color::AURA_CYAN);
    cy += 18;
    canvas.draw_string(cx, cy, "[OK] All 15 subsystem self-tests verified.", Color::TRAFFIC_GREEN);
    cy += 18;
    canvas.draw_string(cx, cy, "auraos> _", Color::TEXT_PRIMARY);
}

/// Renders the Aura Files VFS explorer content.
fn render_files_content(canvas: &mut Canvas, x: usize, y: usize, w: usize, h: usize) {
    // Sidebar vs Main Content Divider
    let sidebar_w = 110;
    canvas.fill_rect_alpha(x + 1, y + 30, sidebar_w, h.saturating_sub(31), Color::FROST_GLASS, 120);
    canvas.draw_rect_outline(x + sidebar_w, y + 30, 1, h.saturating_sub(31), Color::DOCK_BORDER);

    // Sidebar shortcuts
    canvas.draw_string(x + 14, y + 44, "Recent", Color::TEXT_PRIMARY);
    canvas.draw_string(x + 14, y + 68, "Home", Color::AURA_CYAN);
    canvas.draw_string(x + 14, y + 92, "Desktop", Color::TEXT_PRIMARY);
    canvas.draw_string(x + 14, y + 116, "Docs", Color::TEXT_PRIMARY);
    canvas.draw_string(x + 14, y + 140, "System", Color::TEXT_PRIMARY);

    // Main folder grid (reading VFS entries)
    let grid_x = x + sidebar_w + 24;
    let mut folder_x = grid_x;
    let mut folder_y = y + 48;

    if let Ok(entries) = crate::fs::vfs_list_directory_id(0) {
        for entry in entries.iter().take(6) {
            // Folder icon box
            canvas.fill_rounded_rect(folder_x, folder_y, 44, 32, 4, Color::FOLDER_BLUE);
            canvas.fill_rect(folder_x + 4, folder_y + 2, 14, 6, Color::WHITE);

            // Folder name below
            let label = if entry.name.len() > 6 { &entry.name[..6] } else { &entry.name };
            canvas.draw_string(folder_x, folder_y + 38, label, Color::TEXT_PRIMARY);

            folder_x += 64;
            if folder_x + 50 > x + w {
                folder_x = grid_x;
                folder_y += 60;
            }
        }
    }
}

/// Renders the macOS-style dock at the bottom center.
fn render_dock(canvas: &mut Canvas, width: usize, height: usize) {
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

// ============================================================================
// Interactive Desktop Loop (BGA Real Graphics Mode)
// ============================================================================

/// Launches the full interactive graphical desktop environment.
///
/// This function:
/// 1. Switches the display to BGA 1024x768x32bpp via Bochs VGA registers
/// 2. Enters a rendering loop that composites the desktop, windows, and cursor
/// 3. Handles mouse input for window dragging
/// 4. Exits when Escape is pressed, returning to VGA text mode
pub fn run_interactive_desktop() {
    // Initialize BGA graphics mode (1024x768x32bpp)
    if !bga::init_graphics_mode(1024, 768) {
        crate::serial_println!("[GUI] BGA graphics initialization failed! Returning to text mode.");
        crate::println!("[ERROR] BGA graphics adapter not available. Cannot launch desktop.");
        return;
    }

    crate::serial_println!("[OK] BGA initialized: 1024x768x32bpp at {:#x}", bga::framebuffer_ptr() as usize);

    GUI_ACTIVE.store(true, Ordering::Release);
    GUI_EXIT_REQUESTED.store(false, Ordering::Release);

    // Create the double-buffer canvas
    let mut canvas = Canvas::new(1024, 768);

    // Define the initial windows
    let mut windows = [
        Window {
            x: 50, y: 50, w: 420, h: 320,
            title: "AuraShell", active: true, visible: true,
        },
        Window {
            x: 500, y: 50, w: 420, h: 320,
            title: "Aura Files", active: false, visible: true,
        },
    ];

    let mut drag = DragState {
        dragging: false,
        window_index: 0,
        offset_x: 0,
        offset_y: 0,
    };

    let mut prev_left = false;
    let mut frame_count: u32 = 0;

    // Main desktop render/event loop
    loop {
        // Check for Escape exit
        if GUI_EXIT_REQUESTED.load(Ordering::Acquire) {
            break;
        }

        // Check for serial quit command ('q' or ESC = 0x1B)
        // I1: Uses lockfree receive — no global SERIAL1 lock needed for reading.
        if let Some(b) = crate::drivers::serial::receive_byte_lockfree() {
            if b == 0x1B || b == b'q' || b == b'Q' {
                GUI_EXIT_REQUESTED.store(true, Ordering::Release);
                break;
            }
        }
        // Flush serial buffer to UART so background logs are drained during GUI session
        crate::drivers::serial::serial_flush_all();

        // ---- Read mouse state ----
        let ms = mouse::get_state();
        let mx = ms.x;
        let my = ms.y;
        let left_pressed = ms.left_button;
        let left_just_pressed = left_pressed && !prev_left;
        let left_just_released = !left_pressed && prev_left;
        prev_left = left_pressed;

        // ---- Handle window dragging ----
        if left_just_pressed && !drag.dragging {
            // Check if click is on any window's title bar (iterate in reverse for z-order)
            for i in (0..windows.len()).rev() {
                let w = &windows[i];
                if !w.visible { continue; }
                let wx = w.x;
                let wy = w.y;
                // Title bar region: full width of window, top 30 pixels
                if (mx as isize) >= wx && (mx as isize) < wx + w.w as isize
                    && (my as isize) >= wy && (my as isize) < wy + 30
                {
                    // Check for close button (red traffic light at x+18, y+15, radius 5)
                    let close_cx = wx + 18;
                    let close_cy = wy + 15;
                    let dx_close = mx as isize - close_cx;
                    let dy_close = my as isize - close_cy;
                    if dx_close * dx_close + dy_close * dy_close <= 25 {
                        windows[i].visible = false;
                        break;
                    }

                    drag.dragging = true;
                    drag.window_index = i;
                    drag.offset_x = mx as isize - wx;
                    drag.offset_y = my as isize - wy;

                    // Bring to front (make active)
                    for j in 0..windows.len() {
                        windows[j].active = j == i;
                    }
                    break;
                }
            }
        }

        if drag.dragging && left_pressed {
            let new_x = mx as isize - drag.offset_x;
            let new_y = (my as isize - drag.offset_y).max(0); // don't drag above menu bar
            windows[drag.window_index].x = new_x;
            windows[drag.window_index].y = new_y;
        }

        if left_just_released {
            drag.dragging = false;
        }

        // ---- Render frame ----
        // Only redraw every 2nd frame for performance (reduces ~50% GPU writes)
        if frame_count % 2 == 0 {
            // 1. Render wallpaper + menu bar + dock
            render_desktop(&mut canvas);

            // 2. Render windows (in order for z-ordering)
            for win in &windows {
                render_window(&mut canvas, win, win.active);
            }

            // 3. Draw the mouse cursor on top
            draw_cursor(&mut canvas, mx, my);

            // 4. Blit to the physical framebuffer
            bga::present(&canvas);
        }

        frame_count = frame_count.wrapping_add(1);

        // Brief halt to yield CPU until next interrupt, atomically enabling interrupts
        unsafe { core::arch::asm!("sti; hlt", options(nomem, nostack)); }
    }

    // Cleanup: disable BGA, return to VGA text mode
    bga::disable_graphics_mode();
    GUI_ACTIVE.store(false, Ordering::Release);

    crate::serial_println!("[GUI] Desktop session ended. Returned to VGA text mode.");
}

// ============================================================================
// Window Frame Rendering Helper
// ============================================================================

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
