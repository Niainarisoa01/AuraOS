//! ============================================================================
//! AuraOS FAT32 Persistent Filesystem Driver
//! ============================================================================
//!
//! Pure Rust bare-metal FAT32 implementation providing real persistent
//! storage directly on ATA / IDE hard disk drives.
//!
//! Features:
//! - Standard BIOS Parameter Block (BPB) and FSInfo sector decoding.
//! - Dual File Allocation Tables (FAT1 & FAT2 mirroring).
//! - Cluster allocation, chain traversal, and deallocation.
//! - 32-byte Directory Entry parsing and creation (8.3 short names).
//! - Root directory and nested subdirectory traversal.
//! - File reading, writing, creation, and deletion with zero memory leaks.
//! - Volume formatting (`format`) and mounting (`mount`).

use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;
use crate::sync::Spinlock;
use crate::drivers::ata::{read_sector_drive, write_sector_drive, SECTOR_SIZE};

#[allow(dead_code)]
pub const ATTR_READ_ONLY: u8 = 0x01;
#[allow(dead_code)]
pub const ATTR_HIDDEN: u8 = 0x02;
#[allow(dead_code)]
pub const ATTR_SYSTEM: u8 = 0x04;
pub const ATTR_VOLUME_ID: u8 = 0x08;
pub const ATTR_DIRECTORY: u8 = 0x10;
pub const ATTR_ARCHIVE: u8 = 0x20;
pub const ATTR_LFN: u8 = 0x0F;

pub const FAT_FREE: u32 = 0x0000_0000;
pub const FAT_BAD: u32 = 0x0FFF_FFF7;
pub const FAT_EOC: u32 = 0x0FFF_FFFF;

/// 32-byte Directory Entry on disk
#[allow(dead_code)]
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct RawDirEntry {
    pub name: [u8; 8],
    pub ext: [u8; 3],
    pub attr: u8,
    pub reserved: u8,
    pub creation_time_tenths: u8,
    pub creation_time: u16,
    pub creation_date: u16,
    pub last_access_date: u16,
    pub cluster_high: u16,
    pub write_time: u16,
    pub write_date: u16,
    pub cluster_low: u16,
    pub file_size: u32,
}

/// High-level Directory Entry representation
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct FatDirEntry {
    pub name: String,
    pub is_directory: bool,
    pub first_cluster: u32,
    pub file_size: u32,
    pub attributes: u8,
}

/// Decoded BIOS Parameter Block (BPB)
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct Bpb {
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sectors: u16,
    pub num_fats: u8,
    pub total_sectors: u32,
    pub sectors_per_fat: u32,
    pub root_cluster: u32,
    pub fs_info_sector: u16,
    pub volume_id: u32,
    pub volume_label: [u8; 11],
}

/// Statistics and volume metrics
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Fat32Info {
    pub volume_label: String,
    pub total_sectors: u32,
    pub bytes_per_cluster: u32,
    pub total_clusters: u32,
    pub free_clusters: u32,
    pub total_space_kb: u64,
    pub free_space_kb: u64,
}

/// Main FAT32 File System State
pub struct Fat32Fs {
    pub drive: u8,
    pub lba_start: u32,
    pub bpb: Bpb,
    pub fat_start_lba: u32,
    pub data_start_lba: u32,
    pub total_clusters: u32,
}

impl Fat32Fs {
    /// Formats an ATA disk partition with a valid, clean FAT32 filesystem.
    pub fn format(
        drive: u8,
        lba_start: u32,
        total_sectors: u32,
        volume_label: &str,
    ) -> Result<Self, &'static str> {
        if total_sectors < 1024 {
            return Err("Volume too small for FAT32");
        }

        let bytes_per_sector = 512u16;
        let sectors_per_cluster = 1u8; // 512 bytes per cluster for optimal fine-grained storage
        let reserved_sectors = 32u16;
        let num_fats = 2u8;
        let root_cluster = 2u32;
        let fs_info_sector = 1u16;
        let volume_id = 0x2026_0909u32;

        // Compute FAT size: each cluster needs 4 bytes in each FAT
        let approx_data_sectors = total_sectors.saturating_sub(reserved_sectors as u32);
        let approx_clusters = approx_data_sectors / (sectors_per_cluster as u32);
        let fat_bytes = (approx_clusters + 2) * 4;
        let sectors_per_fat = ((fat_bytes + (SECTOR_SIZE as u32) - 1) / (SECTOR_SIZE as u32)).max(16);

        let data_start_lba = lba_start + (reserved_sectors as u32) + (num_fats as u32) * sectors_per_fat;
        if data_start_lba >= lba_start + total_sectors {
            return Err("Reserved & FAT sectors exceed volume size");
        }

        let data_sectors = (lba_start + total_sectors) - data_start_lba;
        let total_clusters = data_sectors / (sectors_per_cluster as u32);

        // Prepare Volume Label (11 bytes space-padded)
        let mut label_bytes = [b' '; 11];
        let label_src = volume_label.as_bytes();
        let copy_len = label_src.len().min(11);
        label_bytes[..copy_len].copy_from_slice(&label_src[..copy_len]);

        // 1. Write Sector 0: Boot Sector / BPB
        let mut bpb_sector = [0u8; SECTOR_SIZE];
        bpb_sector[0] = 0xEB; // JMP short 0x58
        bpb_sector[1] = 0x58;
        bpb_sector[2] = 0x90; // NOP
        bpb_sector[3..11].copy_from_slice(b"AURA_OS ");

        bpb_sector[11..13].copy_from_slice(&bytes_per_sector.to_le_bytes());
        bpb_sector[13] = sectors_per_cluster;
        bpb_sector[14..16].copy_from_slice(&reserved_sectors.to_le_bytes());
        bpb_sector[16] = num_fats;
        bpb_sector[17..19].copy_from_slice(&0u16.to_le_bytes()); // Root entry count = 0
        bpb_sector[19..21].copy_from_slice(&0u16.to_le_bytes()); // Total sectors 16 = 0
        bpb_sector[21] = 0xF8; // Media = Fixed disk
        bpb_sector[22..24].copy_from_slice(&0u16.to_le_bytes()); // Sectors per FAT 16 = 0
        bpb_sector[24..26].copy_from_slice(&63u16.to_le_bytes()); // Sectors per track
        bpb_sector[26..28].copy_from_slice(&255u16.to_le_bytes()); // Heads
        bpb_sector[28..32].copy_from_slice(&lba_start.to_le_bytes()); // Hidden sectors
        bpb_sector[32..36].copy_from_slice(&total_sectors.to_le_bytes()); // Total sectors 32

        // FAT32 Extended BPB
        bpb_sector[36..40].copy_from_slice(&sectors_per_fat.to_le_bytes());
        bpb_sector[40..42].copy_from_slice(&0u16.to_le_bytes()); // Ext flags
        bpb_sector[42..44].copy_from_slice(&0u16.to_le_bytes()); // FS Version
        bpb_sector[44..48].copy_from_slice(&root_cluster.to_le_bytes());
        bpb_sector[48..50].copy_from_slice(&fs_info_sector.to_le_bytes());
        bpb_sector[50..52].copy_from_slice(&6u16.to_le_bytes()); // Backup boot sector
        bpb_sector[64] = 0x80; // Drive number
        bpb_sector[66] = 0x29; // Extended boot signature
        bpb_sector[67..71].copy_from_slice(&volume_id.to_le_bytes());
        bpb_sector[71..82].copy_from_slice(&label_bytes);
        bpb_sector[82..90].copy_from_slice(b"FAT32   ");

        bpb_sector[510] = 0x55;
        bpb_sector[511] = 0xAA;

        write_sector_drive(drive, lba_start, &bpb_sector)?;

        // 2. Write Sector 1: FSInfo Sector
        let mut fsinfo = [0u8; SECTOR_SIZE];
        fsinfo[0..4].copy_from_slice(&0x41615252u32.to_le_bytes()); // "RRaA"
        fsinfo[484..488].copy_from_slice(&0x61417272u32.to_le_bytes()); // "rrAa"
        let free_clusters_init = total_clusters.saturating_sub(1); // root cluster allocated
        fsinfo[488..492].copy_from_slice(&free_clusters_init.to_le_bytes());
        fsinfo[492..496].copy_from_slice(&3u32.to_le_bytes()); // Next free cluster hint = 3
        fsinfo[508..512].copy_from_slice(&0xAA550000u32.to_le_bytes());

        write_sector_drive(drive, lba_start + (fs_info_sector as u32), &fsinfo)?;

        // 3. Initialize FAT 1 and FAT 2
        let fat1_lba = lba_start + (reserved_sectors as u32);
        let fat2_lba = fat1_lba + sectors_per_fat;

        // Sector 0 of FAT contains clusters 0, 1, 2
        let mut fat_sec0 = [0u8; SECTOR_SIZE];
        fat_sec0[0..4].copy_from_slice(&0x0FFF_FFF8u32.to_le_bytes());
        fat_sec0[4..8].copy_from_slice(&FAT_EOC.to_le_bytes());
        fat_sec0[8..12].copy_from_slice(&FAT_EOC.to_le_bytes());

        write_sector_drive(drive, fat1_lba, &fat_sec0)?;
        write_sector_drive(drive, fat2_lba, &fat_sec0)?;

        // Zero-fill remaining FAT sectors
        let zero_sector = [0u8; SECTOR_SIZE];
        for s in 1..sectors_per_fat {
            write_sector_drive(drive, fat1_lba + s, &zero_sector)?;
            write_sector_drive(drive, fat2_lba + s, &zero_sector)?;
        }

        // 4. Initialize Root Directory Cluster (Cluster 2)
        let mut root_sec = [0u8; SECTOR_SIZE];
        root_sec[0..11].copy_from_slice(&label_bytes);
        root_sec[11] = ATTR_VOLUME_ID;
        write_sector_drive(drive, data_start_lba, &root_sec)?;

        for s in 1..(sectors_per_cluster as u32) {
            write_sector_drive(drive, data_start_lba + s, &zero_sector)?;
        }

        let bpb = Bpb {
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sectors,
            num_fats,
            total_sectors,
            sectors_per_fat,
            root_cluster,
            fs_info_sector,
            volume_id,
            volume_label: label_bytes,
        };

        Ok(Self {
            drive,
            lba_start,
            bpb,
            fat_start_lba: fat1_lba,
            data_start_lba,
            total_clusters,
        })
    }

    /// Mounts an existing FAT32 filesystem from the given LBA start sector.
    pub fn mount(drive: u8, lba_start: u32) -> Result<Self, &'static str> {
        let mut bpb_sec = [0u8; SECTOR_SIZE];
        read_sector_drive(drive, lba_start, &mut bpb_sec)?;

        // Check boot signature 0x55, 0xAA
        if bpb_sec[510] != 0x55 || bpb_sec[511] != 0xAA {
            return Err("Invalid boot sector signature (missing 0x55, 0xAA)");
        }

        // Check FAT32 type string at byte 82
        if &bpb_sec[82..87] != b"FAT32" {
            return Err("Not a valid FAT32 filesystem (missing FAT32 signature)");
        }

        let bytes_per_sector = u16::from_le_bytes([bpb_sec[11], bpb_sec[12]]);
        if bytes_per_sector != 512 {
            return Err("Unsupported sector size (only 512 bytes supported)");
        }

        let sectors_per_cluster = bpb_sec[13];
        if sectors_per_cluster == 0 {
            return Err("Invalid sectors per cluster (0)");
        }

        let reserved_sectors = u16::from_le_bytes([bpb_sec[14], bpb_sec[15]]);
        let num_fats = bpb_sec[16];
        let total_sectors = u32::from_le_bytes([bpb_sec[32], bpb_sec[33], bpb_sec[34], bpb_sec[35]]);
        let sectors_per_fat = u32::from_le_bytes([bpb_sec[36], bpb_sec[37], bpb_sec[38], bpb_sec[39]]);
        let root_cluster = u32::from_le_bytes([bpb_sec[44], bpb_sec[45], bpb_sec[46], bpb_sec[47]]);
        let fs_info_sector = u16::from_le_bytes([bpb_sec[48], bpb_sec[49]]);
        let volume_id = u32::from_le_bytes([bpb_sec[67], bpb_sec[68], bpb_sec[69], bpb_sec[70]]);

        let mut volume_label = [0u8; 11];
        volume_label.copy_from_slice(&bpb_sec[71..82]);

        let fat_start_lba = lba_start + (reserved_sectors as u32);
        let data_start_lba = fat_start_lba + (num_fats as u32) * sectors_per_fat;

        let data_sectors = total_sectors.saturating_sub(data_start_lba.saturating_sub(lba_start));
        let total_clusters = data_sectors / (sectors_per_cluster as u32);

        let bpb = Bpb {
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sectors,
            num_fats,
            total_sectors,
            sectors_per_fat,
            root_cluster,
            fs_info_sector,
            volume_id,
            volume_label,
        };

        Ok(Self {
            drive,
            lba_start,
            bpb,
            fat_start_lba,
            data_start_lba,
            total_clusters,
        })
    }

    /// Converts a cluster number (>= 2) to its physical LBA on the disk.
    #[inline]
    pub fn cluster_to_lba(&self, cluster: u32) -> u32 {
        self.data_start_lba + (cluster.saturating_sub(2)) * (self.bpb.sectors_per_cluster as u32)
    }

    /// Reads the next cluster pointer in the FAT table for the given cluster.
    pub fn read_fat_entry(&self, cluster: u32) -> Result<u32, &'static str> {
        let fat_offset = cluster * 4;
        let fat_sector = self.fat_start_lba + (fat_offset / (SECTOR_SIZE as u32));
        let offset_in_sec = (fat_offset % (SECTOR_SIZE as u32)) as usize;

        let mut buf = [0u8; SECTOR_SIZE];
        read_sector_drive(self.drive, fat_sector, &mut buf)?;

        let val = u32::from_le_bytes([
            buf[offset_in_sec],
            buf[offset_in_sec + 1],
            buf[offset_in_sec + 2],
            buf[offset_in_sec + 3],
        ]) & 0x0FFF_FFFF;

        Ok(val)
    }

    /// Writes a cluster pointer into both FAT 1 and FAT 2 (mirroring).
    pub fn write_fat_entry(&self, cluster: u32, value: u32) -> Result<(), &'static str> {
        let fat_offset = cluster * 4;
        let sector_offset = fat_offset / (SECTOR_SIZE as u32);
        let byte_offset = (fat_offset % (SECTOR_SIZE as u32)) as usize;

        for fat_idx in 0..(self.bpb.num_fats as u32) {
            let fat_lba = self.fat_start_lba + fat_idx * self.bpb.sectors_per_fat + sector_offset;
            let mut buf = [0u8; SECTOR_SIZE];
            read_sector_drive(self.drive, fat_lba, &mut buf)?;

            let old_top = buf[byte_offset + 3] & 0xF0;
            let val_bytes = value.to_le_bytes();
            buf[byte_offset] = val_bytes[0];
            buf[byte_offset + 1] = val_bytes[1];
            buf[byte_offset + 2] = val_bytes[2];
            buf[byte_offset + 3] = (val_bytes[3] & 0x0F) | old_top;

            write_sector_drive(self.drive, fat_lba, &buf)?;
        }

        Ok(())
    }

    /// Allocates a free cluster from the FAT, zero-initializes its sectors, and links it.
    pub fn allocate_cluster(&mut self, prev_cluster: Option<u32>) -> Result<u32, &'static str> {
        for c in 3..self.total_clusters {
            if self.read_fat_entry(c)? == FAT_FREE {
                self.write_fat_entry(c, FAT_EOC)?;

                if let Some(prev) = prev_cluster {
                    self.write_fat_entry(prev, c)?;
                }

                let lba = self.cluster_to_lba(c);
                let zero_buf = [0u8; SECTOR_SIZE];
                for s in 0..(self.bpb.sectors_per_cluster as u32) {
                    write_sector_drive(self.drive, lba + s, &zero_buf)?;
                }

                return Ok(c);
            }
        }
        Err("FAT32: No free clusters available (disk full)")
    }

    /// Traverses and frees an entire chain of clusters in the FAT.
    pub fn free_cluster_chain(&mut self, mut cluster: u32) -> Result<(), &'static str> {
        while cluster >= 2 && cluster < 0x0FFF_FFF7 {
            let next = self.read_fat_entry(cluster)?;
            self.write_fat_entry(cluster, FAT_FREE)?;
            cluster = next;
        }
        Ok(())
    }

    /// Reads all directory entries contained in the cluster chain of a directory.
    pub fn list_directory_cluster(&self, mut cluster: u32) -> Result<Vec<FatDirEntry>, &'static str> {
        let mut entries = Vec::new();

        while cluster >= 2 && cluster < 0x0FFF_FFF7 {
            let lba = self.cluster_to_lba(cluster);
            for s in 0..(self.bpb.sectors_per_cluster as u32) {
                let mut buf = [0u8; SECTOR_SIZE];
                read_sector_drive(self.drive, lba + s, &mut buf)?;

                for i in 0..(SECTOR_SIZE / 32) {
                    let offset = i * 32;
                    let first_byte = buf[offset];

                    if first_byte == 0x00 {
                        return Ok(entries);
                    }
                    if first_byte == 0xE5 {
                        continue;
                    }

                    let attr = buf[offset + 11];
                    if attr == ATTR_LFN || (attr & ATTR_VOLUME_ID != 0 && attr & ATTR_DIRECTORY == 0) {
                        continue;
                    }

                    let raw_name = &buf[offset..offset + 8];
                    let raw_ext = &buf[offset + 8..offset + 11];

                    let name_str = core::str::from_utf8(raw_name).unwrap_or("").trim_end();
                    let ext_str = core::str::from_utf8(raw_ext).unwrap_or("").trim_end();

                    let full_name = if ext_str.is_empty() {
                        String::from(name_str)
                    } else {
                        format!("{}.{}", name_str, ext_str)
                    };

                    let cluster_high = u16::from_le_bytes([buf[offset + 20], buf[offset + 21]]);
                    let cluster_low = u16::from_le_bytes([buf[offset + 26], buf[offset + 27]]);
                    let first_cluster = ((cluster_high as u32) << 16) | (cluster_low as u32);
                    let file_size = u32::from_le_bytes([
                        buf[offset + 28],
                        buf[offset + 29],
                        buf[offset + 30],
                        buf[offset + 31],
                    ]);

                    entries.push(FatDirEntry {
                        name: full_name,
                        is_directory: (attr & ATTR_DIRECTORY) != 0,
                        first_cluster,
                        file_size,
                        attributes: attr,
                    });
                }
            }

            cluster = self.read_fat_entry(cluster)?;
        }

        Ok(entries)
    }

    /// Finds an entry in a directory by name (case-insensitive) and returns entry + (sector_lba, entry_index).
    fn find_entry_in_dir(
        &self,
        mut cluster: u32,
        name: &str,
    ) -> Result<Option<(FatDirEntry, u32, usize)>, &'static str> {
        let name_upper = name.to_ascii_uppercase();

        while cluster >= 2 && cluster < 0x0FFF_FFF7 {
            let lba = self.cluster_to_lba(cluster);
            for s in 0..(self.bpb.sectors_per_cluster as u32) {
                let sec_lba = lba + s;
                let mut buf = [0u8; SECTOR_SIZE];
                read_sector_drive(self.drive, sec_lba, &mut buf)?;

                for i in 0..(SECTOR_SIZE / 32) {
                    let offset = i * 32;
                    let first_byte = buf[offset];

                    if first_byte == 0x00 {
                        return Ok(None);
                    }
                    if first_byte == 0xE5 {
                        continue;
                    }

                    let attr = buf[offset + 11];
                    if attr == ATTR_LFN || (attr & ATTR_VOLUME_ID != 0 && attr & ATTR_DIRECTORY == 0) {
                        continue;
                    }

                    let raw_name = &buf[offset..offset + 8];
                    let raw_ext = &buf[offset + 8..offset + 11];

                    let name_str = core::str::from_utf8(raw_name).unwrap_or("").trim_end();
                    let ext_str = core::str::from_utf8(raw_ext).unwrap_or("").trim_end();

                    let full_name = if ext_str.is_empty() {
                        String::from(name_str)
                    } else {
                        format!("{}.{}", name_str, ext_str)
                    };

                    if full_name.to_ascii_uppercase() == name_upper {
                        let cluster_high = u16::from_le_bytes([buf[offset + 20], buf[offset + 21]]);
                        let cluster_low = u16::from_le_bytes([buf[offset + 26], buf[offset + 27]]);
                        let first_cluster = ((cluster_high as u32) << 16) | (cluster_low as u32);
                        let file_size = u32::from_le_bytes([
                            buf[offset + 28],
                            buf[offset + 29],
                            buf[offset + 30],
                            buf[offset + 31],
                        ]);

                        return Ok(Some((
                            FatDirEntry {
                                name: full_name,
                                is_directory: (attr & ATTR_DIRECTORY) != 0,
                                first_cluster,
                                file_size,
                                attributes: attr,
                            },
                            sec_lba,
                            i,
                        )));
                    }
                }
            }
            cluster = self.read_fat_entry(cluster)?;
        }

        Ok(None)
    }

    /// Resolves a path (e.g. `/`, `/DIR`, `/DIR/FILE.TXT`) and returns the target entry and parent cluster.
    pub fn resolve_path(&self, path: &str) -> Result<(Option<FatDirEntry>, u32), &'static str> {
        let trimmed = path.trim_matches('/');
        if trimmed.is_empty() {
            let root_entry = FatDirEntry {
                name: String::from("/"),
                is_directory: true,
                first_cluster: self.bpb.root_cluster,
                file_size: 0,
                attributes: ATTR_DIRECTORY,
            };
            return Ok((Some(root_entry), self.bpb.root_cluster));
        }

        let segments: Vec<&str> = trimmed.split('/').filter(|s| !s.is_empty()).collect();
        let mut curr_cluster = self.bpb.root_cluster;

        for (idx, &segment) in segments.iter().enumerate() {
            let is_last = idx == segments.len() - 1;
            let found = self.find_entry_in_dir(curr_cluster, segment)?;

            match found {
                Some((entry, _, _)) => {
                    if is_last {
                        return Ok((Some(entry), curr_cluster));
                    }
                    if !entry.is_directory {
                        return Err("Component in path is not a directory");
                    }
                    curr_cluster = entry.first_cluster;
                }
                None => {
                    if is_last {
                        return Ok((None, curr_cluster));
                    } else {
                        return Err("Directory in path not found");
                    }
                }
            }
        }

        Err("Failed to resolve path")
    }

    /// Reads the entire content of a file given its path.
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, &'static str> {
        let (entry_opt, _) = self.resolve_path(path)?;
        let entry = entry_opt.ok_or("File not found")?;

        if entry.is_directory {
            return Err("Cannot read directory as file");
        }

        let mut data = Vec::with_capacity(entry.file_size as usize);
        let mut cluster = entry.first_cluster;
        let mut bytes_left = entry.file_size as usize;

        while cluster >= 2 && cluster < 0x0FFF_FFF7 && bytes_left > 0 {
            let lba = self.cluster_to_lba(cluster);
            for s in 0..(self.bpb.sectors_per_cluster as u32) {
                if bytes_left == 0 {
                    break;
                }

                let mut buf = [0u8; SECTOR_SIZE];
                read_sector_drive(self.drive, lba + s, &mut buf)?;

                let chunk = bytes_left.min(SECTOR_SIZE);
                data.extend_from_slice(&buf[..chunk]);
                bytes_left -= chunk;
            }
            cluster = self.read_fat_entry(cluster)?;
        }

        Ok(data)
    }

    /// Converts a filename string to FAT 8.3 formatted bytes [name: 8, ext: 3].
    fn to_short_name(name: &str) -> ([u8; 8], [u8; 3]) {
        let mut name_bytes = [b' '; 8];
        let mut ext_bytes = [b' '; 3];

        let upper = name.to_ascii_uppercase();
        let parts: Vec<&str> = upper.split('.').collect();

        let base = parts[0].as_bytes();
        let copy_base = base.len().min(8);
        name_bytes[..copy_base].copy_from_slice(&base[..copy_base]);

        if parts.len() > 1 {
            let ext = parts[1].as_bytes();
            let copy_ext = ext.len().min(3);
            ext_bytes[..copy_ext].copy_from_slice(&ext[..copy_ext]);
        }

        (name_bytes, ext_bytes)
    }

    /// Writes data to a file, creating it if it doesn't exist or overwriting it if it does.
    pub fn write_file(&mut self, path: &str, data: &[u8]) -> Result<(), &'static str> {
        let trimmed = path.trim_matches('/');
        let (parent_dir, filename) = match trimmed.rfind('/') {
            Some(pos) => (&trimmed[..pos], &trimmed[pos + 1..]),
            None => ("", trimmed),
        };

        if filename.is_empty() {
            return Err("Invalid empty filename");
        }

        let (parent_entry_opt, _) = self.resolve_path(parent_dir)?;
        let parent_cluster = match parent_entry_opt {
            Some(e) if e.is_directory => e.first_cluster,
            Some(_) => return Err("Parent path is not a directory"),
            None => return Err("Parent directory not found"),
        };

        let existing = self.find_entry_in_dir(parent_cluster, filename)?;

        let bytes_per_cluster = (self.bpb.sectors_per_cluster as usize) * SECTOR_SIZE;
        let clusters_needed = ((data.len() + bytes_per_cluster - 1) / bytes_per_cluster).max(1);

        let mut first_cluster = 0u32;
        let mut prev_cluster: Option<u32> = None;

        for _ in 0..clusters_needed {
            let c = self.allocate_cluster(prev_cluster)?;
            if first_cluster == 0 {
                first_cluster = c;
            }
            prev_cluster = Some(c);
        }

        let mut curr_c = first_cluster;
        let mut written = 0usize;

        while curr_c >= 2 && curr_c < 0x0FFF_FFF7 && written < data.len() {
            let lba = self.cluster_to_lba(curr_c);
            for s in 0..(self.bpb.sectors_per_cluster as u32) {
                if written >= data.len() {
                    break;
                }

                let mut sec_buf = [0u8; SECTOR_SIZE];
                let to_copy = (data.len() - written).min(SECTOR_SIZE);
                sec_buf[..to_copy].copy_from_slice(&data[written..written + to_copy]);
                write_sector_drive(self.drive, lba + s, &sec_buf)?;
                written += to_copy;
            }
            curr_c = self.read_fat_entry(curr_c)?;
        }

        let (s_name, s_ext) = Self::to_short_name(filename);
        let cluster_high = ((first_cluster >> 16) & 0xFFFF) as u16;
        let cluster_low = (first_cluster & 0xFFFF) as u16;
        let file_size = data.len() as u32;

        if let Some((old_entry, sec_lba, entry_idx)) = existing {
            self.free_cluster_chain(old_entry.first_cluster)?;

            let mut sec_buf = [0u8; SECTOR_SIZE];
            read_sector_drive(self.drive, sec_lba, &mut sec_buf)?;

            let offset = entry_idx * 32;
            sec_buf[offset + 20..offset + 22].copy_from_slice(&cluster_high.to_le_bytes());
            sec_buf[offset + 26..offset + 28].copy_from_slice(&cluster_low.to_le_bytes());
            sec_buf[offset + 28..offset + 32].copy_from_slice(&file_size.to_le_bytes());

            write_sector_drive(self.drive, sec_lba, &sec_buf)?;
        } else {
            self.insert_dir_entry(
                parent_cluster,
                s_name,
                s_ext,
                ATTR_ARCHIVE,
                first_cluster,
                file_size,
            )?;
        }

        Ok(())
    }

    /// Creates a new subdirectory in the filesystem.
    pub fn create_dir(&mut self, path: &str) -> Result<u32, &'static str> {
        let trimmed = path.trim_matches('/');
        let (parent_dir, dirname) = match trimmed.rfind('/') {
            Some(pos) => (&trimmed[..pos], &trimmed[pos + 1..]),
            None => ("", trimmed),
        };

        if dirname.is_empty() {
            return Err("Invalid empty directory name");
        }

        let (parent_entry_opt, _) = self.resolve_path(parent_dir)?;
        let parent_cluster = match parent_entry_opt {
            Some(e) if e.is_directory => e.first_cluster,
            Some(_) => return Err("Parent path is not a directory"),
            None => return Err("Parent directory not found"),
        };

        if self.find_entry_in_dir(parent_cluster, dirname)?.is_some() {
            return Err("Directory already exists");
        }

        let new_cluster = self.allocate_cluster(None)?;
        let new_lba = self.cluster_to_lba(new_cluster);

        let mut sec_buf = [0u8; SECTOR_SIZE];

        // 1. Entry "."
        sec_buf[0..11].copy_from_slice(b".          ");
        sec_buf[11] = ATTR_DIRECTORY;
        let ch_high = ((new_cluster >> 16) & 0xFFFF) as u16;
        let ch_low = (new_cluster & 0xFFFF) as u16;
        sec_buf[20..22].copy_from_slice(&ch_high.to_le_bytes());
        sec_buf[26..28].copy_from_slice(&ch_low.to_le_bytes());

        // 2. Entry ".."
        sec_buf[32..43].copy_from_slice(b"..         ");
        sec_buf[43] = ATTR_DIRECTORY;
        let parent_cluster_target = if parent_cluster == self.bpb.root_cluster { 0 } else { parent_cluster };
        let pch_high = ((parent_cluster_target >> 16) & 0xFFFF) as u16;
        let pch_low = (parent_cluster_target & 0xFFFF) as u16;
        sec_buf[52..54].copy_from_slice(&pch_high.to_le_bytes());
        sec_buf[58..60].copy_from_slice(&pch_low.to_le_bytes());

        write_sector_drive(self.drive, new_lba, &sec_buf)?;

        let (s_name, s_ext) = Self::to_short_name(dirname);
        self.insert_dir_entry(parent_cluster, s_name, s_ext, ATTR_DIRECTORY, new_cluster, 0)?;

        Ok(new_cluster)
    }

    /// Inserts a 32-byte directory entry into the first free slot of a directory cluster chain.
    fn insert_dir_entry(
        &mut self,
        mut cluster: u32,
        name: [u8; 8],
        ext: [u8; 3],
        attr: u8,
        first_cluster: u32,
        file_size: u32,
    ) -> Result<(), &'static str> {
        let ch_high = ((first_cluster >> 16) & 0xFFFF) as u16;
        let ch_low = (first_cluster & 0xFFFF) as u16;

        let mut entry_bytes = [0u8; 32];
        entry_bytes[0..8].copy_from_slice(&name);
        entry_bytes[8..11].copy_from_slice(&ext);
        entry_bytes[11] = attr;
        entry_bytes[20..22].copy_from_slice(&ch_high.to_le_bytes());
        entry_bytes[26..28].copy_from_slice(&ch_low.to_le_bytes());
        entry_bytes[28..32].copy_from_slice(&file_size.to_le_bytes());

        while cluster >= 2 && cluster < 0x0FFF_FFF7 {
            let lba = self.cluster_to_lba(cluster);
            for s in 0..(self.bpb.sectors_per_cluster as u32) {
                let sec_lba = lba + s;
                let mut buf = [0u8; SECTOR_SIZE];
                read_sector_drive(self.drive, sec_lba, &mut buf)?;

                for i in 0..(SECTOR_SIZE / 32) {
                    let offset = i * 32;
                    let first_b = buf[offset];

                    if first_b == 0x00 || first_b == 0xE5 {
                        buf[offset..offset + 32].copy_from_slice(&entry_bytes);
                        write_sector_drive(self.drive, sec_lba, &buf)?;
                        return Ok(());
                    }
                }
            }

            let next = self.read_fat_entry(cluster)?;
            if next >= 0x0FFF_FFF7 {
                let new_c = self.allocate_cluster(Some(cluster))?;
                let new_lba = self.cluster_to_lba(new_c);
                let mut new_sec = [0u8; SECTOR_SIZE];
                new_sec[0..32].copy_from_slice(&entry_bytes);
                write_sector_drive(self.drive, new_lba, &new_sec)?;
                return Ok(());
            }
            cluster = next;
        }

        Err("FAT32: Directory slot allocation failure")
    }

    /// Deletes a file or directory entry and frees its associated cluster chain.
    pub fn delete_entry(&mut self, path: &str) -> Result<(), &'static str> {
        let trimmed = path.trim_matches('/');
        let (parent_dir, filename) = match trimmed.rfind('/') {
            Some(pos) => (&trimmed[..pos], &trimmed[pos + 1..]),
            None => ("", trimmed),
        };

        let (parent_entry_opt, _) = self.resolve_path(parent_dir)?;
        let parent_cluster = match parent_entry_opt {
            Some(e) if e.is_directory => e.first_cluster,
            Some(_) => return Err("Parent path is not a directory"),
            None => return Err("Parent directory not found"),
        };

        let entry_info = self.find_entry_in_dir(parent_cluster, filename)?;
        let (entry, sec_lba, entry_idx) = entry_info.ok_or("File not found")?;

        self.free_cluster_chain(entry.first_cluster)?;

        let mut buf = [0u8; SECTOR_SIZE];
        read_sector_drive(self.drive, sec_lba, &mut buf)?;
        buf[entry_idx * 32] = 0xE5;
        write_sector_drive(self.drive, sec_lba, &buf)?;

        Ok(())
    }

    /// Gathers volume statistics and storage capacity.
    pub fn fs_info(&self) -> Fat32Info {
        let mut free_clusters = 0u32;
        for c in 2..self.total_clusters {
            if let Ok(entry) = self.read_fat_entry(c) {
                if entry == FAT_FREE {
                    free_clusters += 1;
                }
            }
        }

        let bytes_per_cluster = (self.bpb.sectors_per_cluster as u32) * (SECTOR_SIZE as u32);
        let total_space_kb = ((self.total_clusters as u64) * (bytes_per_cluster as u64)) / 1024;
        let free_space_kb = ((free_clusters as u64) * (bytes_per_cluster as u64)) / 1024;

        let label = core::str::from_utf8(&self.bpb.volume_label).unwrap_or("AURAOS").trim();

        Fat32Info {
            volume_label: String::from(label),
            total_sectors: self.bpb.total_sectors,
            bytes_per_cluster,
            total_clusters: self.total_clusters,
            free_clusters,
            total_space_kb,
            free_space_kb,
        }
    }
}

/// Global system-wide mounted FAT32 filesystem instance.
pub static FAT32_FS: Spinlock<Option<Fat32Fs>> = Spinlock::new(None);
