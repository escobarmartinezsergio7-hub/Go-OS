use alloc::string::String;
use alloc::vec::Vec;
use crate::fs::{DirEntry, FileType, FileSystem};
use crate::virtio::block;
use uefi::boot::{self, OpenProtocolAttributes, OpenProtocolParams};
use uefi::proto::media::block::BlockIO;
use uefi::proto::loaded_image::LoadedImage;
use uefi::Handle;

const SECTOR_SIZE: usize = 512;
const MAX_UEFI_BLOCK_SIZE: usize = 4096;
const FAT32_COPY_IO_MIN_BYTES: usize = 64 * 1024;
const FAT32_COPY_IO_REMOVABLE_BYTES: usize = 256 * 1024;
const FAT32_COPY_IO_MAX_BYTES: usize = 1024 * 1024;
const FAT32_EOC: u32 = 0x0FFF_FFFF;
const FAT32_DIR_ATTR_LFN: u8 = 0x0F;

// FAT32 Boot Sector Structure
#[repr(C, packed)]
struct BootSector {
    jmp: [u8; 3],
    oem: [u8; 8],
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    fats: u8,
    root_entries: u16,
    total_sectors_16: u16,
    media: u8,
    sectors_per_fat_16: u16,
    sectors_per_track: u16,
    heads: u16,
    hidden_sectors: u32,
    total_sectors_32: u32,
    // FAT32 Extended
    sectors_per_fat_32: u32,
    flags: u16,
    version: u16,
    root_cluster: u32,
    fs_info: u16,
    backup_boot: u16,
    reserved: [u8; 12],
    drive_number: u8,
    reserved2: u8,
    signature: u8,
    vol_id: u32,
    label: [u8; 11],
    fs_type: [u8; 8],
}

// FAT32 Directory Entry
#[repr(C, packed)]
struct FatDirEntry {
    name: [u8; 11],
    attr: u8,
    nt_res: u8,
    create_time_tenth: u8,
    create_time: u16,
    create_date: u16,
    last_access_date: u16,
    cluster_high: u16,
    write_time: u16,
    write_date: u16,
    cluster_low: u16,
    size: u32,
}

impl Clone for FatDirEntry {
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            attr: self.attr,
            nt_res: self.nt_res,
            create_time_tenth: self.create_time_tenth,
            create_time: self.create_time,
            create_date: self.create_date,
            last_access_date: self.last_access_date,
            cluster_high: self.cluster_high,
            write_time: self.write_time,
            write_date: self.write_date,
            cluster_low: self.cluster_low,
            size: self.size,
        }
    }
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct FatLongDirEntry {
    order: u8,
    name1: [u16; 5],
    attr: u8,
    entry_type: u8,
    checksum: u8,
    name2: [u16; 6],
    zero: u16,
    name3: [u16; 2],
}

#[derive(PartialEq, Clone, Copy)]
pub enum InitStatus {
    Uninitialized,
    InProgress,
    Success,
    Failed,
}

pub struct Fat32 {
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sectors: u16,
    pub fats: u8,
    pub sectors_per_fat: u32,
    pub root_cluster: u32,
    pub partition_start: u64,
    pub fat_start: u64,
    pub data_start: u64,
    pub volume_label: [u8; 11],
    pub init_status: InitStatus,
    pub uefi_block_handle: Option<Handle>,
    pub next_free_cluster_hint: u32,
}

pub static mut GLOBAL_FAT: Fat32 = Fat32 {
    bytes_per_sector: 0,
    sectors_per_cluster: 0,
    reserved_sectors: 0,
    fats: 0,
    sectors_per_fat: 0,
    root_cluster: 0,
    partition_start: 0,
    fat_start: 0,
    data_start: 0,
    volume_label: [0; 11],
    init_status: InitStatus::Uninitialized,
    uefi_block_handle: None,
    next_free_cluster_hint: 2,
};

#[derive(Clone, Copy)]
struct ProbeResult {
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    fats: u8,
    sectors_per_fat: u32,
    root_cluster: u32,
    partition_start: u64,
    fat_start: u64,
    data_start: u64,
    volume_label: [u8; 11],
}

#[derive(Clone, Copy)]
struct UefiVolumeCandidate {
    handle: Handle,
    probe: ProbeResult,
    removable: bool,
    logical_partition: bool,
    total_mib: u64,
}

#[derive(Clone, Copy)]
struct UefiBlockDeviceCandidate {
    handle: Handle,
    removable: bool,
    logical_partition: bool,
    total_mib: u64,
}

#[derive(Clone, Copy)]
pub struct DetectedVolume {
    pub index: usize,
    pub volume_label: [u8; 11],
    pub partition_start: u64,
    pub root_cluster: u32,
    pub removable: bool,
    pub logical_partition: bool,
    pub total_mib: u64,
}

#[derive(Clone, Copy)]
pub struct DetectedBlockDevice {
    pub index: usize,
    pub removable: bool,
    pub logical_partition: bool,
    pub total_mib: u64,
    pub fat_volume_index: Option<usize>,
    pub fat_volume_label: [u8; 11],
}

#[repr(align(4096))]
struct AlignedBlock([u8; MAX_UEFI_BLOCK_SIZE]);

#[repr(align(4096))]
struct CopyIoBuffer([u8; FAT32_COPY_IO_MAX_BYTES]);

static mut FAT32_COPY_IO_BUFFER: CopyIoBuffer = CopyIoBuffer([0u8; FAT32_COPY_IO_MAX_BYTES]);

impl Fat32 {
    pub const fn new() -> Self {
        Self {
            bytes_per_sector: 0,
            sectors_per_cluster: 0,
            reserved_sectors: 0,
            fats: 0,
            sectors_per_fat: 0,
            root_cluster: 0,
            partition_start: 0,
            fat_start: 0,
            data_start: 0,
            volume_label: [0; 11],
            init_status: InitStatus::Uninitialized,
            uefi_block_handle: None,
            next_free_cluster_hint: 2,
        }
    }

    pub fn unmount(&mut self) {
        self.bytes_per_sector = 0;
        self.sectors_per_cluster = 0;
        self.reserved_sectors = 0;
        self.fats = 0;
        self.sectors_per_fat = 0;
        self.root_cluster = 0;
        self.partition_start = 0;
        self.fat_start = 0;
        self.data_start = 0;
        self.volume_label = [0; 11];
        self.init_status = InitStatus::Uninitialized;
        self.uefi_block_handle = None;
        self.next_free_cluster_hint = 2;
    }

    fn cluster_to_lba(&self, cluster: u32) -> u64 {
        let data_start_sector = self.data_start;
        // Calculation: DataStart + (Cluster - 2) * SectorsPerCluster
        // Note: Cluster 2 is the first data cluster.
        data_start_sector + ((cluster as u64 - 2) * self.sectors_per_cluster as u64)
    }

    fn read_sector_virtio_or_nvme(&self, lba: u64, buffer: &mut [u8]) -> bool {
        // Try VirtIO first
        if block::read(lba, buffer) {
            return true;
        }
        // Fallback to NVMe
        if crate::nvme::read(lba, buffer) {
            return true;
        }
        false
    }

    fn write_sector_virtio_or_nvme(&self, lba: u64, buffer: &[u8]) -> bool {
        if buffer.len() < SECTOR_SIZE {
            return false;
        }

        // Write support exists on VirtIO. NVMe write path is not implemented yet.
        block::write(lba, &buffer[0..SECTOR_SIZE])
    }

    fn read_sector_from_uefi_handle(handle: Handle, lba: u64, buffer: &mut [u8]) -> bool {
        if buffer.len() < SECTOR_SIZE {
            return false;
        }

        let params = OpenProtocolParams {
            handle,
            agent: boot::image_handle(),
            controller: None,
        };

        let blk = match unsafe {
            boot::open_protocol::<BlockIO>(params, OpenProtocolAttributes::GetProtocol)
        } {
            Ok(p) => p,
            Err(_) => return false,
        };

        let (media_id, last_block, block_size) = {
            let media = blk.media();
            if !media.is_media_present() {
                return false;
            }
            (
                media.media_id(),
                media.last_block(),
                media.block_size() as usize,
            )
        };

        if block_size < SECTOR_SIZE
            || block_size > MAX_UEFI_BLOCK_SIZE
            || (block_size % SECTOR_SIZE) != 0
        {
            return false;
        }

        let byte_offset = match lba.checked_mul(SECTOR_SIZE as u64) {
            Some(v) => v,
            None => return false,
        };
        let block_lba = byte_offset / block_size as u64;
        let offset = (byte_offset % block_size as u64) as usize;

        if block_lba > last_block {
            return false;
        }

        let mut scratch = AlignedBlock([0u8; MAX_UEFI_BLOCK_SIZE]);
        if blk
            .read_blocks(
                media_id,
                block_lba,
                &mut scratch.0[0..block_size],
            )
            .is_err()
        {
            return false;
        }

        buffer[0..SECTOR_SIZE].copy_from_slice(&scratch.0[offset..offset + SECTOR_SIZE]);
        true
    }

    fn write_sector_from_uefi_handle(handle: Handle, lba: u64, buffer: &[u8]) -> bool {
        if buffer.len() < SECTOR_SIZE {
            return false;
        }

        let params = OpenProtocolParams {
            handle,
            agent: boot::image_handle(),
            controller: None,
        };

        let mut blk = match unsafe {
            boot::open_protocol::<BlockIO>(params, OpenProtocolAttributes::GetProtocol)
        } {
            Ok(p) => p,
            Err(_) => return false,
        };

        let (media_id, last_block, block_size) = {
            let media = blk.media();
            if !media.is_media_present() {
                return false;
            }
            (
                media.media_id(),
                media.last_block(),
                media.block_size() as usize,
            )
        };

        if block_size < SECTOR_SIZE
            || block_size > MAX_UEFI_BLOCK_SIZE
            || (block_size % SECTOR_SIZE) != 0
        {
            return false;
        }

        let byte_offset = match lba.checked_mul(SECTOR_SIZE as u64) {
            Some(v) => v,
            None => return false,
        };
        let block_lba = byte_offset / block_size as u64;
        let offset = (byte_offset % block_size as u64) as usize;

        if block_lba > last_block {
            return false;
        }

        let mut scratch = AlignedBlock([0u8; MAX_UEFI_BLOCK_SIZE]);

        // For sub-block writes, preserve untouched bytes via read-modify-write.
        if block_size != SECTOR_SIZE || offset != 0 {
            if blk
                .read_blocks(
                    media_id,
                    block_lba,
                    &mut scratch.0[0..block_size],
                )
                .is_err()
            {
                return false;
            }
        }

        scratch.0[offset..offset + SECTOR_SIZE].copy_from_slice(&buffer[0..SECTOR_SIZE]);

        blk.write_blocks(media_id, block_lba, &scratch.0[0..block_size])
            .is_ok()
    }

    // Read 512-byte logical sectors from the active storage source.
    fn read_sector(&self, lba: u64, buffer: &mut [u8]) -> bool {
        if let Some(handle) = self.uefi_block_handle {
            if Self::read_sector_from_uefi_handle(handle, lba, buffer) {
                return true;
            }
        }

        self.read_sector_virtio_or_nvme(lba, buffer)
    }

    // Write one 512-byte logical sector to the active storage source.
    fn write_sector(&self, lba: u64, buffer: &[u8]) -> bool {
        if let Some(handle) = self.uefi_block_handle {
            if Self::write_sector_from_uefi_handle(handle, lba, buffer) {
                return true;
            }
        }

        self.write_sector_virtio_or_nvme(lba, buffer)
    }

    fn read_sector_span_from_uefi_handle(
        handle: Handle,
        lba: u64,
        sectors: usize,
        buffer: &mut [u8],
    ) -> bool {
        if sectors == 0 {
            return true;
        }
        let total_bytes = match sectors.checked_mul(SECTOR_SIZE) {
            Some(v) => v,
            None => return false,
        };
        if buffer.len() < total_bytes {
            return false;
        }

        let params = OpenProtocolParams {
            handle,
            agent: boot::image_handle(),
            controller: None,
        };

        let blk = match unsafe {
            boot::open_protocol::<BlockIO>(params, OpenProtocolAttributes::GetProtocol)
        } {
            Ok(p) => p,
            Err(_) => return false,
        };

        let (media_id, last_block, block_size) = {
            let media = blk.media();
            if !media.is_media_present() {
                return false;
            }
            (
                media.media_id(),
                media.last_block(),
                media.block_size() as usize,
            )
        };

        if block_size < SECTOR_SIZE
            || block_size > MAX_UEFI_BLOCK_SIZE
            || (block_size % SECTOR_SIZE) != 0
        {
            return false;
        }

        let start_byte = match lba.checked_mul(SECTOR_SIZE as u64) {
            Some(v) => v,
            None => return false,
        };

        // Fast path: fully block-aligned range in one firmware call.
        if (start_byte % block_size as u64) == 0 && (total_bytes % block_size) == 0 {
            let start_block = start_byte / block_size as u64;
            let blocks = total_bytes / block_size;
            if blocks > 0 {
                let end_block = match start_block.checked_add(blocks as u64 - 1) {
                    Some(v) => v,
                    None => return false,
                };
                if end_block <= last_block
                    && blk
                        .read_blocks(media_id, start_block, &mut buffer[..total_bytes])
                        .is_ok()
                {
                    return true;
                }
            }
        }

        // Fallback path: block-wise reads with sub-range extraction.
        let mut scratch = AlignedBlock([0u8; MAX_UEFI_BLOCK_SIZE]);
        let mut remaining = total_bytes;
        let mut dst_off = 0usize;
        let mut cur_byte = start_byte;

        while remaining > 0 {
            let block_lba = cur_byte / block_size as u64;
            if block_lba > last_block {
                return false;
            }

            if blk
                .read_blocks(media_id, block_lba, &mut scratch.0[0..block_size])
                .is_err()
            {
                return false;
            }

            let offset = (cur_byte % block_size as u64) as usize;
            let take = core::cmp::min(remaining, block_size - offset);
            buffer[dst_off..dst_off + take].copy_from_slice(&scratch.0[offset..offset + take]);

            cur_byte = match cur_byte.checked_add(take as u64) {
                Some(v) => v,
                None => return false,
            };
            dst_off += take;
            remaining -= take;
        }

        true
    }

    fn write_sector_span_from_uefi_handle(
        handle: Handle,
        lba: u64,
        sectors: usize,
        buffer: &[u8],
    ) -> bool {
        if sectors == 0 {
            return true;
        }
        let total_bytes = match sectors.checked_mul(SECTOR_SIZE) {
            Some(v) => v,
            None => return false,
        };
        if buffer.len() < total_bytes {
            return false;
        }

        let params = OpenProtocolParams {
            handle,
            agent: boot::image_handle(),
            controller: None,
        };

        let mut blk = match unsafe {
            boot::open_protocol::<BlockIO>(params, OpenProtocolAttributes::GetProtocol)
        } {
            Ok(p) => p,
            Err(_) => return false,
        };

        let (media_id, last_block, block_size) = {
            let media = blk.media();
            if !media.is_media_present() {
                return false;
            }
            (
                media.media_id(),
                media.last_block(),
                media.block_size() as usize,
            )
        };

        if block_size < SECTOR_SIZE
            || block_size > MAX_UEFI_BLOCK_SIZE
            || (block_size % SECTOR_SIZE) != 0
        {
            return false;
        }

        let start_byte = match lba.checked_mul(SECTOR_SIZE as u64) {
            Some(v) => v,
            None => return false,
        };

        // Fast path: fully block-aligned range in one firmware call.
        if (start_byte % block_size as u64) == 0 && (total_bytes % block_size) == 0 {
            let start_block = start_byte / block_size as u64;
            let blocks = total_bytes / block_size;
            if blocks > 0 {
                let end_block = match start_block.checked_add(blocks as u64 - 1) {
                    Some(v) => v,
                    None => return false,
                };
                if end_block <= last_block
                    && blk
                        .write_blocks(media_id, start_block, &buffer[..total_bytes])
                        .is_ok()
                {
                    return true;
                }
            }
        }

        // Fallback path: read-modify-write for partial firmware blocks.
        let mut scratch = AlignedBlock([0u8; MAX_UEFI_BLOCK_SIZE]);
        let mut remaining = total_bytes;
        let mut src_off = 0usize;
        let mut cur_byte = start_byte;

        while remaining > 0 {
            let block_lba = cur_byte / block_size as u64;
            if block_lba > last_block {
                return false;
            }
            let offset = (cur_byte % block_size as u64) as usize;
            let take = core::cmp::min(remaining, block_size - offset);

            if offset == 0 && take == block_size {
                scratch.0[0..block_size].copy_from_slice(&buffer[src_off..src_off + block_size]);
            } else {
                if blk
                    .read_blocks(media_id, block_lba, &mut scratch.0[0..block_size])
                    .is_err()
                {
                    return false;
                }
                scratch.0[offset..offset + take].copy_from_slice(&buffer[src_off..src_off + take]);
            }

            if blk
                .write_blocks(media_id, block_lba, &scratch.0[0..block_size])
                .is_err()
            {
                return false;
            }

            cur_byte = match cur_byte.checked_add(take as u64) {
                Some(v) => v,
                None => return false,
            };
            src_off += take;
            remaining -= take;
        }

        true
    }

    fn read_sector_span(&self, lba: u64, sectors: usize, buffer: &mut [u8]) -> bool {
        if sectors == 0 {
            return true;
        }
        let total_bytes = match sectors.checked_mul(SECTOR_SIZE) {
            Some(v) => v,
            None => return false,
        };
        if buffer.len() < total_bytes {
            return false;
        }

        if let Some(handle) = self.uefi_block_handle {
            if Self::read_sector_span_from_uefi_handle(handle, lba, sectors, &mut buffer[..total_bytes]) {
                return true;
            }
        }

        let mut i = 0usize;
        while i < sectors {
            let off = i * SECTOR_SIZE;
            if !self.read_sector(lba + i as u64, &mut buffer[off..off + SECTOR_SIZE]) {
                return false;
            }
            i += 1;
        }

        true
    }

    fn write_sector_span(&self, lba: u64, sectors: usize, buffer: &[u8]) -> bool {
        if sectors == 0 {
            return true;
        }
        let total_bytes = match sectors.checked_mul(SECTOR_SIZE) {
            Some(v) => v,
            None => return false,
        };
        if buffer.len() < total_bytes {
            return false;
        }

        if let Some(handle) = self.uefi_block_handle {
            if Self::write_sector_span_from_uefi_handle(handle, lba, sectors, &buffer[..total_bytes]) {
                return true;
            }
        }

        let mut i = 0usize;
        while i < sectors {
            let off = i * SECTOR_SIZE;
            if !self.write_sector(lba + i as u64, &buffer[off..off + SECTOR_SIZE]) {
                return false;
            }
            i += 1;
        }

        true
    }

    fn uefi_handle_copy_profile(handle: Handle) -> Option<(bool, usize)> {
        let params = OpenProtocolParams {
            handle,
            agent: boot::image_handle(),
            controller: None,
        };

        let blk = unsafe {
            boot::open_protocol::<BlockIO>(params, OpenProtocolAttributes::GetProtocol)
        }
        .ok()?;

        let media = blk.media();
        if !media.is_media_present() {
            return None;
        }

        let block_size = media.block_size() as usize;
        if block_size < SECTOR_SIZE
            || block_size > MAX_UEFI_BLOCK_SIZE
            || (block_size % SECTOR_SIZE) != 0
        {
            return None;
        }

        Some((media.is_removable_media(), block_size))
    }

    fn recommended_copy_io_bytes(src_fat: &Fat32, dst_fat: &Fat32) -> usize {
        let src_profile = src_fat
            .uefi_block_handle
            .and_then(Self::uefi_handle_copy_profile);
        let dst_profile = dst_fat
            .uefi_block_handle
            .and_then(Self::uefi_handle_copy_profile);

        let mut bytes = match (src_profile, dst_profile) {
            (Some((src_removable, _)), Some((dst_removable, _))) => {
                if src_removable || dst_removable {
                    FAT32_COPY_IO_REMOVABLE_BYTES
                } else {
                    FAT32_COPY_IO_MAX_BYTES
                }
            }
            _ => FAT32_COPY_IO_MIN_BYTES,
        };

        let mut align = SECTOR_SIZE;
        if let Some((_, bs)) = src_profile {
            align = core::cmp::max(align, bs);
        }
        if let Some((_, bs)) = dst_profile {
            align = core::cmp::max(align, bs);
        }

        bytes = bytes.clamp(FAT32_COPY_IO_MIN_BYTES, FAT32_COPY_IO_MAX_BYTES);
        let aligned = (bytes / align).saturating_mul(align);
        if aligned >= FAT32_COPY_IO_MIN_BYTES {
            bytes = aligned;
        }

        // Keep logical-sector alignment for all backends.
        let logical_aligned = (bytes / SECTOR_SIZE).max(1) * SECTOR_SIZE;
        logical_aligned.clamp(FAT32_COPY_IO_MIN_BYTES, FAT32_COPY_IO_MAX_BYTES)
    }

    fn is_supported_partition_type(p_type: u8) -> bool {
        p_type == 0x0B || p_type == 0x0C || p_type == 0x0E
    }

    fn parse_bpb(sector: &[u8; SECTOR_SIZE], partition_start: u64) -> Option<ProbeResult> {
        if sector[510] != 0x55 || sector[511] != 0xAA {
            return None;
        }

        let bpb = unsafe { &*(sector.as_ptr() as *const BootSector) };
        if bpb.bytes_per_sector != 512
            || bpb.sectors_per_cluster == 0
            || bpb.reserved_sectors == 0
            || bpb.fats == 0
            || bpb.fats > 2
            || bpb.sectors_per_fat_32 == 0
            || bpb.root_cluster < 2
        {
            return None;
        }

        let fat_start = partition_start + bpb.reserved_sectors as u64;
        let data_start = fat_start + (bpb.fats as u64 * bpb.sectors_per_fat_32 as u64);

        Some(ProbeResult {
            bytes_per_sector: bpb.bytes_per_sector,
            sectors_per_cluster: bpb.sectors_per_cluster,
            reserved_sectors: bpb.reserved_sectors,
            fats: bpb.fats,
            sectors_per_fat: bpb.sectors_per_fat_32,
            root_cluster: bpb.root_cluster,
            partition_start,
            fat_start,
            data_start,
            volume_label: bpb.label,
        })
    }

    fn probe_with_reader<F>(mut read_sector: F) -> Option<ProbeResult>
    where
        F: FnMut(u64, &mut [u8]) -> bool,
    {
        let mut sector0 = [0u8; SECTOR_SIZE];
        if !read_sector(0, &mut sector0) {
            return None;
        }

        // Direct BPB on LBA0 (partition handle or superfloppy image).
        if let Some(found) = Self::parse_bpb(&sector0, 0) {
            return Some(found);
        }

        // Otherwise, inspect MBR partition entries.
        if sector0[510] != 0x55 || sector0[511] != 0xAA {
            return None;
        }

        for i in 0..4 {
            let offset = 446 + (i * 16);
            let p_type = sector0[offset + 4];
            if !Self::is_supported_partition_type(p_type) {
                continue;
            }

            let mut lba_bytes = [0u8; 4];
            lba_bytes.copy_from_slice(&sector0[offset + 8..offset + 12]);
            let partition_lba = u32::from_le_bytes(lba_bytes) as u64;
            if partition_lba == 0 {
                continue;
            }

            let mut part_sector = [0u8; SECTOR_SIZE];
            if !read_sector(partition_lba, &mut part_sector) {
                continue;
            }

            if let Some(found) = Self::parse_bpb(&part_sector, partition_lba) {
                return Some(found);
            }
        }

        None
    }

    fn apply_probe_result(&mut self, found: ProbeResult) {
        self.partition_start = found.partition_start;
        self.bytes_per_sector = found.bytes_per_sector;
        self.sectors_per_cluster = found.sectors_per_cluster;
        self.reserved_sectors = found.reserved_sectors;
        self.fats = found.fats;
        self.sectors_per_fat = found.sectors_per_fat;
        self.root_cluster = found.root_cluster;
        self.volume_label = found.volume_label;
        self.fat_start = found.fat_start;
        self.data_start = found.data_start;
        self.next_free_cluster_hint = 2;
    }

    fn blockio_priority(is_removable: bool, is_partition: bool) -> u8 {
        match (is_removable, is_partition) {
            (true, false) => 0,  // USB disk/device handle
            (true, true) => 1,   // USB partition handle
            (false, false) => 2, // fixed disk/device
            (false, true) => 3,  // fixed disk partition
        }
    }

    fn boot_device_handle() -> Option<Handle> {
        let loaded = match boot::open_protocol_exclusive::<LoadedImage>(boot::image_handle()) {
            Ok(v) => v,
            Err(_) => return None,
        };
        loaded.device()
    }

    fn probe_candidate_as_fat(device: UefiBlockDeviceCandidate) -> Option<UefiVolumeCandidate> {
        let probe =
            Self::probe_with_reader(|lba, buf| Self::read_sector_from_uefi_handle(device.handle, lba, buf))?;

        Some(UefiVolumeCandidate {
            handle: device.handle,
            probe,
            removable: device.removable,
            logical_partition: device.logical_partition,
            total_mib: device.total_mib,
        })
    }

    fn scan_uefi_block_devices() -> Vec<UefiBlockDeviceCandidate> {
        let mut out = Vec::new();
        let handle_buf = boot::find_handles::<BlockIO>().unwrap_or_default();
        if handle_buf.is_empty() {
            return out;
        }

        // Keep handle order stable across scans so UI device indexes remain consistent.
        let mut handles: Vec<Handle> = handle_buf.iter().copied().collect();
        handles.sort_unstable();

        for priority in 0u8..=3 {
            for handle in handles.iter().copied() {
                let params = OpenProtocolParams {
                    handle,
                    agent: boot::image_handle(),
                    controller: None,
                };

                let blk = match unsafe {
                    boot::open_protocol::<BlockIO>(params, OpenProtocolAttributes::GetProtocol)
                } {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                let media = blk.media();
                if !media.is_media_present() {
                    continue;
                }

                let removable = media.is_removable_media();
                let logical_partition = media.is_logical_partition();
                let this_priority = Self::blockio_priority(removable, logical_partition);
                if this_priority != priority {
                    continue;
                }

                let block_size = media.block_size() as usize;
                if block_size < SECTOR_SIZE
                    || block_size > MAX_UEFI_BLOCK_SIZE
                    || (block_size % SECTOR_SIZE) != 0
                {
                    continue;
                }

                let total_mib = media
                    .last_block()
                    .saturating_add(1)
                    .saturating_mul(block_size as u64)
                    / (1024 * 1024);

                out.push(UefiBlockDeviceCandidate {
                    handle,
                    removable,
                    logical_partition,
                    total_mib,
                });
            }
        }

        out
    }

    fn scan_uefi_fat_volumes() -> Vec<UefiVolumeCandidate> {
        let mut out = Vec::new();
        let devices = Self::scan_uefi_block_devices();
        if devices.is_empty() {
            return out;
        }

        // Probe boot device first.
        if let Some(boot_handle) = Self::boot_device_handle() {
            let mut i = 0usize;
            while i < devices.len() {
                let d = devices[i];
                if d.handle == boot_handle {
                    if let Some(found) = Self::probe_candidate_as_fat(d) {
                        out.push(found);
                    }
                    break;
                }
                i += 1;
            }
        }

        // Then probe logical partition handles only (safer than raw disk handles).
        let mut i = 0usize;
        while i < devices.len() {
            let d = devices[i];
            if d.logical_partition && !out.iter().any(|v| v.handle == d.handle) {
                if let Some(found) = Self::probe_candidate_as_fat(d) {
                    out.push(found);
                }
            }
            i += 1;
        }

        // Fallback for superfloppy USB media without partition handles.
        if out.is_empty() {
            let mut j = 0usize;
            while j < devices.len() {
                let d = devices[j];
                if d.removable && !d.logical_partition {
                    if let Some(found) = Self::probe_candidate_as_fat(d) {
                        out.push(found);
                    }
                }
                j += 1;
            }
        }

        out
    }

    fn try_init_from_boot_device(&mut self) -> bool {
        let loaded = match boot::open_protocol_exclusive::<LoadedImage>(boot::image_handle()) {
            Ok(v) => v,
            Err(_) => return false,
        };

        let handle = match loaded.device() {
            Some(h) => h,
            None => return false,
        };

        if let Some(found) =
            Self::probe_with_reader(|lba, buf| Self::read_sector_from_uefi_handle(handle, lba, buf))
        {
            self.apply_probe_result(found);
            self.uefi_block_handle = Some(handle);
            return true;
        }

        false
    }

    fn try_init_via_uefi_blockio(&mut self) -> bool {
        let candidates = Self::scan_uefi_fat_volumes();
        if candidates.is_empty() {
            return false;
        }

        let selected = candidates[0];
        self.apply_probe_result(selected.probe);
        self.uefi_block_handle = Some(selected.handle);
        true
    }

    pub fn detect_uefi_fat_volumes() -> Vec<DetectedVolume> {
        let candidates = Self::scan_uefi_fat_volumes();
        let mut out = Vec::new();
        let mut i = 0usize;
        while i < candidates.len() {
            let c = candidates[i];
            out.push(DetectedVolume {
                index: i,
                volume_label: c.probe.volume_label,
                partition_start: c.probe.partition_start,
                root_cluster: c.probe.root_cluster,
                removable: c.removable,
                logical_partition: c.logical_partition,
                total_mib: c.total_mib,
            });
            i += 1;
        }
        out
    }

    pub fn detect_uefi_block_devices() -> Vec<DetectedBlockDevice> {
        let devices = Self::scan_uefi_block_devices();
        let mut out = Vec::new();

        let mut i = 0usize;
        while i < devices.len() {
            let d = devices[i];
            out.push(DetectedBlockDevice {
                index: i,
                removable: d.removable,
                logical_partition: d.logical_partition,
                total_mib: d.total_mib,
                fat_volume_index: None,
                fat_volume_label: [0u8; 11],
            });

            i += 1;
        }

        out
    }

    pub fn boot_block_device_index() -> Option<usize> {
        let boot_handle = Self::boot_device_handle()?;
        let devices = Self::scan_uefi_block_devices();
        let mut i = 0usize;
        while i < devices.len() {
            if devices[i].handle == boot_handle {
                return Some(i);
            }
            i += 1;
        }
        None
    }

    pub fn mount_uefi_block_device(&mut self, device_index: usize) -> Result<DetectedVolume, &'static str> {
        let devices = Self::scan_uefi_block_devices();
        if devices.is_empty() {
            self.init_status = InitStatus::Failed;
            return Err("NO UEFI BLOCK DEVICES DETECTED.");
        }
        if device_index >= devices.len() {
            return Err("DEVICE INDEX OUT OF RANGE.");
        }

        let device = devices[device_index];
        let Some(selected) = Self::probe_candidate_as_fat(device) else {
            return Err("SELECTED DEVICE IS NOT A MOUNTABLE FAT32 VOLUME.");
        };

        self.apply_probe_result(selected.probe);
        self.uefi_block_handle = Some(selected.handle);
        self.init_status = InitStatus::Success;

        Ok(DetectedVolume {
            index: device_index,
            volume_label: selected.probe.volume_label,
            partition_start: selected.probe.partition_start,
            root_cluster: selected.probe.root_cluster,
            removable: selected.removable,
            logical_partition: selected.logical_partition,
            total_mib: selected.total_mib,
        })
    }

    pub fn mount_uefi_fat_volume(&mut self, index: usize) -> Result<DetectedVolume, &'static str> {
        let candidates = Self::scan_uefi_fat_volumes();
        if candidates.is_empty() {
            self.init_status = InitStatus::Failed;
            return Err("NO FAT32 VOLUMES DETECTED.");
        }
        if index >= candidates.len() {
            return Err("VOLUME INDEX OUT OF RANGE.");
        }

        let selected = candidates[index];
        self.apply_probe_result(selected.probe);
        self.uefi_block_handle = Some(selected.handle);
        self.init_status = InitStatus::Success;

        Ok(DetectedVolume {
            index,
            volume_label: selected.probe.volume_label,
            partition_start: selected.probe.partition_start,
            root_cluster: selected.probe.root_cluster,
            removable: selected.removable,
            logical_partition: selected.logical_partition,
            total_mib: selected.total_mib,
        })
    }

    fn cluster_size_bytes(&self) -> usize {
        (self.sectors_per_cluster as usize).saturating_mul(SECTOR_SIZE)
    }

    fn read_cluster_chain(&mut self, start_cluster: u32, max_clusters: usize) -> Result<Vec<u32>, &'static str> {
        let mut chain = Vec::new();
        if start_cluster < 2 {
            return Ok(chain);
        }

        let mut current = start_cluster;
        let mut guard = 0usize;
        let mut cached_fat_lba: u64 = u64::MAX;
        let mut cached_fat_sector = [0u8; SECTOR_SIZE];

        while current >= 2 && current < 0x0FFF_FFF8 {
            chain.push(current);
            if chain.len() >= max_clusters {
                break;
            }

            let fat_offset = (current as u64).checked_mul(4).ok_or("FAT index overflow")?;
            let lba = self
                .fat_start
                .checked_add(fat_offset / SECTOR_SIZE as u64)
                .ok_or("FAT index overflow")?;
            let offset = (fat_offset % SECTOR_SIZE as u64) as usize;
            if offset + 4 > SECTOR_SIZE {
                return Err("FAT index overflow");
            }

            if lba != cached_fat_lba {
                if !self.read_sector(lba, &mut cached_fat_sector) {
                    return Err("FAT read error");
                }
                cached_fat_lba = lba;
            }

            let mut bytes = [0u8; 4];
            bytes.copy_from_slice(&cached_fat_sector[offset..offset + 4]);
            let next = u32::from_le_bytes(bytes) & 0x0FFF_FFFF;
            if next == current || next < 2 || next >= 0x0FFF_FFF8 {
                break;
            }

            current = next;
            guard += 1;
            if guard > 0x100000 {
                return Err("FAT chain loop");
            }
        }

        Ok(chain)
    }

    fn decode_lfn_part(raw: &[u8; 32]) -> String {
        let mut out = String::new();

        let mut push_u16 = |lo: u8, hi: u8| -> bool {
            let c = u16::from_le_bytes([lo, hi]);
            if c == 0x0000 {
                return false;
            }
            if c == 0xFFFF {
                return true;
            }
            if c < 0x80 {
                out.push(c as u8 as char);
            } else {
                // Keep ASCII-safe fallback in this stage.
                out.push('?');
            }
            true
        };

        // name1: 1..10
        for i in 0..5usize {
            let lo = raw[1 + i * 2];
            let hi = raw[2 + i * 2];
            if !push_u16(lo, hi) {
                return out;
            }
        }
        // name2: 14..25
        for i in 0..6usize {
            let lo = raw[14 + i * 2];
            let hi = raw[15 + i * 2];
            if !push_u16(lo, hi) {
                return out;
            }
        }
        // name3: 28..31
        for i in 0..2usize {
            let lo = raw[28 + i * 2];
            let hi = raw[29 + i * 2];
            if !push_u16(lo, hi) {
                return out;
            }
        }

        out
    }

    fn short_name_to_string(name: &[u8; 11], file_type: FileType) -> String {
        let mut out = String::new();

        let name_len = name[0..8]
            .iter()
            .position(|&c| c == b' ' || c == 0)
            .unwrap_or(8);
        for b in &name[0..name_len] {
            out.push(*b as char);
        }

        if file_type == FileType::File {
            let ext_len = name[8..11]
                .iter()
                .position(|&c| c == b' ' || c == 0)
                .unwrap_or(3);
            if ext_len > 0 {
                out.push('.');
                for b in &name[8..8 + ext_len] {
                    out.push(*b as char);
                }
            }
        }

        out
    }

    fn read_dir_entries_with_limit(
        &mut self,
        cluster: u32,
        max_clusters: usize,
    ) -> Result<Vec<DirEntry>, &'static str> {
        let start = self.normalized_dir_cluster(cluster);
        let cluster_chain = self.read_cluster_chain(start, max_clusters)?;
        if cluster_chain.is_empty() {
            return Ok(Vec::new());
        }

        let cluster_size = self.cluster_size_bytes();
        let mut entries = Vec::new();
        let mut lfn_parts: Vec<String> = Vec::new();
        let mut end_found = false;

        for dir_cluster in cluster_chain {
            let mut cluster_buf = alloc::vec![0u8; cluster_size];
            for sec in 0..self.sectors_per_cluster as usize {
                let lba = self.cluster_to_lba(dir_cluster) + sec as u64;
                let start_off = sec * SECTOR_SIZE;
                let end_off = start_off + SECTOR_SIZE;
                if !self.read_sector(lba, &mut cluster_buf[start_off..end_off]) {
                    return Err("Directory read failed");
                }
            }

            let entry_count = cluster_size / 32;
            for i in 0..entry_count {
                let off = i * 32;
                let mut raw = [0u8; 32];
                raw.copy_from_slice(&cluster_buf[off..off + 32]);

                let first = raw[0];
                if first == 0x00 {
                    end_found = true;
                    break;
                }
                if first == 0xE5 {
                    lfn_parts.clear();
                    continue;
                }

                let attr = raw[11];
                if (attr & FAT32_DIR_ATTR_LFN) == FAT32_DIR_ATTR_LFN {
                    let part = Self::decode_lfn_part(&raw);
                    lfn_parts.push(part);
                    continue;
                }

                // Skip volume labels and unsupported entries.
                if (attr & 0x08) != 0 {
                    lfn_parts.clear();
                    continue;
                }

                let mut short = [0u8; 11];
                short.copy_from_slice(&raw[0..11]);

                let file_type = if (attr & 0x10) != 0 {
                    FileType::Directory
                } else {
                    FileType::File
                };

                let cluster_low = u16::from_le_bytes([raw[26], raw[27]]) as u32;
                let cluster_high = u16::from_le_bytes([raw[20], raw[21]]) as u32;
                let data_cluster = (cluster_high << 16) | cluster_low;
                let size = u32::from_le_bytes([raw[28], raw[29], raw[30], raw[31]]);

                let mut entry = DirEntry::empty();
                entry.valid = true;
                entry.name = short;
                entry.size = size;
                entry.cluster = if data_cluster == 0 {
                    self.root_cluster
                } else {
                    data_cluster
                };
                entry.file_type = file_type;

                if !lfn_parts.is_empty() {
                    let mut full = String::new();
                    for part in lfn_parts.iter().rev() {
                        full.push_str(part.as_str());
                    }
                    if !full.is_empty() {
                        entry.set_display_name(full.as_str());
                    }
                }
                if entry.display_len == 0 {
                    let short_text = Self::short_name_to_string(&entry.name, entry.file_type);
                    entry.set_display_name(short_text.as_str());
                }

                entries.push(entry);
                lfn_parts.clear();
            }

            if end_found {
                break;
            }
        }

        Ok(entries)
    }

    pub fn read_dir_entries(&mut self, cluster: u32) -> Result<Vec<DirEntry>, &'static str> {
        self.read_dir_entries_with_limit(cluster, 1024)
    }

    pub fn read_dir_entries_limited(
        &mut self,
        cluster: u32,
        max_clusters: usize,
    ) -> Result<Vec<DirEntry>, &'static str> {
        let limit = max_clusters.max(1).min(1024);
        self.read_dir_entries_with_limit(cluster, limit)
    }

    pub fn read_file_sized(
        &mut self,
        start_cluster: u32,
        file_size: usize,
        buffer: &mut [u8],
    ) -> Result<usize, &'static str> {
        self.read_file_sized_with_progress(start_cluster, file_size, buffer, |_copied, _total| true)
    }

    pub fn read_file_sized_with_progress<F>(
        &mut self,
        start_cluster: u32,
        file_size: usize,
        buffer: &mut [u8],
        mut progress: F,
    ) -> Result<usize, &'static str>
    where
        F: FnMut(usize, usize) -> bool,
    {
        if start_cluster < 2 || file_size == 0 || buffer.is_empty() {
            let _ = progress(0, 0);
            return Ok(0);
        }

        let target = file_size.min(buffer.len());
        if target == 0 {
            let _ = progress(0, 0);
            return Ok(0);
        }
        if !progress(0, target) {
            return Err("Operation canceled");
        }

        let cluster_size = self.cluster_size_bytes();
        if cluster_size == 0 {
            return Err("Invalid cluster size");
        }
        let needed_clusters = (target + cluster_size - 1) / cluster_size;
        let max_clusters = core::cmp::max(needed_clusters.saturating_add(1), 8).min(262_144);
        let chain = self.read_cluster_chain(start_cluster, max_clusters)?;
        if chain.is_empty() {
            return Err("Invalid file cluster");
        }

        if let Some(handle) = self.uefi_block_handle {
            if let Ok(copied) =
                self.read_chain_sized_via_uefi(handle, chain.as_slice(), target, buffer, &mut progress)
            {
                return Ok(copied);
            }
        }

        let mut copied = 0usize;
        for cluster in chain {
            for sec in 0..self.sectors_per_cluster as usize {
                if copied >= target {
                    let _ = progress(copied, target);
                    return Ok(copied);
                }
                let lba = self.cluster_to_lba(cluster) + sec as u64;
                let mut sector = [0u8; SECTOR_SIZE];
                if !self.read_sector(lba, &mut sector) {
                    return Err("IO Error");
                }

                let take = (target - copied).min(SECTOR_SIZE);
                buffer[copied..copied + take].copy_from_slice(&sector[..take]);
                copied += take;
                if !progress(copied, target) {
                    return Err("Operation canceled");
                }
            }
        }

        Ok(copied)
    }

    fn read_chain_sized_via_uefi<F>(
        &self,
        handle: Handle,
        chain: &[u32],
        target: usize,
        buffer: &mut [u8],
        progress: &mut F,
    ) -> Result<usize, &'static str>
    where
        F: FnMut(usize, usize) -> bool,
    {
        if target == 0 {
            return Ok(0);
        }
        if !progress(0, target) {
            return Err("Operation canceled");
        }

        let params = OpenProtocolParams {
            handle,
            agent: boot::image_handle(),
            controller: None,
        };

        let blk = match unsafe {
            boot::open_protocol::<BlockIO>(params, OpenProtocolAttributes::GetProtocol)
        } {
            Ok(p) => p,
            Err(_) => return Err("BlockIO open failed"),
        };

        let (media_id, last_block, block_size) = {
            let media = blk.media();
            if !media.is_media_present() {
                return Err("Media not present");
            }
            (
                media.media_id(),
                media.last_block(),
                media.block_size() as usize,
            )
        };

        if block_size < SECTOR_SIZE
            || block_size > MAX_UEFI_BLOCK_SIZE
            || (block_size % SECTOR_SIZE) != 0
        {
            return Err("Unsupported block size");
        }

        let mut copied = 0usize;
        let mut cached_block_lba = u64::MAX;
        let mut scratch = AlignedBlock([0u8; MAX_UEFI_BLOCK_SIZE]);

        for cluster in chain {
            for sec in 0..self.sectors_per_cluster as usize {
                if copied >= target {
                    return Ok(copied);
                }

                let lba = self.cluster_to_lba(*cluster) + sec as u64;
                let byte_offset = lba
                    .checked_mul(SECTOR_SIZE as u64)
                    .ok_or("LBA overflow")?;
                let block_lba = byte_offset / block_size as u64;
                let offset = (byte_offset % block_size as u64) as usize;

                if block_lba > last_block {
                    return Err("IO Error");
                }

                if block_lba != cached_block_lba {
                    if blk
                        .read_blocks(media_id, block_lba, &mut scratch.0[0..block_size])
                        .is_err()
                    {
                        return Err("IO Error");
                    }
                    cached_block_lba = block_lba;
                }

                let take = (target - copied).min(SECTOR_SIZE);
                buffer[copied..copied + take]
                    .copy_from_slice(&scratch.0[offset..offset + take]);
                copied += take;
                if !progress(copied, target) {
                    return Err("Operation canceled");
                }
            }
        }

        Ok(copied)
    }

    fn write_chain_content_via_uefi<F>(
        &self,
        handle: Handle,
        chain: &[u32],
        content: &[u8],
        progress: &mut F,
    ) -> Result<Option<usize>, &'static str>
    where
        F: FnMut(usize, usize) -> bool,
    {
        if chain.is_empty() {
            return Ok(Some(0));
        }

        let params = OpenProtocolParams {
            handle,
            agent: boot::image_handle(),
            controller: None,
        };

        let mut blk = match unsafe {
            boot::open_protocol::<BlockIO>(params, OpenProtocolAttributes::GetProtocol)
        } {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

        let (media_id, last_block, block_size) = {
            let media = blk.media();
            if !media.is_media_present() {
                return Ok(None);
            }
            (
                media.media_id(),
                media.last_block(),
                media.block_size() as usize,
            )
        };

        if block_size < SECTOR_SIZE
            || block_size > MAX_UEFI_BLOCK_SIZE
            || (block_size % SECTOR_SIZE) != 0
        {
            return Ok(None);
        }

        let total_len = content.len();
        let mut written_data = 0usize;
        let mut content_offset = 0usize;
        let mut sector_tmp = [0u8; SECTOR_SIZE];

        let mut scratch = AlignedBlock([0u8; MAX_UEFI_BLOCK_SIZE]);
        let mut cached_block_lba = u64::MAX;
        let mut cached_block_valid = false;
        let mut cached_block_dirty = false;

        for cluster in chain.iter() {
            for sec in 0..self.sectors_per_cluster as usize {
                let lba = self.cluster_to_lba(*cluster) + sec as u64;
                let copy_len = total_len
                    .saturating_sub(content_offset)
                    .min(SECTOR_SIZE);

                if block_size == SECTOR_SIZE {
                    if copy_len == SECTOR_SIZE {
                        if blk
                            .write_blocks(
                                media_id,
                                lba,
                                &content[content_offset..content_offset + SECTOR_SIZE],
                            )
                            .is_err()
                        {
                            return Err("Data write failed");
                        }
                    } else {
                        sector_tmp.fill(0);
                        if copy_len > 0 {
                            sector_tmp[..copy_len]
                                .copy_from_slice(&content[content_offset..content_offset + copy_len]);
                        }
                        if blk.write_blocks(media_id, lba, &sector_tmp).is_err() {
                            return Err("Data write failed");
                        }
                    }
                } else {
                    let byte_offset = lba
                        .checked_mul(SECTOR_SIZE as u64)
                        .ok_or("LBA overflow")?;
                    let block_lba = byte_offset / block_size as u64;
                    let offset = (byte_offset % block_size as u64) as usize;

                    if block_lba > last_block {
                        return Err("Data write failed");
                    }

                    if !cached_block_valid || cached_block_lba != block_lba {
                        if cached_block_valid && cached_block_dirty {
                            if blk
                                .write_blocks(
                                    media_id,
                                    cached_block_lba,
                                    &scratch.0[0..block_size],
                                )
                                .is_err()
                            {
                                return Err("Data write failed");
                            }
                        }
                        if blk
                            .read_blocks(media_id, block_lba, &mut scratch.0[0..block_size])
                            .is_err()
                        {
                            return Err("Data write failed");
                        }
                        cached_block_lba = block_lba;
                        cached_block_valid = true;
                    }

                    if copy_len == SECTOR_SIZE {
                        scratch.0[offset..offset + SECTOR_SIZE]
                            .copy_from_slice(&content[content_offset..content_offset + SECTOR_SIZE]);
                    } else {
                        scratch.0[offset..offset + SECTOR_SIZE].fill(0);
                        if copy_len > 0 {
                            scratch.0[offset..offset + copy_len]
                                .copy_from_slice(&content[content_offset..content_offset + copy_len]);
                        }
                    }
                    cached_block_dirty = true;
                }

                written_data = written_data.saturating_add(copy_len);
                content_offset = content_offset.saturating_add(SECTOR_SIZE);
                if !progress(written_data.min(total_len), total_len) {
                    return Err("Operation canceled");
                }
            }
        }

        if cached_block_valid && cached_block_dirty {
            if blk
                .write_blocks(media_id, cached_block_lba, &scratch.0[0..block_size])
                .is_err()
            {
                return Err("Data write failed");
            }
        }

        Ok(Some(written_data))
    }
}

impl FileSystem for Fat32 {
    fn init(&mut self) -> bool {
        // Quick exit if already initialized
        if self.init_status == InitStatus::Success { return true; }

        self.init_status = InitStatus::InProgress;

        // Reset source selection before probing.
        self.uefi_block_handle = None;

        if self.try_init_from_boot_device() {
            self.init_status = InitStatus::Success;
            return true;
        }

        if self.try_init_via_uefi_blockio() {
            self.init_status = InitStatus::Success;
            return true;
        }

        if let Some(found) =
            Self::probe_with_reader(|lba, buf| self.read_sector_virtio_or_nvme(lba, buf))
        {
            self.apply_probe_result(found);
            self.init_status = InitStatus::Success;
            return true;
        }

        // No valid filesystem found
        self.init_status = InitStatus::Failed;
        false
    }

    fn root_dir(&mut self) -> Result<u32, &'static str> {
        Ok(self.root_cluster)
    }

    fn read_dir(&mut self, cluster: u32) -> Result<[DirEntry; 16], &'static str> {
        let dynamic = self.read_dir_entries(cluster)?;
        let mut out = [DirEntry::empty(); 16];
        let mut idx = 0usize;

        for entry in dynamic.into_iter() {
            if !entry.valid {
                continue;
            }
            if idx >= out.len() {
                break;
            }
            out[idx] = entry;
            idx += 1;
        }

        Ok(out)
    }

    fn read_file(&mut self, cluster: u32, buffer: &mut [u8]) -> Result<usize, &'static str> {
        self.read_file_sized(cluster, buffer.len(), buffer)
    }
}

impl Fat32 {
    fn normalized_dir_cluster(&self, cluster: u32) -> u32 {
        if cluster >= 2 {
            cluster
        } else {
            self.root_cluster
        }
    }

    fn fat_entry_lba_offset(&self, cluster: u32, fat_index: u32) -> Option<(u64, usize)> {
        let fat_offset = (cluster as u64).checked_mul(4)?;
        let fat_base = self
            .fat_start
            .checked_add((fat_index as u64).checked_mul(self.sectors_per_fat as u64)?)?;
        let lba = fat_base.checked_add(fat_offset / SECTOR_SIZE as u64)?;
        let offset = (fat_offset % SECTOR_SIZE as u64) as usize;
        if offset + 4 > SECTOR_SIZE {
            return None;
        }
        Some((lba, offset))
    }

    fn read_fat_entry(&mut self, cluster: u32) -> Result<u32, &'static str> {
        let (lba, offset) = self
            .fat_entry_lba_offset(cluster, 0)
            .ok_or("FAT index overflow")?;

        let mut sector = [0u8; SECTOR_SIZE];
        if !self.read_sector(lba, &mut sector) {
            return Err("FAT read error");
        }

        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&sector[offset..offset + 4]);
        Ok(u32::from_le_bytes(bytes) & 0x0FFF_FFFF)
    }

    fn write_fat_entry(&mut self, cluster: u32, value: u32) -> Result<(), &'static str> {
        let copies = (self.fats as usize).max(1);
        for fat_idx in 0..copies {
            let (lba, offset) = self
                .fat_entry_lba_offset(cluster, fat_idx as u32)
                .ok_or("FAT index overflow")?;

            let mut sector = [0u8; SECTOR_SIZE];
            if !self.read_sector(lba, &mut sector) {
                return Err("FAT read error");
            }

            let mut old = [0u8; 4];
            old.copy_from_slice(&sector[offset..offset + 4]);
            let old_raw = u32::from_le_bytes(old);
            let new_raw = (old_raw & 0xF000_0000) | (value & 0x0FFF_FFFF);
            sector[offset..offset + 4].copy_from_slice(&new_raw.to_le_bytes());

            if !self.write_sector(lba, &sector) {
                return Err("FAT write error");
            }
        }
        Ok(())
    }

    fn free_cluster_chain(&mut self, start_cluster: u32) -> Result<(), &'static str> {
        let mut cluster = start_cluster;
        let mut guard = 0usize;

        while cluster >= 2 && cluster < 0x0FFF_FFF8 {
            let next = self.read_fat_entry(cluster)?;
            self.write_fat_entry(cluster, 0)?;

            if next == cluster || next < 2 || next >= 0x0FFF_FFF8 {
                break;
            }

            cluster = next;
            guard += 1;
            if guard > 0x10000 {
                return Err("FAT chain loop");
            }
        }

        Ok(())
    }

    fn find_free_cluster(&mut self) -> Result<u32, &'static str> {
        let total_entries = ((self.sectors_per_fat as u64 * SECTOR_SIZE as u64) / 4) as u32;
        if total_entries <= 2 {
            return Err("Invalid FAT size");
        }

        let mut start = self.next_free_cluster_hint;
        if start < 2 || start >= total_entries {
            start = 2;
        }

        let mut cluster = start;
        loop {
            if self.read_fat_entry(cluster)? == 0 {
                let mut next_hint = cluster.saturating_add(1);
                if next_hint >= total_entries {
                    next_hint = 2;
                }
                self.next_free_cluster_hint = next_hint;
                return Ok(cluster);
            }

            cluster = cluster.saturating_add(1);
            if cluster >= total_entries {
                cluster = 2;
            }
            if cluster == start {
                break;
            }
        }

        Err("No free clusters")
    }

    fn valid_short_char(b: u8) -> bool {
        b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'(' | b')' | b'-' | b'@'
                    | b'^' | b'_' | b'`' | b'{' | b'}' | b'~'
            )
    }

    fn to_short_name(filename: &str) -> Option<[u8; 11]> {
        let trimmed = filename.trim();
        if trimmed.is_empty() {
            return None;
        }

        let mut parts = trimmed.splitn(2, '.');
        let name = parts.next().unwrap_or("");
        let ext = parts.next().unwrap_or("");

        if name.is_empty() || name.len() > 8 || ext.len() > 3 {
            return None;
        }

        let mut out = [b' '; 11];
        for (i, b) in name.bytes().enumerate() {
            if !Self::valid_short_char(b) {
                return None;
            }
            out[i] = b.to_ascii_uppercase();
        }

        for (i, b) in ext.bytes().enumerate() {
            if !Self::valid_short_char(b) {
                return None;
            }
            out[8 + i] = b.to_ascii_uppercase();
        }

        Some(out)
    }

    fn split_name_and_ext(filename: &str) -> (&str, &str) {
        let trimmed = filename.trim();
        if trimmed.is_empty() {
            return ("", "");
        }

        if trimmed == "." || trimmed == ".." {
            return ("", "");
        }

        if let Some(dot) = trimmed.rfind('.') {
            if dot > 0 && dot + 1 < trimmed.len() {
                return (&trimmed[..dot], &trimmed[dot + 1..]);
            }
            if dot > 0 && dot + 1 == trimmed.len() {
                return (&trimmed[..dot], "");
            }
            if dot == 0 && trimmed.len() > 1 {
                return (&trimmed[1..], "");
            }
        }

        (trimmed, "")
    }

    fn normalize_short_component(input: &str, max_len: usize) -> Vec<u8> {
        let mut out = Vec::new();
        if max_len == 0 {
            return out;
        }

        for mut b in input.bytes() {
            if b == b'.' || b == b' ' || b == b'\t' {
                continue;
            }
            b = b.to_ascii_uppercase();
            let mapped = if Self::valid_short_char(b) { b } else { b'_' };
            out.push(mapped);
            if out.len() >= max_len {
                break;
            }
        }
        out
    }

    fn short_name_hash(name: &str) -> u16 {
        let mut h: u32 = 2166136261;
        for b in name.bytes() {
            let folded = if b.is_ascii_uppercase() {
                b.to_ascii_lowercase()
            } else {
                b
            };
            h ^= folded as u32;
            h = h.wrapping_mul(16777619);
        }
        (h & 0xFFFF) as u16
    }

    fn hex_nibble_upper(n: u8) -> u8 {
        if n < 10 {
            b'0' + n
        } else {
            b'A' + (n - 10)
        }
    }

    fn to_short_name_relaxed(filename: &str) -> Option<[u8; 11]> {
        if let Some(strict) = Self::to_short_name(filename) {
            return Some(strict);
        }

        let trimmed = filename.trim();
        if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
            return None;
        }

        let (name_part, ext_part) = Self::split_name_and_ext(trimmed);
        let mut name_norm = Self::normalize_short_component(name_part, 64);
        let ext_norm = Self::normalize_short_component(ext_part, 3);

        if name_norm.is_empty() {
            name_norm.push(b'F');
            name_norm.push(b'I');
            name_norm.push(b'L');
            name_norm.push(b'E');
        }

        let hash = Self::short_name_hash(trimmed);
        let hash_bytes = [
            Self::hex_nibble_upper(((hash >> 12) & 0x0F) as u8),
            Self::hex_nibble_upper(((hash >> 8) & 0x0F) as u8),
            Self::hex_nibble_upper(((hash >> 4) & 0x0F) as u8),
            Self::hex_nibble_upper((hash & 0x0F) as u8),
        ];

        let mut out = [b' '; 11];
        if name_norm.len() >= 4 {
            out[0..4].copy_from_slice(&name_norm[0..4]);
            out[4..8].copy_from_slice(&hash_bytes);
        } else {
            let mut pos = 0usize;
            for b in name_norm.iter() {
                if pos >= 8 {
                    break;
                }
                out[pos] = *b;
                pos += 1;
            }
            for hb in hash_bytes.iter() {
                if pos >= 8 {
                    break;
                }
                out[pos] = *hb;
                pos += 1;
            }
            while pos < 8 {
                out[pos] = b'_';
                pos += 1;
            }
        }

        for (i, b) in ext_norm.iter().enumerate() {
            if i >= 3 {
                break;
            }
            out[8 + i] = *b;
        }

        Some(out)
    }

    fn entry_cluster(entry: &FatDirEntry) -> u32 {
        ((entry.cluster_high as u32) << 16) | (entry.cluster_low as u32)
    }

    fn set_entry_cluster(entry: &mut FatDirEntry, cluster: u32) {
        entry.cluster_high = ((cluster >> 16) & 0xFFFF) as u16;
        entry.cluster_low = (cluster & 0xFFFF) as u16;
    }

    fn next_contiguous_lba_run(
        &self,
        chain: &[u32],
        sectors_per_cluster: usize,
        cluster_idx: &mut usize,
        sector_idx: &mut usize,
        max_sectors: usize,
    ) -> Option<(u64, usize)> {
        if max_sectors == 0 || sectors_per_cluster == 0 || *cluster_idx >= chain.len() {
            return None;
        }
        if *sector_idx >= sectors_per_cluster {
            return None;
        }

        let start_lba = self
            .cluster_to_lba(chain[*cluster_idx])
            .checked_add(*sector_idx as u64)?;

        let mut run_len = 0usize;
        let mut ci = *cluster_idx;
        let mut si = *sector_idx;
        let mut expected_next = start_lba;

        while run_len < max_sectors && ci < chain.len() {
            let this_lba = self.cluster_to_lba(chain[ci]).checked_add(si as u64)?;
            if run_len > 0 && this_lba != expected_next {
                break;
            }

            run_len += 1;
            expected_next = this_lba.saturating_add(1);

            si += 1;
            if si >= sectors_per_cluster {
                si = 0;
                ci += 1;
            }
        }

        if run_len == 0 {
            return None;
        }

        *cluster_idx = ci;
        *sector_idx = si;
        Some((start_lba, run_len))
    }

    pub fn ensure_subdirectory(
        &mut self,
        parent_cluster: u32,
        name: &str,
    ) -> Result<u32, &'static str> {
        if self.bytes_per_sector == 0 {
            return Err("Filesystem not initialized");
        }
        let strict_short = Self::to_short_name(name);
        let short_name = strict_short
            .or_else(|| Self::to_short_name_relaxed(name))
            .ok_or("Invalid filename")?;
        let parent_cluster = self.normalized_dir_cluster(parent_cluster);

        let dir_chain = self.read_cluster_chain(parent_cluster, 1024)?;
        if dir_chain.is_empty() {
            return Err("Directory read failed");
        }

        let mut free_slot: Option<(usize, usize, usize)> = None;
        let mut scan_done = false;

        'outer: for (ci, &cluster) in dir_chain.iter().enumerate() {
            for sec in 0..self.sectors_per_cluster as usize {
                let lba = self.cluster_to_lba(cluster) + sec as u64;
                let mut sector = [0u8; SECTOR_SIZE];
                if !self.read_sector(lba, &mut sector) {
                    return Err("Directory read failed");
                }

                let entries_per_sector = SECTOR_SIZE / 32;
                for i in 0..entries_per_sector {
                    let off = i * 32;
                    let first_byte = sector[off];

                    if first_byte == 0 {
                        if free_slot.is_none() {
                            free_slot = Some((ci, sec, i));
                        }
                        scan_done = true;
                        break 'outer;
                    }
                    if first_byte == 0xE5 {
                        if free_slot.is_none() {
                            free_slot = Some((ci, sec, i));
                        }
                        continue;
                    }

                    let attr = sector[off + 11];
                    if (attr & 0x0F) == 0x0F || (attr & 0x08) != 0 {
                        continue;
                    }

                    let mut entry_name = [0u8; 11];
                    entry_name.copy_from_slice(&sector[off..off + 11]);
                    if entry_name == short_name {
                        if strict_short.is_none() {
                            return Err("Target already exists");
                        }
                        if (attr & 0x10) != 0 {
                            let high = u16::from_le_bytes([sector[off + 20], sector[off + 21]]);
                            let low = u16::from_le_bytes([sector[off + 26], sector[off + 27]]);
                            return Ok(((high as u32) << 16) | (low as u32));
                        } else {
                            return Err("Target exists but is a file");
                        }
                    }
                }
            }
        }

        // Not found, create new
        let (slot_ci, slot_sec, slot_idx) = if let Some(free) = free_slot {
            free
        } else {
             // Extend parent directory
             let new_cluster = self.find_free_cluster()?;
             self.write_fat_entry(new_cluster, FAT32_EOC)?;
             let last_cluster = *dir_chain.last().ok_or("Empty directory chain")?;
             self.write_fat_entry(last_cluster, new_cluster)?;

             let zero_sector = [0u8; SECTOR_SIZE];
             for sec in 0..self.sectors_per_cluster as usize {
                 let lba = self.cluster_to_lba(new_cluster) + sec as u64;
                 self.write_sector(lba, &zero_sector);
             }
             (dir_chain.len(), 0, 0)
        };

        // Allocate cluster for the new subdirectory
        let subdir_cluster = self.find_free_cluster()?;
        self.write_fat_entry(subdir_cluster, FAT32_EOC)?;

        // Initialize . and .. entries
        let mut subdir_sector = [0u8; SECTOR_SIZE];
        
        // Dot entry
        subdir_sector[0..11].copy_from_slice(b".          ");
        subdir_sector[11] = 0x10; // Directory
        let high = (subdir_cluster >> 16) as u16;
        let low = (subdir_cluster & 0xFFFF) as u16;
        subdir_sector[20..22].copy_from_slice(&high.to_le_bytes());
        subdir_sector[26..28].copy_from_slice(&low.to_le_bytes());

        // DotDot entry
        subdir_sector[32..43].copy_from_slice(b"..         ");
        subdir_sector[43] = 0x10;
        let parent_target = if parent_cluster == self.root_cluster { 0 } else { parent_cluster };
        let high_p = (parent_target >> 16) as u16;
        let low_p = (parent_target & 0xFFFF) as u16;
        subdir_sector[52..54].copy_from_slice(&high_p.to_le_bytes());
        subdir_sector[58..60].copy_from_slice(&low_p.to_le_bytes());

        // Write first sector and zero others
        self.write_sector(self.cluster_to_lba(subdir_cluster), &subdir_sector);
        let zero_sector = [0u8; SECTOR_SIZE];
        for sec in 1..self.sectors_per_cluster as usize {
            self.write_sector(self.cluster_to_lba(subdir_cluster) + sec as u64, &zero_sector);
        }

        // Write entry in parent directory
        let slot_cluster = if slot_ci < dir_chain.len() {
            dir_chain[slot_ci]
        } else {
            // We extended the chain
            let updated_chain = self.read_cluster_chain(parent_cluster, 1024)?;
            *updated_chain.last().ok_or("Directory chain error")?
        };

        let mut parent_sector = [0u8; SECTOR_SIZE];
        let lba = self.cluster_to_lba(slot_cluster) + slot_sec as u64;
        self.read_sector(lba, &mut parent_sector);
        
        let off = slot_idx * 32;
        parent_sector[off..off+11].copy_from_slice(&short_name);
        parent_sector[off+11] = 0x10; // Directory
        let high_s = (subdir_cluster >> 16) as u16;
        let low_s = (subdir_cluster & 0xFFFF) as u16;
        parent_sector[off+20..off+22].copy_from_slice(&high_s.to_le_bytes());
        parent_sector[off+26..off+28].copy_from_slice(&low_s.to_le_bytes());
        
        // Zero other fields
        parent_sector[off+12..off+20].fill(0);
        parent_sector[off+22..off+26].fill(0);
        parent_sector[off+28..off+32].fill(0);

        self.write_sector(lba, &parent_sector);

        Ok(subdir_cluster)
    }

    pub fn write_text_file_in_dir(
        &mut self,
        dir_cluster: u32,
        filename: &str,
        content: &[u8],
    ) -> Result<(), &'static str> {
        self.write_text_file_in_dir_with_progress(
            dir_cluster,
            filename,
            content,
            |_written, _total| true,
        )
    }

    pub fn write_text_file_in_dir_with_progress<F>(
        &mut self,
        dir_cluster: u32,
        filename: &str,
        content: &[u8],
        mut progress: F,
    ) -> Result<(), &'static str>
    where
        F: FnMut(usize, usize) -> bool,
    {
        if self.bytes_per_sector == 0 {
            return Err("Filesystem not initialized");
        }
        let strict_short = Self::to_short_name(filename);
        let short_name = strict_short
            .or_else(|| Self::to_short_name_relaxed(filename))
            .ok_or("Invalid filename")?;
        let dir_cluster = self.normalized_dir_cluster(dir_cluster);
        let total_len = content.len();
        if !progress(0, total_len) {
            return Err("Operation canceled");
        }
        let mut metadata_steps = 0usize;

        // Walk all directory clusters to find an existing entry or a free slot.
        let dir_chain = self.read_cluster_chain(dir_cluster, 1024)?;
        if dir_chain.is_empty() {
            return Err("Directory read failed");
        }

        // (cluster_index_in_chain, sector_within_cluster, entry_index_within_sector)
        let mut existing_slot: Option<(usize, usize, usize)> = None;
        let mut free_slot: Option<(usize, usize, usize)> = None;
        let mut scan_done = false;

        'outer: for (ci, &cluster) in dir_chain.iter().enumerate() {
            for sec in 0..self.sectors_per_cluster as usize {
                let lba = self.cluster_to_lba(cluster) + sec as u64;
                let mut sector = [0u8; SECTOR_SIZE];
                if !self.read_sector(lba, &mut sector) {
                    return Err("Directory read failed");
                }
                metadata_steps = metadata_steps.saturating_add(1);
                if (metadata_steps & 0x1F) == 0 && !progress(0, total_len) {
                    return Err("Operation canceled");
                }

                let entries_per_sector = SECTOR_SIZE / 32;
                for i in 0..entries_per_sector {
                    let off = i * 32;
                    let first_byte = sector[off];

                    if first_byte == 0 {
                        // End-of-directory marker: this slot is free
                        if free_slot.is_none() {
                            free_slot = Some((ci, sec, i));
                        }
                        scan_done = true;
                        break 'outer;
                    }
                    if first_byte == 0xE5 {
                        // Deleted entry: usable as free slot
                        if free_slot.is_none() {
                            free_slot = Some((ci, sec, i));
                        }
                        continue;
                    }

                    let attr = sector[off + 11];
                    // Skip LFN entries and volume labels
                    if (attr & 0x0F) == 0x0F || (attr & 0x08) != 0 {
                        continue;
                    }

                    // Check if this entry matches our target filename
                    let mut entry_name = [0u8; 11];
                    entry_name.copy_from_slice(&sector[off..off + 11]);
                    if entry_name == short_name {
                        if (attr & 0x10) != 0 {
                            return Err("Target is a directory");
                        }
                        if strict_short.is_none() {
                            return Err("Target already exists");
                        }
                        existing_slot = Some((ci, sec, i));
                        break 'outer;
                    }
                }
            }
        }

        // Determine which slot to use
        let (slot_ci, slot_sec, slot_idx) = if let Some(existing) = existing_slot {
            existing
        } else if let Some(free) = free_slot {
            free
        } else {
            // Directory is full across all clusters — allocate a new one
            let new_cluster = self.find_free_cluster()?;
            self.write_fat_entry(new_cluster, FAT32_EOC)?;

            // Link the new cluster to the end of the directory chain
            let last_cluster = *dir_chain.last().ok_or("Empty directory chain")?;
            self.write_fat_entry(last_cluster, new_cluster)?;

            // Zero out the new cluster so all entries start as 0x00
            let zero_sector = [0u8; SECTOR_SIZE];
            for sec in 0..self.sectors_per_cluster as usize {
                let lba = self.cluster_to_lba(new_cluster) + sec as u64;
                if !self.write_sector(lba, &zero_sector) {
                    return Err("Directory write failed");
                }
                metadata_steps = metadata_steps.saturating_add(1);
                if (metadata_steps & 0x1F) == 0 && !progress(0, total_len) {
                    return Err("Operation canceled");
                }
            }

            // Use the first entry in the new cluster
            (dir_chain.len(), 0usize, 0usize)
        };

        // Read the sector containing our target slot
        let slot_cluster = if slot_ci < dir_chain.len() {
            dir_chain[slot_ci]
        } else {
            // We just allocated a new cluster; re-read chain to get it
            let updated_chain = self.read_cluster_chain(dir_cluster, 1024)?;
            *updated_chain.get(slot_ci).ok_or("Directory chain error")?
        };
        let slot_lba = self.cluster_to_lba(slot_cluster) + slot_sec as u64;
        let mut dir_sector = [0u8; SECTOR_SIZE];
        if !self.read_sector(slot_lba, &mut dir_sector) {
            return Err("Directory read failed");
        }

        let entries =
            unsafe { core::slice::from_raw_parts_mut(dir_sector.as_mut_ptr() as *mut FatDirEntry, 16) };

        let idx = slot_idx;

        if existing_slot.is_none() {
            // Initialize a new entry
            entries[idx] = FatDirEntry {
                name: [0; 11],
                attr: 0x20,
                nt_res: 0,
                create_time_tenth: 0,
                create_time: 0,
                create_date: 0,
                last_access_date: 0,
                cluster_high: 0,
                write_time: 0,
                write_date: 0,
                cluster_low: 0,
                size: 0,
            };
        }

        let old_cluster = Self::entry_cluster(&entries[idx]);
        entries[idx].name = short_name;
        entries[idx].attr = 0x20;

        if content.is_empty() {
            if old_cluster >= 2 {
                self.free_cluster_chain(old_cluster)?;
                self.next_free_cluster_hint = old_cluster;
            }
            Self::set_entry_cluster(&mut entries[idx], 0);
            entries[idx].size = 0;
            if !progress(0, 0) {
                return Err("Operation canceled");
            }
        } else {
            let cluster_size = self.cluster_size_bytes();
            let required_clusters = (content.len() + cluster_size - 1) / cluster_size;

            let mut chain = if old_cluster >= 2 {
                self.read_cluster_chain(old_cluster, 1024)?
            } else {
                Vec::new()
            };

            if chain.is_empty() {
                let first = self.find_free_cluster()?;
                self.write_fat_entry(first, FAT32_EOC)?;
                chain.push(first);
            }
            if !progress(0, total_len) {
                return Err("Operation canceled");
            }

            if chain.len() < required_clusters {
                let mut prev = *chain.last().ok_or("Invalid FAT chain")?;
                for _ in chain.len()..required_clusters {
                    let next = self.find_free_cluster()?;
                    self.write_fat_entry(prev, next)?;
                    self.write_fat_entry(next, FAT32_EOC)?;
                    chain.push(next);
                    prev = next;
                    metadata_steps = metadata_steps.saturating_add(1);
                    if (metadata_steps & 0x0F) == 0 && !progress(0, total_len) {
                        return Err("Operation canceled");
                    }
                }
            }
            if !progress(0, total_len) {
                return Err("Operation canceled");
            }

            if chain.len() > required_clusters {
                let keep_last = chain[required_clusters - 1];
                let tail_start = chain[required_clusters];
                self.write_fat_entry(keep_last, FAT32_EOC)?;
                self.free_cluster_chain(tail_start)?;
                self.next_free_cluster_hint = tail_start;
                chain.truncate(required_clusters);
            } else {
                let last = *chain.last().ok_or("Invalid FAT chain")?;
                self.write_fat_entry(last, FAT32_EOC)?;
            }
            if !progress(0, total_len) {
                return Err("Operation canceled");
            }

            let mut written = 0usize;
            let mut wrote_via_uefi = false;
            if let Some(handle) = self.uefi_block_handle {
                match self.write_chain_content_via_uefi(handle, chain.as_slice(), content, &mut progress)? {
                    Some(done) => {
                        written = done;
                        wrote_via_uefi = true;
                    }
                    None => {}
                }
            }
            if !wrote_via_uefi {
                for (i, cluster) in chain.iter().enumerate() {
                    let cluster_start = i * cluster_size;
                    let cluster_end = core::cmp::min(cluster_start + cluster_size, content.len());

                    for sec in 0..self.sectors_per_cluster as usize {
                        let mut sector = [0u8; SECTOR_SIZE];
                        let sec_start = cluster_start + (sec * SECTOR_SIZE);

                        if sec_start < cluster_end {
                            let sec_end = core::cmp::min(sec_start + SECTOR_SIZE, cluster_end);
                            let copy_len = sec_end - sec_start;
                            sector[..copy_len].copy_from_slice(&content[sec_start..sec_end]);
                            written = written.saturating_add(copy_len);
                        }

                        let lba = self.cluster_to_lba(*cluster) + sec as u64;
                        if !self.write_sector(lba, &sector) {
                            return Err("Data write failed");
                        }
                        let visible_written = written.min(total_len);
                        if !progress(visible_written, total_len) {
                            return Err("Operation canceled");
                        }
                    }
                }
            }

            Self::set_entry_cluster(&mut entries[idx], chain[0]);
            entries[idx].size = content.len() as u32;
        }

        if !self.write_sector(slot_lba, &dir_sector) {
            return Err("Directory write failed");
        }
        if !progress(total_len, total_len) {
            return Err("Operation canceled");
        }

        Ok(())
    }

    pub fn copy_file_from_fat_in_dir_with_progress<F>(
        &mut self,
        src_fat: &mut Fat32,
        src_cluster: u32,
        src_size: usize,
        dir_cluster: u32,
        filename: &str,
        mut progress: F,
    ) -> Result<usize, &'static str>
    where
        F: FnMut(usize, usize) -> bool,
    {
        if self.bytes_per_sector == 0 || src_fat.bytes_per_sector == 0 {
            return Err("Filesystem not initialized");
        }
        if src_cluster < 2 || src_size == 0 {
            self.write_text_file_in_dir_with_progress(
                dir_cluster,
                filename,
                &[],
                |written, total| progress(written, total),
            )?;
            return Ok(0);
        }

        let strict_short = Self::to_short_name(filename);
        let short_name = strict_short
            .or_else(|| Self::to_short_name_relaxed(filename))
            .ok_or("Invalid filename")?;
        let dir_cluster = self.normalized_dir_cluster(dir_cluster);
        let total_len = src_size;
        if !progress(0, total_len) {
            return Err("Operation canceled");
        }

        let src_cluster_size = src_fat.cluster_size_bytes();
        if src_cluster_size == 0 {
            return Err("Invalid source cluster size");
        }
        let src_needed_clusters = (total_len + src_cluster_size - 1) / src_cluster_size;
        let src_max_clusters = core::cmp::max(src_needed_clusters.saturating_add(1), 8).min(262_144);
        let src_chain = src_fat.read_cluster_chain(src_cluster, src_max_clusters)?;
        if src_chain.is_empty() {
            return Err("Invalid source file cluster");
        }

        let mut metadata_steps = 0usize;

        let dir_chain = self.read_cluster_chain(dir_cluster, 1024)?;
        if dir_chain.is_empty() {
            return Err("Directory read failed");
        }

        let mut existing_slot: Option<(usize, usize, usize)> = None;
        let mut free_slot: Option<(usize, usize, usize)> = None;

        'outer: for (ci, &cluster) in dir_chain.iter().enumerate() {
            for sec in 0..self.sectors_per_cluster as usize {
                let lba = self.cluster_to_lba(cluster) + sec as u64;
                let mut sector = [0u8; SECTOR_SIZE];
                if !self.read_sector(lba, &mut sector) {
                    return Err("Directory read failed");
                }
                metadata_steps = metadata_steps.saturating_add(1);
                if (metadata_steps & 0x1F) == 0 && !progress(0, total_len) {
                    return Err("Operation canceled");
                }

                let entries_per_sector = SECTOR_SIZE / 32;
                for i in 0..entries_per_sector {
                    let off = i * 32;
                    let first_byte = sector[off];

                    if first_byte == 0 {
                        if free_slot.is_none() {
                            free_slot = Some((ci, sec, i));
                        }
                        break 'outer;
                    }
                    if first_byte == 0xE5 {
                        if free_slot.is_none() {
                            free_slot = Some((ci, sec, i));
                        }
                        continue;
                    }

                    let attr = sector[off + 11];
                    if (attr & 0x0F) == 0x0F || (attr & 0x08) != 0 {
                        continue;
                    }

                    let mut entry_name = [0u8; 11];
                    entry_name.copy_from_slice(&sector[off..off + 11]);
                    if entry_name == short_name {
                        if (attr & 0x10) != 0 {
                            return Err("Target is a directory");
                        }
                        if strict_short.is_none() {
                            return Err("Target already exists");
                        }
                        existing_slot = Some((ci, sec, i));
                        break 'outer;
                    }
                }
            }
        }

        let (slot_ci, slot_sec, slot_idx) = if let Some(existing) = existing_slot {
            existing
        } else if let Some(free) = free_slot {
            free
        } else {
            let new_cluster = self.find_free_cluster()?;
            self.write_fat_entry(new_cluster, FAT32_EOC)?;

            let last_cluster = *dir_chain.last().ok_or("Empty directory chain")?;
            self.write_fat_entry(last_cluster, new_cluster)?;

            let zero_sector = [0u8; SECTOR_SIZE];
            for sec in 0..self.sectors_per_cluster as usize {
                let lba = self.cluster_to_lba(new_cluster) + sec as u64;
                if !self.write_sector(lba, &zero_sector) {
                    return Err("Directory write failed");
                }
                metadata_steps = metadata_steps.saturating_add(1);
                if (metadata_steps & 0x1F) == 0 && !progress(0, total_len) {
                    return Err("Operation canceled");
                }
            }

            (dir_chain.len(), 0usize, 0usize)
        };

        let slot_cluster = if slot_ci < dir_chain.len() {
            dir_chain[slot_ci]
        } else {
            let updated_chain = self.read_cluster_chain(dir_cluster, 1024)?;
            *updated_chain.get(slot_ci).ok_or("Directory chain error")?
        };
        let slot_lba = self.cluster_to_lba(slot_cluster) + slot_sec as u64;
        let mut dir_sector = [0u8; SECTOR_SIZE];
        if !self.read_sector(slot_lba, &mut dir_sector) {
            return Err("Directory read failed");
        }

        let entries =
            unsafe { core::slice::from_raw_parts_mut(dir_sector.as_mut_ptr() as *mut FatDirEntry, 16) };
        let idx = slot_idx;

        if existing_slot.is_none() {
            entries[idx] = FatDirEntry {
                name: [0; 11],
                attr: 0x20,
                nt_res: 0,
                create_time_tenth: 0,
                create_time: 0,
                create_date: 0,
                last_access_date: 0,
                cluster_high: 0,
                write_time: 0,
                write_date: 0,
                cluster_low: 0,
                size: 0,
            };
        }

        let old_cluster = Self::entry_cluster(&entries[idx]);
        entries[idx].name = short_name;
        entries[idx].attr = 0x20;

        let cluster_size = self.cluster_size_bytes();
        let required_clusters = (total_len + cluster_size - 1) / cluster_size;

        let mut chain = if old_cluster >= 2 {
            self.read_cluster_chain(old_cluster, 1024)?
        } else {
            Vec::new()
        };

        if chain.is_empty() {
            let first = self.find_free_cluster()?;
            self.write_fat_entry(first, FAT32_EOC)?;
            chain.push(first);
        }
        if !progress(0, total_len) {
            return Err("Operation canceled");
        }

        if chain.len() < required_clusters {
            let mut prev = *chain.last().ok_or("Invalid FAT chain")?;
            for _ in chain.len()..required_clusters {
                let next = self.find_free_cluster()?;
                self.write_fat_entry(prev, next)?;
                self.write_fat_entry(next, FAT32_EOC)?;
                chain.push(next);
                prev = next;
                metadata_steps = metadata_steps.saturating_add(1);
                if (metadata_steps & 0x0F) == 0 && !progress(0, total_len) {
                    return Err("Operation canceled");
                }
            }
        }
        if !progress(0, total_len) {
            return Err("Operation canceled");
        }

        if chain.len() > required_clusters {
            let keep_last = chain[required_clusters - 1];
            let tail_start = chain[required_clusters];
            self.write_fat_entry(keep_last, FAT32_EOC)?;
            self.free_cluster_chain(tail_start)?;
            self.next_free_cluster_hint = tail_start;
            chain.truncate(required_clusters);
        } else {
            let last = *chain.last().ok_or("Invalid FAT chain")?;
            self.write_fat_entry(last, FAT32_EOC)?;
        }
        if !progress(0, total_len) {
            return Err("Operation canceled");
        }

        let copy_io_bytes = Self::recommended_copy_io_bytes(src_fat, self);
        let copy_chunk_sectors = (copy_io_bytes / SECTOR_SIZE).max(1);
        let src_total_sectors = (total_len + SECTOR_SIZE - 1) / SECTOR_SIZE;
        let dst_total_sectors = required_clusters.saturating_mul(self.sectors_per_cluster as usize);

        let mut copied = 0usize;
        let mut src_chain_idx = 0usize;
        let mut src_sec_idx = 0usize;
        let mut dst_chain_idx = 0usize;
        let mut dst_sec_idx = 0usize;
        let mut src_sectors_left = src_total_sectors;
        let mut dst_sectors_left = dst_total_sectors;
        let mut src_run_lba = 0u64;
        let mut src_run_left = 0usize;

        let copy_io = unsafe { &mut FAT32_COPY_IO_BUFFER.0 };

        while dst_sectors_left > 0 {
            let Some((mut dst_lba, mut dst_run_left)) = self.next_contiguous_lba_run(
                chain.as_slice(),
                self.sectors_per_cluster as usize,
                &mut dst_chain_idx,
                &mut dst_sec_idx,
                dst_sectors_left,
            ) else {
                return Err("Data write failed");
            };

            while dst_run_left > 0 {
                if src_sectors_left > 0 && src_run_left == 0 {
                    let Some((next_src_lba, next_src_run)) = src_fat.next_contiguous_lba_run(
                        src_chain.as_slice(),
                        src_fat.sectors_per_cluster as usize,
                        &mut src_chain_idx,
                        &mut src_sec_idx,
                        src_sectors_left,
                    ) else {
                        return Err("Source read failed");
                    };
                    src_run_lba = next_src_lba;
                    src_run_left = next_src_run;
                }

                let mut sectors_this_step = core::cmp::min(copy_chunk_sectors, dst_run_left);
                let use_source_data = src_sectors_left > 0;
                if use_source_data {
                    sectors_this_step = core::cmp::min(sectors_this_step, src_run_left);
                    sectors_this_step = core::cmp::min(sectors_this_step, src_sectors_left);
                }
                if sectors_this_step == 0 {
                    return Err("Copy scheduling failed");
                }

                let bytes_this_step = sectors_this_step * SECTOR_SIZE;
                let chunk = &mut copy_io[..bytes_this_step];

                if use_source_data {
                    if !src_fat.read_sector_span(src_run_lba, sectors_this_step, chunk) {
                        return Err("Source read failed");
                    }

                    // Pad the last partial logical sector with zeroes.
                    let remaining_file_bytes = total_len.saturating_sub(copied);
                    if remaining_file_bytes < bytes_this_step {
                        chunk[remaining_file_bytes..bytes_this_step].fill(0);
                    }
                } else {
                    chunk.fill(0);
                }

                if !self.write_sector_span(dst_lba, sectors_this_step, chunk) {
                    return Err("Data write failed");
                }

                if use_source_data {
                    let visible = core::cmp::min(total_len.saturating_sub(copied), bytes_this_step);
                    copied = copied.saturating_add(visible);
                    src_sectors_left = src_sectors_left.saturating_sub(sectors_this_step);
                    src_run_left = src_run_left.saturating_sub(sectors_this_step);
                    src_run_lba = src_run_lba.saturating_add(sectors_this_step as u64);
                }

                dst_lba = dst_lba.saturating_add(sectors_this_step as u64);
                dst_run_left = dst_run_left.saturating_sub(sectors_this_step);
                dst_sectors_left = dst_sectors_left.saturating_sub(sectors_this_step);

                if !progress(copied.min(total_len), total_len) {
                    return Err("Operation canceled");
                }
            }
        }

        Self::set_entry_cluster(&mut entries[idx], chain[0]);
        entries[idx].size = total_len as u32;

        if !self.write_sector(slot_lba, &dir_sector) {
            return Err("Directory write failed");
        }
        if !progress(total_len, total_len) {
            return Err("Operation canceled");
        }

        Ok(total_len)
    }

    fn directory_is_empty(&mut self, cluster: u32) -> Result<bool, &'static str> {
        let entries = self.read_dir_entries(cluster)?;
        for entry in entries.iter() {
            if !entry.valid {
                continue;
            }
            if entry.matches_name(".") || entry.matches_name("..") {
                continue;
            }
            return Ok(false);
        }
        Ok(true)
    }

    pub fn rename_entry_in_dir(
        &mut self,
        dir_cluster: u32,
        from_name: &str,
        to_name: &str,
        expect_directory: Option<bool>,
    ) -> Result<(), &'static str> {
        if self.bytes_per_sector == 0 {
            return Err("Filesystem not initialized");
        }

        let from_short = Self::to_short_name(from_name).ok_or("Invalid source 8.3 filename")?;
        let to_short = Self::to_short_name(to_name)
            .or_else(|| Self::to_short_name_relaxed(to_name))
            .ok_or("Invalid destination filename")?;
        if from_short == to_short {
            return Ok(());
        }

        let dir_cluster = self.normalized_dir_cluster(dir_cluster);
        let dir_chain = self.read_cluster_chain(dir_cluster, 1024)?;
        if dir_chain.is_empty() {
            return Err("Directory read failed");
        }

        let cluster_size = self.cluster_size_bytes();
        let mut dir_bytes = Vec::new();
        dir_bytes.resize(cluster_size.saturating_mul(dir_chain.len()), 0);

        for (ci, &cluster) in dir_chain.iter().enumerate() {
            for sec in 0..self.sectors_per_cluster as usize {
                let lba = self.cluster_to_lba(cluster) + sec as u64;
                let start = ci
                    .saturating_mul(cluster_size)
                    .saturating_add(sec.saturating_mul(SECTOR_SIZE));
                let end = start.saturating_add(SECTOR_SIZE);
                if end > dir_bytes.len() {
                    return Err("Directory read failed");
                }
                if !self.read_sector(lba, &mut dir_bytes[start..end]) {
                    return Err("Directory read failed");
                }
            }
        }

        let mut pending_lfn_offsets: Vec<usize> = Vec::new();
        let mut target_offset: Option<usize> = None;
        let mut target_lfn_offsets: Vec<usize> = Vec::new();
        let mut type_mismatch = false;
        let entry_count = dir_bytes.len() / 32;

        for idx in 0..entry_count {
            let off = idx * 32;
            let first = dir_bytes[off];
            if first == 0x00 {
                break;
            }
            if first == 0xE5 {
                pending_lfn_offsets.clear();
                continue;
            }

            let attr = dir_bytes[off + 11];
            if (attr & FAT32_DIR_ATTR_LFN) == FAT32_DIR_ATTR_LFN {
                pending_lfn_offsets.push(off);
                continue;
            }
            if (attr & 0x08) != 0 {
                pending_lfn_offsets.clear();
                continue;
            }

            let is_directory = (attr & 0x10) != 0;
            let mut short_name = [0u8; 11];
            short_name.copy_from_slice(&dir_bytes[off..off + 11]);

            if short_name == to_short {
                return Err("Destination already exists");
            }

            let type_matches = expect_directory.map(|want| want == is_directory).unwrap_or(true);
            if short_name == from_short {
                if type_matches {
                    target_offset = Some(off);
                    target_lfn_offsets = pending_lfn_offsets.clone();
                    break;
                }
                type_mismatch = true;
            }

            pending_lfn_offsets.clear();
        }

        let Some(target_off) = target_offset else {
            if type_mismatch {
                return Err("Entry type mismatch");
            }
            return Err("Entry not found");
        };

        dir_bytes[target_off..target_off + 11].copy_from_slice(&to_short);
        for lfn_off in target_lfn_offsets.into_iter() {
            if lfn_off < dir_bytes.len() {
                dir_bytes[lfn_off] = 0xE5;
            }
        }

        for (ci, &cluster) in dir_chain.iter().enumerate() {
            for sec in 0..self.sectors_per_cluster as usize {
                let lba = self.cluster_to_lba(cluster) + sec as u64;
                let start = ci
                    .saturating_mul(cluster_size)
                    .saturating_add(sec.saturating_mul(SECTOR_SIZE));
                let end = start.saturating_add(SECTOR_SIZE);
                if end > dir_bytes.len() {
                    return Err("Directory write failed");
                }
                if !self.write_sector(lba, &dir_bytes[start..end]) {
                    return Err("Directory write failed");
                }
            }
        }

        Ok(())
    }

    pub fn delete_directory_in_dir(
        &mut self,
        dir_cluster: u32,
        dirname: &str,
    ) -> Result<(), &'static str> {
        if self.bytes_per_sector == 0 {
            return Err("Filesystem not initialized");
        }

        let short_name = Self::to_short_name(dirname).ok_or("Invalid 8.3 filename")?;
        let dir_cluster = self.normalized_dir_cluster(dir_cluster);

        let dir_chain = self.read_cluster_chain(dir_cluster, 1024)?;
        if dir_chain.is_empty() {
            return Err("Directory read failed");
        }

        for &cluster in dir_chain.iter() {
            for sec in 0..self.sectors_per_cluster as usize {
                let lba = self.cluster_to_lba(cluster) + sec as u64;
                let mut dir_sector = [0u8; SECTOR_SIZE];
                if !self.read_sector(lba, &mut dir_sector) {
                    return Err("Directory read failed");
                }

                let entries = unsafe {
                    core::slice::from_raw_parts_mut(dir_sector.as_mut_ptr() as *mut FatDirEntry, 16)
                };

                for i in 0..entries.len() {
                    let e = &entries[i];
                    if e.name[0] == 0 {
                        return Err("Directory not found");
                    }
                    if e.name[0] == 0xE5 {
                        continue;
                    }
                    if (e.attr & 0x0F) == 0x0F || (e.attr & 0x08) != 0 {
                        continue;
                    }
                    if e.name != short_name {
                        continue;
                    }

                    if (entries[i].attr & 0x10) == 0 {
                        return Err("Target is a file");
                    }

                    let target_cluster = Self::entry_cluster(&entries[i]);
                    let target_cluster = self.normalized_dir_cluster(target_cluster);
                    if target_cluster == self.root_cluster {
                        return Err("Cannot delete root directory");
                    }

                    if !self.directory_is_empty(target_cluster)? {
                        return Err("Directory not empty");
                    }

                    if target_cluster >= 2 {
                        self.free_cluster_chain(target_cluster)?;
                    }

                    entries[i].name[0] = 0xE5;
                    entries[i].size = 0;
                    Self::set_entry_cluster(&mut entries[i], 0);

                    if !self.write_sector(lba, &dir_sector) {
                        return Err("Directory write failed");
                    }

                    return Ok(());
                }
            }
        }

        Err("Directory not found")
    }

    pub fn delete_file_in_dir(&mut self, dir_cluster: u32, filename: &str) -> Result<(), &'static str> {
        if self.bytes_per_sector == 0 {
            return Err("Filesystem not initialized");
        }

        let short_name = Self::to_short_name(filename).ok_or("Invalid 8.3 filename")?;
        let dir_cluster = self.normalized_dir_cluster(dir_cluster);

        // Walk all directory clusters to find the entry
        let dir_chain = self.read_cluster_chain(dir_cluster, 1024)?;
        if dir_chain.is_empty() {
            return Err("Directory read failed");
        }

        for &cluster in dir_chain.iter() {
            for sec in 0..self.sectors_per_cluster as usize {
                let lba = self.cluster_to_lba(cluster) + sec as u64;
                let mut dir_sector = [0u8; SECTOR_SIZE];
                if !self.read_sector(lba, &mut dir_sector) {
                    return Err("Directory read failed");
                }

                let entries = unsafe {
                    core::slice::from_raw_parts_mut(dir_sector.as_mut_ptr() as *mut FatDirEntry, 16)
                };

                for i in 0..entries.len() {
                    let e = &entries[i];
                    if e.name[0] == 0 {
                        return Err("File not found");
                    }
                    if e.name[0] == 0xE5 {
                        continue;
                    }
                    if (e.attr & 0x0F) == 0x0F || (e.attr & 0x08) != 0 {
                        continue;
                    }
                    if e.name == short_name {
                        if (entries[i].attr & 0x10) != 0 {
                            return Err("Cannot delete directory");
                        }

                        let file_cluster = Self::entry_cluster(&entries[i]);
                        if file_cluster >= 2 {
                            self.free_cluster_chain(file_cluster)?;
                        }

                        entries[i].name[0] = 0xE5;
                        entries[i].size = 0;
                        Self::set_entry_cluster(&mut entries[i], 0);

                        if !self.write_sector(lba, &dir_sector) {
                            return Err("Directory write failed");
                        }

                        return Ok(());
                    }
                }
            }
        }

        Err("File not found")
    }

    pub fn empty_directory(&mut self, dir_cluster: u32) -> Result<(), &'static str> {
        if self.bytes_per_sector == 0 {
            return Err("Filesystem not initialized");
        }
        let dir_cluster = self.normalized_dir_cluster(dir_cluster);
        if dir_cluster == self.root_cluster {
            return Err("Cannot empty root directory");
        }

        let dir_chain = self.read_cluster_chain(dir_cluster, 1024)?;
        if dir_chain.is_empty() {
            return Err("Directory read failed");
        }

        let mut modified = false;
        for &cluster in dir_chain.iter() {
            for sec in 0..self.sectors_per_cluster as usize {
                let lba = self.cluster_to_lba(cluster) + sec as u64;
                let mut dir_sector = [0u8; SECTOR_SIZE];
                if !self.read_sector(lba, &mut dir_sector) {
                    continue;
                }
                let entries = unsafe {
                    core::slice::from_raw_parts_mut(dir_sector.as_mut_ptr() as *mut FatDirEntry, 16)
                };
                let mut sector_modified = false;

                for i in 0..entries.len() {
                    let e = &entries[i];
                    if e.name[0] == 0 || e.name[0] == 0xE5 {
                        continue;
                    }
                    if (e.attr & 0x0F) == 0x0F || (e.attr & 0x08) != 0 {
                        continue;
                    }
                    // Skip . and ..
                    if e.name[0] == b'.' && e.name[1] == b' ' { continue; }
                    if e.name[0] == b'.' && e.name[1] == b'.' && e.name[2] == b' ' { continue; }

                    let target_cluster = Self::entry_cluster(&entries[i]);
                    let target_cluster = self.normalized_dir_cluster(target_cluster);
                    let is_dir = (e.attr & 0x10) != 0;

                    if is_dir {
                        let _ = self.empty_directory(target_cluster);
                    }
                    
                    if target_cluster >= 2 {
                        let _ = self.free_cluster_chain(target_cluster);
                    }
                    entries[i].name[0] = 0xE5;
                    sector_modified = true;
                }
                if sector_modified {
                    let _ = self.write_sector(lba, &dir_sector);
                    modified = true;
                }
            }
        }
        if modified { Ok(()) } else { Err("No files to delete or error reading") }
    }

    pub fn resolve_path(&mut self, start_cluster: u32, path: &str) -> Result<(u32, u32), &'static str> {
        let mut current_dir = start_cluster;
        let mut target_cluster = current_dir;
        
        for part in path.split('/') {
            if part.is_empty() { continue; }
            let short_name = Self::to_short_name(part).ok_or("Invalid path component")?;
            
            let dir_chain = self.read_cluster_chain(current_dir, 1024)?;
            let mut found = false;
            
            'search: for &cluster in dir_chain.iter() {
                for sec in 0..self.sectors_per_cluster as usize {
                    let lba = self.cluster_to_lba(cluster) + sec as u64;
                    let mut dir_sector = [0u8; SECTOR_SIZE];
                    if !self.read_sector(lba, &mut dir_sector) { continue; }
                    
                    let entries = unsafe {
                        core::slice::from_raw_parts(dir_sector.as_ptr() as *const FatDirEntry, 16)
                    };
                    
                    for e in entries.iter() {
                        if e.name[0] == 0 { break 'search; }
                        if e.name[0] == 0xE5 || (e.attr & 0x0F) == 0x0F || (e.attr & 0x08) != 0 { continue; }
                        
                        if e.name == short_name {
                            current_dir = Self::entry_cluster(e);
                            target_cluster = current_dir;
                            found = true;
                            break 'search;
                        }
                    }
                }
            }
            if !found { return Err("Path not found"); }
        }
        Ok((start_cluster, target_cluster))
    }

    pub fn move_entry(&mut self, src_dir_cluster: u32, dst_dir_cluster: u32, filename: &str) -> Result<(), &'static str> {
        let short_name = Self::to_short_name(filename).ok_or("Invalid filename")?;
        
        let src_chain = self.read_cluster_chain(src_dir_cluster, 1024)?;
        let mut target_entry = None;
        let mut src_lba = 0;
        let mut src_sector = [0u8; SECTOR_SIZE];
        
        'search: for &cluster in src_chain.iter() {
            for sec in 0..self.sectors_per_cluster as usize {
                let lba = self.cluster_to_lba(cluster) + sec as u64;
                if !self.read_sector(lba, &mut src_sector) { continue; }
                
                let entries = unsafe { core::slice::from_raw_parts_mut(src_sector.as_mut_ptr() as *mut FatDirEntry, 16) };
                for i in 0..entries.len() {
                    let e = &mut entries[i];
                    if e.name[0] == 0 { break 'search; }
                    if e.name[0] == 0xE5 || (e.attr & 0x0F) == 0x0F || (e.attr & 0x08) != 0 { continue; }
                    
                    if e.name == short_name {
                        target_entry = Some(e.clone());
                        e.name[0] = 0xE5; // Mark as deleted in source
                        src_lba = lba;
                        break 'search;
                    }
                }
            }
        }
        
        let mut entry = target_entry.ok_or("Entry not found")?;
        
        // Find empty slot in destination
        let mut dst_chain = self.read_cluster_chain(dst_dir_cluster, 1024)?;
        if dst_chain.is_empty() {
             return Err("Destination directory doesn't exist");
        }
        
        for &cluster in dst_chain.iter() {
             for sec in 0..self.sectors_per_cluster as usize {
                 let lba = self.cluster_to_lba(cluster) + sec as u64;
                 let mut dir_sector = [0u8; SECTOR_SIZE];
                 if !self.read_sector(lba, &mut dir_sector) { continue; }
                 
                 let entries = unsafe { core::slice::from_raw_parts_mut(dir_sector.as_mut_ptr() as *mut FatDirEntry, 16) };
                 for i in 0..entries.len() {
                     if entries[i].name[0] == 0 || entries[i].name[0] == 0xE5 {
                         entries[i] = entry; // Place it
                         if !self.write_sector(lba, &dir_sector) {
                             return Err("Failed to write to destination");
                         }
                         // Commit deletion at source now that it's moved
                         self.write_sector(src_lba, &src_sector);
                         return Ok(());
                     }
                 }
             }
        }
        
        // If no slot was found, append a cluster to the destination chain.
        let new_cluster = self.find_free_cluster()?;
        self.write_fat_entry(new_cluster, FAT32_EOC)?;
        let last_cluster = *dst_chain.last().unwrap();
        self.write_fat_entry(last_cluster, new_cluster)?;
        
        let zero_sector = [0u8; SECTOR_SIZE];
        for sec in 0..self.sectors_per_cluster as usize {
            let lba = self.cluster_to_lba(new_cluster) + sec as u64;
            self.write_sector(lba, &zero_sector);
        }
        
        let new_lba = self.cluster_to_lba(new_cluster);
        let mut dir_sector = [0u8; SECTOR_SIZE];
        self.read_sector(new_lba, &mut dir_sector);
        let entries = unsafe { core::slice::from_raw_parts_mut(dir_sector.as_mut_ptr() as *mut FatDirEntry, 16) };
        entries[0] = entry;
        
        self.write_sector(new_lba, &dir_sector);
        self.write_sector(src_lba, &src_sector);
        
        Ok(())
    }
}
