//! ============================================================================
//! Kernel Error Types
//! ============================================================================
//!
//! Typed error enums replacing ad-hoc `&'static str` return values across the
//! VFS, ATA storage driver, and FAT32 filesystem. Each enum implements
//! `Display` so existing call sites that print errors continue to work
//! unchanged, and `From<&'static str>` eases the mechanical migration of
//! legacy `Err("message")` bodies.

use core::fmt;

/// Errors returned by the Virtual File System (VFS / RAMFS) layer.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsError {
    NotFound,
    NotADirectory,
    AlreadyExists,
    IsDirectory,
    NotAFile,
    PermissionDenied,
    DiskFull,
    BadPath,
    RootProtected,
    Deleted,
}

impl fmt::Display for VfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            VfsError::NotFound => "not found",
            VfsError::NotADirectory => "not a directory",
            VfsError::AlreadyExists => "already exists",
            VfsError::IsDirectory => "is a directory",
            VfsError::NotAFile => "not a file",
            VfsError::PermissionDenied => "permission denied",
            VfsError::DiskFull => "disk full",
            VfsError::BadPath => "invalid path",
            VfsError::RootProtected => "cannot modify root",
            VfsError::Deleted => "entry has been deleted",
        };
        f.write_str(msg)
    }
}

/// Maps legacy `&'static str` error messages onto typed variants.
impl From<&'static str> for VfsError {
    fn from(msg: &'static str) -> Self {
        match msg {
            "File or directory not found" | "Entry not found in parent" | "Path not found" => {
                VfsError::NotFound
            }
            "Not a directory in path" | "Parent is not a directory" | "Not a directory" => {
                VfsError::NotADirectory
            }
            "Entry already exists" => VfsError::AlreadyExists,
            "Target exists and is a directory" | "Cannot read directory as file" => {
                VfsError::IsDirectory
            }
            "Cannot modify root" => VfsError::RootProtected,
            "Cannot read deleted file" | "Operation on a deleted file" => VfsError::Deleted,
            _ => VfsError::NotFound,
        }
    }
}

/// Errors returned by the ATA/IDE storage driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtaError {
    DriveNotReady,
    DriveBusy,
    InvalidLba,
    DriveError,
    NoDrive,
}

impl fmt::Display for AtaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            AtaError::DriveNotReady => "drive not ready",
            AtaError::DriveBusy => "drive busy",
            AtaError::InvalidLba => "invalid LBA",
            AtaError::DriveError => "drive error",
            AtaError::NoDrive => "no such drive",
        };
        f.write_str(msg)
    }
}

/// Maps legacy `&'static str` ATA error messages onto typed variants.
impl From<&'static str> for AtaError {
    fn from(msg: &'static str) -> Self {
        match msg {
            "Drive not ready" | "Timeout waiting for drive" => AtaError::DriveNotReady,
            "Drive busy" | "Drive still busy" | "Drive not ready after command" => {
                AtaError::DriveBusy
            }
            "Invalid LBA" | "LBA out of range" => AtaError::InvalidLba,
            _ => AtaError::DriveError,
        }
    }
}

/// Errors returned by the FAT32 filesystem layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fat32Error {
    NotFound,
    BadSignature,
    Corrupt,
    Io(AtaError),
    InvalidName,
    NotADirectory,
    AlreadyExists,
}

impl fmt::Display for Fat32Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fat32Error::NotFound => f.write_str("not found (FAT32)"),
            Fat32Error::BadSignature => f.write_str("bad FAT32 signature"),
            Fat32Error::Corrupt => f.write_str("corrupt FAT32 structures"),
            Fat32Error::Io(e) => write!(f, "I/O error: {}", e),
            Fat32Error::InvalidName => f.write_str("invalid FAT32 name"),
            Fat32Error::NotADirectory => f.write_str("not a directory (FAT32)"),
            Fat32Error::AlreadyExists => f.write_str("already exists (FAT32)"),
        }
    }
}

/// Maps legacy `&'static str` FAT32 error messages onto typed variants.
impl From<&'static str> for Fat32Error {
    fn from(msg: &'static str) -> Self {
        match msg {
            "Entry not found" | "Path not found" | "File not found" | "Invalid path" => {
                Fat32Error::NotFound
            }
            "Bad signature" | "Invalid boot sector signature" => Fat32Error::BadSignature,
            "Not a directory" | "Not a directory in path" => Fat32Error::NotADirectory,
            "Entry already exists" | "Name already exists" | "Directory already exists" => {
                Fat32Error::AlreadyExists
            }
            "Invalid name" | "Name too long" => Fat32Error::InvalidName,
            "Corrupt FAT" | "Corrupt directory" => Fat32Error::Corrupt,
            _ => Fat32Error::Corrupt,
        }
    }
}

/// Converts a lower-level ATA I/O error into a FAT32 I/O error.
impl From<AtaError> for Fat32Error {
    fn from(e: AtaError) -> Self {
        Fat32Error::Io(e)
    }
}