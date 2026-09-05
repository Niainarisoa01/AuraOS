use core::fmt;
use core::ptr::write_volatile;
use core::sync::atomic::{AtomicBool, Ordering};

/// Les 16 couleurs standard de la palette VGA x86.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

/// Combine la couleur du texte (avant-plan) et la couleur du fond (arrière-plan).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct ColorCode(u8);

impl ColorCode {
    pub const fn new(foreground: Color, background: Color) -> ColorCode {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }
}

/// Représentation d'un caractère VGA à l'écran (2 octets : ASCII + Attribut couleur).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}

/// Dimensions de l'écran VGA texte standard.
const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;

/// Tampon mémoire VGA pointant vers l'adresse matérielle `0xb8000`.
#[repr(transparent)]
struct Buffer {
    chars: [[ScreenChar; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

/// Le pilote d'écriture VGA qui gère le curseur, les couleurs et le défilement.
pub struct Writer {
    column_position: usize,
    color_code: ColorCode,
    buffer: *mut Buffer,
}

unsafe impl Send for Writer {}

impl Writer {
    /// Accès sécurisé au tampon matériel à l'exécution.
    #[inline]
    fn buffer_mut(&mut self) -> &mut Buffer {
        unsafe { &mut *self.buffer }
    }

    /// Écrit un seul octet (caractère ASCII ou saut de ligne).
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.column_position >= BUFFER_WIDTH {
                    self.new_line();
                }

                let row = BUFFER_HEIGHT - 1;
                let col = self.column_position;
                let color_code = self.color_code;

                unsafe {
                    write_volatile(
                        &mut self.buffer_mut().chars[row][col],
                        ScreenChar {
                            ascii_character: byte,
                            color_code,
                        },
                    );
                }
                self.column_position += 1;
            }
        }
    }

    /// Écrit une chaîne de caractères complète.
    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                _ => self.write_byte(0xfe),
            }
        }
    }

    /// Déplace toutes les lignes vers le haut d'un cran (Scrolling automatique).
    fn new_line(&mut self) {
        for row in 1..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                let character = self.buffer_mut().chars[row][col];
                unsafe {
                    write_volatile(&mut self.buffer_mut().chars[row - 1][col], character);
                }
            }
        }
        self.clear_row(BUFFER_HEIGHT - 1);
        self.column_position = 0;
    }

    /// Efface une ligne en la remplissant d'espaces vides.
    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii_character: b' ',
            color_code: self.color_code,
        };
        for col in 0..BUFFER_WIDTH {
            unsafe {
                write_volatile(&mut self.buffer_mut().chars[row][col], blank);
            }
        }
    }

    /// Efface l'ensemble de l'écran VGA et remet le curseur au début.
    pub fn clear_screen(&mut self) {
        for row in 0..BUFFER_HEIGHT {
            self.clear_row(row);
        }
        self.column_position = 0;
    }

    /// Efface le dernier caractère saisi (touche Retour arrière / Backspace).
    pub fn backspace(&mut self) {
        if self.column_position > 0 {
            self.column_position -= 1;
            let row = BUFFER_HEIGHT - 1;
            let col = self.column_position;
            let blank = ScreenChar {
                ascii_character: b' ',
                color_code: self.color_code,
            };
            unsafe {
                write_volatile(&mut self.buffer_mut().chars[row][col], blank);
            }
        }
    }

    /// Change la couleur d'écriture courante.
    #[allow(dead_code)]
    pub fn set_color(&mut self, foreground: Color, background: Color) {
        self.color_code = ColorCode::new(foreground, background);
    }
}

/// Permet à notre `Writer` d'utiliser le formattage standard Rust (ex: `write!`, `format_args!`).
impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

// ============================================================================
// Verrou Bare-Metal (Spinlock) pour la sécurité thread-safe / interruptions
// ============================================================================

pub struct Spinlock<T> {
    lock: AtomicBool,
    data: core::cell::UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for Spinlock<T> {}

impl<T> Spinlock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            lock: AtomicBool::new(false),
            data: core::cell::UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> SpinlockGuard<'_, T> {
        while self.lock.swap(true, Ordering::Acquire) {
            core::hint::spin_loop();
        }
        SpinlockGuard { spinlock: self }
    }
}

pub struct SpinlockGuard<'a, T> {
    spinlock: &'a Spinlock<T>,
}

impl<'a, T> core::ops::Deref for SpinlockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.spinlock.data.get() }
    }
}

impl<'a, T> core::ops::DerefMut for SpinlockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.spinlock.data.get() }
    }
}

impl<'a, T> Drop for SpinlockGuard<'a, T> {
    fn drop(&mut self) {
        self.spinlock.lock.store(false, Ordering::Release);
    }
}

/// Instance globale et unique du pilote d'affichage VGA, protégée par Spinlock.
pub static WRITER: Spinlock<Writer> = Spinlock::new(Writer {
    column_position: 0,
    color_code: ColorCode::new(Color::LightCyan, Color::Black),
    buffer: 0xb8000 as *mut Buffer,
});

// ============================================================================
// Macros d'impression globales : print! et println! pour le noyau
// ============================================================================

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::vga_buffer::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    WRITER.lock().write_fmt(args).unwrap();
}

/// Supprime le dernier caractère affiché à l'écran.
pub fn backspace() {
    WRITER.lock().backspace();
}

/// Efface complètement l'écran VGA.
pub fn clear_screen() {
    WRITER.lock().clear_screen();
}
