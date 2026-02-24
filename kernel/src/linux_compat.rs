use alloc::string::String;
use alloc::vec::Vec;
#[cfg(target_arch = "x86_64")]
use core::arch::asm;
use core::ptr;

pub const ELF_MAX_FILE_BYTES: usize = 256 * 1024 * 1024;
pub const ELF_MAX_STAGED_IMAGE_BYTES: usize = 256 * 1024 * 1024;

const ELF_HEADER_SIZE: usize = 64;
const ELF_CLASS_64: u8 = 2;
const ELF_DATA_LE: u8 = 1;
const ELF_VERSION_CURRENT: u8 = 1;

const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;
const EM_X86_64: u16 = 62;

const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const PT_INTERP: u32 = 3;
const PT_TLS: u32 = 7;

const DT_NULL: i64 = 0;
const DT_NEEDED: i64 = 1;
const DT_HASH: i64 = 4;
const DT_SYMTAB: i64 = 6;
const DT_PLTRELSZ: i64 = 2;
const DT_STRTAB: i64 = 5;
const DT_SYMENT: i64 = 11;
const DT_RELA: i64 = 7;
const DT_RELASZ: i64 = 8;
const DT_RELAENT: i64 = 9;
const DT_STRSZ: i64 = 10;
const DT_REL: i64 = 17;
const DT_RELSZ: i64 = 18;
const DT_RELENT: i64 = 19;
const DT_PLTREL: i64 = 20;
const DT_JMPREL: i64 = 23;
const DT_SONAME: i64 = 14;
const DT_RPATH: i64 = 15;
const DT_RUNPATH: i64 = 29;

const DT_PLTREL_REL: u64 = 17;
const DT_PLTREL_RELA: u64 = 7;
const R_X86_64_64: u32 = 1;
const R_X86_64_COPY: u32 = 5;
const R_X86_64_GLOB_DAT: u32 = 6;
const R_X86_64_JUMP_SLOT: u32 = 7;
const R_X86_64_RELATIVE: u32 = 8;

const PAGE_SIZE: u64 = 4096;
const LINUX_STACK_SIZE: usize = 128 * 1024;

const AT_NULL: u64 = 0;
const AT_PHDR: u64 = 3;
const AT_PHENT: u64 = 4;
const AT_PHNUM: u64 = 5;
const AT_PAGESZ: u64 = 6;
const AT_BASE: u64 = 7;
const AT_FLAGS: u64 = 8;
const AT_ENTRY: u64 = 9;
const AT_UID: u64 = 11;
const AT_EUID: u64 = 12;
const AT_GID: u64 = 13;
const AT_EGID: u64 = 14;
const AT_SECURE: u64 = 23;
const AT_RANDOM: u64 = 25;
const AT_EXECFN: u64 = 31;

pub const PHASE2_MAIN_LOAD_BIAS: u64 = 0x0000_0004_0000_0000;
pub const PHASE2_INTERP_LOAD_BIAS: u64 = 0x0000_0006_0000_0000;

#[derive(Clone)]
pub struct ElfLoadSegment {
    pub file_offset: u64,
    pub file_size: u64,
    pub vaddr: u64,
    pub mem_size: u64,
}

#[derive(Clone)]
pub struct ElfInspectReport {
    pub e_type: u16,
    pub machine: u16,
    pub entry: u64,
    pub ph_count: u16,
    pub load_segments: Vec<ElfLoadSegment>,
    pub load_file_bytes: u64,
    pub load_mem_bytes: u64,
    pub span_start: u64,
    pub span_end: u64,
    pub has_interp: bool,
    pub interp_path: Option<String>,
    pub has_dynamic: bool,
    pub has_tls: bool,
    pub tls_vaddr: u64,
    pub tls_offset: u64,
    pub tls_filesz: u64,
    pub tls_memsz: u64,
    pub tls_align: u64,
    pub syscall_sites: usize,
}

pub struct Phase1StageReport {
    pub span_start: u64,
    pub span_size: u64,
    pub entry_virt: u64,
    pub entry_offset: u64,
    pub load_segments: usize,
    pub syscall_sites: usize,
    pub sample_hash: u32,
}

pub struct DynamicInspectReport {
    pub interp_path: Option<String>,
    pub needed: Vec<String>,
    pub soname: Option<String>,
    pub rpath: Option<String>,
    pub runpath: Option<String>,
    pub strtab_virt: u64,
    pub strtab_size: u64,
}

pub struct Phase2DynamicStageReport {
    pub load_bias: u64,
    pub image_start: u64,
    pub image_size: u64,
    pub entry_virt: u64,
    pub entry_offset: u64,
    pub load_segments: usize,
    pub sample_hash: u32,
}

struct RuntimeDynImage {
    report: ElfInspectReport,
    dyn_info: RuntimeRelocDynamicInfo,
    image: Vec<u8>,
    phdr_blob: Vec<u8>,
    tls_block: Vec<u8>,
    tls_tcb_addr: u64,
    image_start: u64,
    image_size: u64,
    load_bias: u64,
    entry_virt: u64,
    phdr_addr: u64,
    phent: u64,
    phnum: u64,
    sample_hash: u32,
    reloc_total: u32,
    reloc_applied: u32,
    reloc_unsupported: u32,
    reloc_errors: u32,
}

pub struct LinuxDynLaunchPlan {
    pub main_base: u64,
    pub main_entry: u64,
    pub interp_base: u64,
    pub interp_entry: u64,
    pub stack_ptr: u64,
    pub stack_bytes: usize,
    pub argv_count: usize,
    pub env_count: usize,
    pub aux_pairs: usize,
    pub main_hash: u32,
    pub interp_hash: u32,
    pub tls_tcb_addr: u64,
    pub main_reloc_total: u32,
    pub main_reloc_applied: u32,
    pub main_reloc_unsupported: u32,
    pub main_reloc_errors: u32,
    pub interp_reloc_total: u32,
    pub interp_reloc_applied: u32,
    pub interp_reloc_unsupported: u32,
    pub interp_reloc_errors: u32,
    pub symbol_traces: Vec<LinuxDynSymbolTrace>,
    main_image: RuntimeDynImage,
    interp_image: RuntimeDynImage,
    stack_image: Vec<u8>,
}

pub struct LinuxDynSymbolTrace {
    pub requestor: String,
    pub symbol: String,
    pub provider: String,
    pub reloc_kind: String,
    pub slot_addr: u64,
    pub value_addr: u64,
}

fn read_u16_le_at(raw: &[u8], off: usize) -> Option<u16> {
    if off + 2 > raw.len() {
        return None;
    }
    Some(u16::from_le_bytes([raw[off], raw[off + 1]]))
}

fn read_u32_le_at(raw: &[u8], off: usize) -> Option<u32> {
    if off + 4 > raw.len() {
        return None;
    }
    Some(u32::from_le_bytes([
        raw[off],
        raw[off + 1],
        raw[off + 2],
        raw[off + 3],
    ]))
}

fn read_u64_le_at(raw: &[u8], off: usize) -> Option<u64> {
    if off + 8 > raw.len() {
        return None;
    }
    Some(u64::from_le_bytes([
        raw[off],
        raw[off + 1],
        raw[off + 2],
        raw[off + 3],
        raw[off + 4],
        raw[off + 5],
        raw[off + 6],
        raw[off + 7],
    ]))
}

fn read_i64_le_at(raw: &[u8], off: usize) -> Option<i64> {
    if off + 8 > raw.len() {
        return None;
    }
    Some(i64::from_le_bytes([
        raw[off],
        raw[off + 1],
        raw[off + 2],
        raw[off + 3],
        raw[off + 4],
        raw[off + 5],
        raw[off + 6],
        raw[off + 7],
    ]))
}

fn align_down(value: u64, align: u64) -> u64 {
    if align == 0 {
        return value;
    }
    value & !(align - 1)
}

fn align_up(value: u64, align: u64) -> Option<u64> {
    if align == 0 {
        return Some(value);
    }
    let plus = align.saturating_sub(1);
    value.checked_add(plus).map(|v| align_down(v, align))
}

fn u64_to_usize(value: u64) -> Option<usize> {
    if value > usize::MAX as u64 {
        return None;
    }
    Some(value as usize)
}

fn checked_range(raw_len: usize, offset: u64, size: u64) -> Option<(usize, usize)> {
    let start = u64_to_usize(offset)?;
    let len = u64_to_usize(size)?;
    let end = start.checked_add(len)?;
    if end > raw_len {
        return None;
    }
    Some((start, end))
}

fn vaddr_to_file_offset(load_segments: &[ElfLoadSegment], vaddr: u64) -> Option<u64> {
    for seg in load_segments.iter() {
        if seg.file_size == 0 {
            continue;
        }
        let file_end = seg.vaddr.checked_add(seg.file_size)?;
        if vaddr < seg.vaddr || vaddr >= file_end {
            continue;
        }
        let rel = vaddr.checked_sub(seg.vaddr)?;
        let off = seg.file_offset.checked_add(rel)?;
        return Some(off);
    }
    None
}

fn read_dynamic_string(
    raw: &[u8],
    report: &ElfInspectReport,
    strtab_virt: u64,
    strtab_size: u64,
    str_offset: u64,
) -> Option<String> {
    if strtab_size == 0 || str_offset >= strtab_size {
        return None;
    }

    let str_addr = strtab_virt.checked_add(str_offset)?;
    let str_file_off = vaddr_to_file_offset(report.load_segments.as_slice(), str_addr)?;
    let str_start = u64_to_usize(str_file_off)?;
    let max_len = u64_to_usize(strtab_size.checked_sub(str_offset)?)?;
    let str_limit = str_start.checked_add(max_len)?;
    if str_limit > raw.len() {
        return None;
    }

    let mut str_end = str_start;
    while str_end < str_limit {
        if raw[str_end] == 0 {
            break;
        }
        str_end += 1;
    }
    if str_end == str_limit {
        return None;
    }

    Some(String::from_utf8_lossy(&raw[str_start..str_end]).into_owned())
}

fn parse_c_string(bytes: &[u8]) -> String {
    let mut end = bytes.len();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == 0 {
            end = i;
            break;
        }
        i += 1;
    }
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn count_syscall_sites(raw: &[u8]) -> usize {
    let mut count = 0usize;
    let mut i = 0usize;
    while i + 1 < raw.len() {
        if raw[i] == 0x0F && raw[i + 1] == 0x05 {
            count += 1;
            i += 2;
        } else {
            i += 1;
        }
    }
    count
}

pub fn elf_type_name(e_type: u16) -> &'static str {
    match e_type {
        0 => "NONE",
        1 => "REL",
        2 => "EXEC",
        3 => "DYN",
        4 => "CORE",
        _ => "OTHER",
    }
}

pub fn machine_name(machine: u16) -> &'static str {
    match machine {
        62 => "x86_64",
        3 => "x86",
        183 => "AArch64",
        40 => "ARM",
        _ => "OTHER",
    }
}

pub fn inspect_elf64(raw: &[u8]) -> Result<ElfInspectReport, &'static str> {
    if raw.len() < ELF_HEADER_SIZE {
        return Err("ELF invalido: archivo demasiado pequeno.");
    }
    if raw.len() > ELF_MAX_FILE_BYTES {
        return Err("ELF invalido: archivo demasiado grande.");
    }
    if &raw[0..4] != b"\x7FELF" {
        return Err("ELF invalido: magic.");
    }
    if raw[4] != ELF_CLASS_64 {
        return Err("ELF invalido: solo ELF64 soportado.");
    }
    if raw[5] != ELF_DATA_LE {
        return Err("ELF invalido: solo little-endian soportado.");
    }
    if raw[6] != ELF_VERSION_CURRENT {
        return Err("ELF invalido: version de identificacion.");
    }

    let e_type = read_u16_le_at(raw, 16).ok_or("ELF invalido: e_type.")?;
    let machine = read_u16_le_at(raw, 18).ok_or("ELF invalido: e_machine.")?;
    let e_version = read_u32_le_at(raw, 20).ok_or("ELF invalido: e_version.")?;
    if e_version != 1 {
        return Err("ELF invalido: e_version.");
    }
    let entry = read_u64_le_at(raw, 24).ok_or("ELF invalido: e_entry.")?;
    let phoff = read_u64_le_at(raw, 32).ok_or("ELF invalido: e_phoff.")?;
    let phentsize = read_u16_le_at(raw, 54).ok_or("ELF invalido: e_phentsize.")?;
    let phnum = read_u16_le_at(raw, 56).ok_or("ELF invalido: e_phnum.")?;

    if phnum == 0 {
        return Err("ELF invalido: no hay program headers.");
    }
    if phentsize < 56 {
        return Err("ELF invalido: tamano de program header.");
    }

    let phoff_usize = u64_to_usize(phoff).ok_or("ELF invalido: e_phoff fuera de rango.")?;
    let phentsize_usize = phentsize as usize;
    let table_len = phentsize_usize
        .checked_mul(phnum as usize)
        .ok_or("ELF invalido: overflow en tabla PH.")?;
    let table_end = phoff_usize
        .checked_add(table_len)
        .ok_or("ELF invalido: overflow en tabla PH.")?;
    if table_end > raw.len() {
        return Err("ELF invalido: tabla PH fuera de rango.");
    }

    let mut load_segments = Vec::new();
    let mut load_file_bytes = 0u64;
    let mut load_mem_bytes = 0u64;
    let mut min_vaddr = u64::MAX;
    let mut max_vaddr = 0u64;
    let mut has_interp = false;
    let mut interp_path: Option<String> = None;
    let mut has_dynamic = false;
    let mut has_tls = false;
    let mut tls_vaddr = 0u64;
    let mut tls_offset = 0u64;
    let mut tls_filesz = 0u64;
    let mut tls_memsz = 0u64;
    let mut tls_align = 1u64;

    let mut i = 0usize;
    while i < phnum as usize {
        let off = phoff_usize + i * phentsize_usize;

        let p_type = read_u32_le_at(raw, off).ok_or("ELF invalido: p_type.")?;
        let p_offset = read_u64_le_at(raw, off + 8).ok_or("ELF invalido: p_offset.")?;
        let p_vaddr = read_u64_le_at(raw, off + 16).ok_or("ELF invalido: p_vaddr.")?;
        let p_filesz = read_u64_le_at(raw, off + 32).ok_or("ELF invalido: p_filesz.")?;
        let p_memsz = read_u64_le_at(raw, off + 40).ok_or("ELF invalido: p_memsz.")?;

        if p_filesz > p_memsz {
            return Err("ELF invalido: p_filesz > p_memsz.");
        }

        if p_type == PT_INTERP {
            has_interp = true;
            let (start, end) =
                checked_range(raw.len(), p_offset, p_filesz).ok_or("ELF invalido: PT_INTERP.")?;
            interp_path = Some(parse_c_string(&raw[start..end]));
        } else if p_type == PT_DYNAMIC {
            has_dynamic = true;
        } else if p_type == PT_TLS {
            has_tls = true;
            tls_vaddr = p_vaddr;
            tls_offset = p_offset;
            tls_filesz = p_filesz;
            tls_memsz = p_memsz;
            let p_align_val = read_u64_le_at(raw, off + 48).unwrap_or(1);
            tls_align = if p_align_val == 0 { 1 } else { p_align_val };
        } else if p_type == PT_LOAD {
            let _ =
                checked_range(raw.len(), p_offset, p_filesz).ok_or("ELF invalido: PT_LOAD.")?;

            load_file_bytes = load_file_bytes.saturating_add(p_filesz);
            load_mem_bytes = load_mem_bytes.saturating_add(p_memsz);
            if p_vaddr < min_vaddr {
                min_vaddr = p_vaddr;
            }
            let seg_end = p_vaddr
                .checked_add(p_memsz)
                .ok_or("ELF invalido: overflow en PT_LOAD.")?;
            if seg_end > max_vaddr {
                max_vaddr = seg_end;
            }

            load_segments.push(ElfLoadSegment {
                file_offset: p_offset,
                file_size: p_filesz,
                vaddr: p_vaddr,
                mem_size: p_memsz,
            });
        }

        i += 1;
    }

    if load_segments.is_empty() {
        return Err("ELF invalido: sin segmentos PT_LOAD.");
    }

    let span_start = align_down(min_vaddr, PAGE_SIZE);
    let span_end = align_up(max_vaddr, PAGE_SIZE).ok_or("ELF invalido: overflow en span.")?;
    if span_end <= span_start {
        return Err("ELF invalido: span de carga vacio.");
    }

    Ok(ElfInspectReport {
        e_type,
        machine,
        entry,
        ph_count: phnum,
        load_segments,
        load_file_bytes,
        load_mem_bytes,
        span_start,
        span_end,
        has_interp,
        interp_path,
        has_dynamic,
        has_tls,
        tls_vaddr,
        tls_offset,
        tls_filesz,
        tls_memsz,
        tls_align,
        syscall_sites: count_syscall_sites(raw),
    })
}

pub fn inspect_dynamic_elf64(raw: &[u8]) -> Result<DynamicInspectReport, &'static str> {
    let report = inspect_elf64(raw)?;
    if report.machine != EM_X86_64 {
        return Err("ELF dinamico: requiere x86_64.");
    }
    if !report.has_dynamic {
        return Err("ELF dinamico: falta PT_DYNAMIC.");
    }

    let phoff = read_u64_le_at(raw, 32).ok_or("ELF dinamico: e_phoff.")?;
    let phentsize = read_u16_le_at(raw, 54).ok_or("ELF dinamico: e_phentsize.")?;
    let phnum = read_u16_le_at(raw, 56).ok_or("ELF dinamico: e_phnum.")?;
    if phnum == 0 {
        return Err("ELF dinamico: no hay program headers.");
    }
    if phentsize < 56 {
        return Err("ELF dinamico: tamano de PH invalido.");
    }

    let phoff_usize = u64_to_usize(phoff).ok_or("ELF dinamico: e_phoff fuera de rango.")?;
    let phentsize_usize = phentsize as usize;
    let table_len = phentsize_usize
        .checked_mul(phnum as usize)
        .ok_or("ELF dinamico: overflow en tabla PH.")?;
    let table_end = phoff_usize
        .checked_add(table_len)
        .ok_or("ELF dinamico: overflow en tabla PH.")?;
    if table_end > raw.len() {
        return Err("ELF dinamico: tabla PH fuera de rango.");
    }

    let mut dyn_range: Option<(usize, usize)> = None;
    let mut i = 0usize;
    while i < phnum as usize {
        let off = phoff_usize + i * phentsize_usize;
        let p_type = read_u32_le_at(raw, off).ok_or("ELF dinamico: p_type.")?;
        if p_type == PT_DYNAMIC {
            let p_offset = read_u64_le_at(raw, off + 8).ok_or("ELF dinamico: p_offset.")?;
            let p_filesz = read_u64_le_at(raw, off + 32).ok_or("ELF dinamico: p_filesz.")?;
            if p_filesz == 0 {
                return Err("ELF dinamico: PT_DYNAMIC vacio.");
            }
            dyn_range = Some(
                checked_range(raw.len(), p_offset, p_filesz)
                    .ok_or("ELF dinamico: PT_DYNAMIC fuera de rango.")?,
            );
            break;
        }
        i += 1;
    }

    let (dyn_start, dyn_end) = dyn_range.ok_or("ELF dinamico: no se encontro PT_DYNAMIC.")?;
    let mut strtab_virt = 0u64;
    let mut strtab_size = 0u64;
    let mut needed_offsets: Vec<u64> = Vec::new();
    let mut soname_offset: Option<u64> = None;
    let mut rpath_offset: Option<u64> = None;
    let mut runpath_offset: Option<u64> = None;

    let mut cursor = dyn_start;
    while cursor + 16 <= dyn_end {
        let tag = read_i64_le_at(raw, cursor).ok_or("ELF dinamico: DT tag invalido.")?;
        let val = read_u64_le_at(raw, cursor + 8).ok_or("ELF dinamico: DT value invalido.")?;
        cursor += 16;

        if tag == DT_NULL {
            break;
        }

        match tag {
            DT_NEEDED => needed_offsets.push(val),
            DT_STRTAB => strtab_virt = val,
            DT_STRSZ => strtab_size = val,
            DT_SONAME => soname_offset = Some(val),
            DT_RPATH => rpath_offset = Some(val),
            DT_RUNPATH => runpath_offset = Some(val),
            _ => {}
        }
    }

    if strtab_virt == 0 || strtab_size == 0 {
        return Err("ELF dinamico: DT_STRTAB/DT_STRSZ invalido.");
    }

    let mut needed = Vec::new();
    for off in needed_offsets.iter() {
        if let Some(name) = read_dynamic_string(raw, &report, strtab_virt, strtab_size, *off) {
            needed.push(name);
        }
    }

    let soname = soname_offset
        .and_then(|off| read_dynamic_string(raw, &report, strtab_virt, strtab_size, off));
    let rpath = rpath_offset
        .and_then(|off| read_dynamic_string(raw, &report, strtab_virt, strtab_size, off));
    let runpath = runpath_offset
        .and_then(|off| read_dynamic_string(raw, &report, strtab_virt, strtab_size, off));

    Ok(DynamicInspectReport {
        interp_path: report.interp_path,
        needed,
        soname,
        rpath,
        runpath,
        strtab_virt,
        strtab_size,
    })
}

pub fn phase2_dynamic_compatibility(
    report: &ElfInspectReport,
    dynamic: &DynamicInspectReport,
) -> Result<(), &'static str> {
    if report.machine != EM_X86_64 {
        return Err("fase2: requiere ELF x86_64.");
    }
    if report.e_type != ET_DYN {
        return Err("fase2: requiere ELF ET_DYN (PIE).");
    }
    if !report.has_dynamic {
        return Err("fase2: falta PT_DYNAMIC.");
    }
    if dynamic.interp_path.is_none() {
        return Err("fase2: falta PT_INTERP (loader dinamico).");
    }
    // PT_TLS is now supported — TLS block will be allocated at runtime

    let span_size = report
        .span_end
        .checked_sub(report.span_start)
        .ok_or("fase2: span invalido.")?;
    if span_size == 0 {
        return Err("fase2: span vacio.");
    }
    if span_size > ELF_MAX_STAGED_IMAGE_BYTES as u64 {
        return Err("fase2: imagen demasiado grande para staging.");
    }

    Ok(())
}

pub fn phase1_static_compatibility(report: &ElfInspectReport) -> Result<(), &'static str> {
    if report.machine != EM_X86_64 {
        return Err("requiere ELF x86_64.");
    }
    if report.e_type != ET_EXEC {
        if report.e_type == ET_DYN {
            return Err("ELF ET_DYN no soportado en fase1 (solo ET_EXEC estatico).");
        }
        return Err("tipo ELF no soportado para fase1.");
    }
    if report.has_interp {
        return Err("requiere PT_INTERP (loader dinamico).");
    }
    if report.has_dynamic {
        return Err("requiere PT_DYNAMIC (link dinamico).");
    }
    if report.has_tls {
        return Err("PT_TLS aun no soportado en fase1.");
    }
    if report.load_mem_bytes == 0 {
        return Err("sin memoria PT_LOAD util.");
    }

    let span_size = report
        .span_end
        .checked_sub(report.span_start)
        .ok_or("span invalido.")?;
    if span_size == 0 {
        return Err("span de carga vacio.");
    }
    if span_size > ELF_MAX_STAGED_IMAGE_BYTES as u64 {
        return Err("imagen ELF demasiado grande para staging fase1.");
    }

    if report.entry < report.span_start || report.entry >= report.span_end {
        return Err("entry point fuera del rango PT_LOAD.");
    }

    Ok(())
}

pub fn newlib_cpp_port_diagnosis(report: &ElfInspectReport) -> &'static str {
    if report.machine != EM_X86_64 {
        return "newlib-c++: requiere x86_64.";
    }
    if report.e_type != ET_EXEC {
        if report.e_type == ET_DYN {
            return "newlib-c++: recompila en modo no-PIE (ET_EXEC).";
        }
        return "newlib-c++: tipo ELF no soportado.";
    }
    if report.has_interp || report.has_dynamic {
        return "newlib-c++: recompila estatico (sin PT_INTERP/PT_DYNAMIC).";
    }
    if report.has_tls {
        return "newlib-c++: PT_TLS detectado; evita thread_local en fase1.";
    }
    if report.syscall_sites == 0 {
        return "newlib-c++: perfil estatico OK (sin syscall explicito detectado).";
    }
    "newlib-c++: perfil estatico compatible para porting fase1."
}

pub fn stage_static_elf64(raw: &[u8]) -> Result<Phase1StageReport, &'static str> {
    let report = inspect_elf64(raw)?;
    phase1_static_compatibility(&report)?;

    let span_size = report
        .span_end
        .checked_sub(report.span_start)
        .ok_or("span invalido.")?;
    let span_size_usize = u64_to_usize(span_size).ok_or("span fuera de rango.")?;
    if span_size_usize > ELF_MAX_STAGED_IMAGE_BYTES {
        return Err("staging excede limite de memoria fase1.");
    }

    let mut image = alloc::vec![0u8; span_size_usize];

    for seg in report.load_segments.iter() {
        if seg.file_size == 0 {
            continue;
        }
        let (src_start, src_end) =
            checked_range(raw.len(), seg.file_offset, seg.file_size).ok_or("PT_LOAD invalido.")?;
        let rel = seg
            .vaddr
            .checked_sub(report.span_start)
            .ok_or("PT_LOAD fuera de span.")?;
        let dst_start = u64_to_usize(rel).ok_or("offset PT_LOAD fuera de rango.")?;
        let copy_len = u64_to_usize(seg.file_size).ok_or("tamano PT_LOAD fuera de rango.")?;
        let dst_end = dst_start
            .checked_add(copy_len)
            .ok_or("overflow al copiar PT_LOAD.")?;
        if dst_end > image.len() {
            return Err("PT_LOAD excede imagen staged.");
        }
        image[dst_start..dst_end].copy_from_slice(&raw[src_start..src_end]);
    }

    let entry_offset = report
        .entry
        .checked_sub(report.span_start)
        .ok_or("entry fuera de span.")?;
    let entry_offset_usize = u64_to_usize(entry_offset).ok_or("entry offset fuera de rango.")?;
    if entry_offset_usize >= image.len() {
        return Err("entry offset fuera de imagen staged.");
    }

    let mut hash = 2166136261u32;

    Ok(Phase1StageReport {
        span_start: report.span_start,
        span_size,
        entry_virt: report.entry,
        entry_offset,
        load_segments: report.load_segments.len(),
        syscall_sites: report.syscall_sites,
        sample_hash: hash,
    })
}

pub fn stage_dyn_elf64(raw: &[u8], load_bias: u64) -> Result<Phase2DynamicStageReport, &'static str> {
    let report = inspect_elf64(raw)?;
    if report.machine != EM_X86_64 {
        return Err("staging fase2: requiere ELF x86_64.");
    }
    if report.e_type != ET_DYN {
        return Err("staging fase2: requiere ELF ET_DYN.");
    }
    // PT_TLS is now supported — TLS handled in stage_runtime_dyn_image
    if (load_bias & (PAGE_SIZE - 1)) != 0 {
        return Err("staging fase2: load_bias debe estar alineado a pagina.");
    }

    let mapped_start = report
        .span_start
        .checked_add(load_bias)
        .ok_or("staging fase2: overflow en span_start.")?;
    let mapped_end = report
        .span_end
        .checked_add(load_bias)
        .ok_or("staging fase2: overflow en span_end.")?;
    if mapped_end <= mapped_start {
        return Err("staging fase2: span vacio.");
    }
    let image_size = mapped_end
        .checked_sub(mapped_start)
        .ok_or("staging fase2: span invalido.")?;
    if image_size > ELF_MAX_STAGED_IMAGE_BYTES as u64 {
        return Err("staging fase2: imagen demasiado grande.");
    }

    let image_size_usize = u64_to_usize(image_size).ok_or("staging fase2: span fuera de rango.")?;
    let mut image = alloc::vec![0u8; image_size_usize];

    for seg in report.load_segments.iter() {
        if seg.file_size == 0 {
            continue;
        }

        let (src_start, src_end) =
            checked_range(raw.len(), seg.file_offset, seg.file_size).ok_or("PT_LOAD invalido.")?;
        let mapped_vaddr = seg
            .vaddr
            .checked_add(load_bias)
            .ok_or("staging fase2: overflow en PT_LOAD vaddr.")?;
        let rel = mapped_vaddr
            .checked_sub(mapped_start)
            .ok_or("staging fase2: PT_LOAD fuera de span.")?;
        let dst_start = u64_to_usize(rel).ok_or("staging fase2: offset PT_LOAD invalido.")?;
        let copy_len = u64_to_usize(seg.file_size).ok_or("staging fase2: PT_LOAD size invalido.")?;
        let dst_end = dst_start
            .checked_add(copy_len)
            .ok_or("staging fase2: overflow al copiar PT_LOAD.")?;
        if dst_end > image.len() {
            return Err("staging fase2: PT_LOAD excede imagen.");
        }
        image[dst_start..dst_end].copy_from_slice(&raw[src_start..src_end]);
    }

    let entry_virt = report
        .entry
        .checked_add(load_bias)
        .ok_or("staging fase2: overflow en entry.")?;
    let entry_offset = entry_virt
        .checked_sub(mapped_start)
        .ok_or("staging fase2: entry fuera de span.")?;
    let entry_offset_usize =
        u64_to_usize(entry_offset).ok_or("staging fase2: entry offset fuera de rango.")?;
    if entry_offset_usize >= image.len() {
        return Err("staging fase2: entry fuera de imagen.");
    }

    let sample_len = core::cmp::min(256usize, image.len().saturating_sub(entry_offset_usize));
    let mut hash = 2166136261u32;
    let mut i = 0usize;
    while i < sample_len {
        hash ^= image[entry_offset_usize + i] as u32;
        hash = hash.wrapping_mul(16777619);
        i += 1;
    }

    Ok(Phase2DynamicStageReport {
        load_bias,
        image_start: mapped_start,
        image_size,
        entry_virt,
        entry_offset,
        load_segments: report.load_segments.len(),
        sample_hash: hash,
    })
}

#[derive(Clone, Copy)]
struct RuntimeRelocDynamicInfo {
    strtab_addr: u64,
    strtab_size: u64,
    hash_addr: u64,
    symtab_addr: u64,
    syment_size: u64,
    rela_addr: u64,
    rela_size: u64,
    rela_ent: u64,
    rel_addr: u64,
    rel_size: u64,
    rel_ent: u64,
    jmprel_addr: u64,
    jmprel_size: u64,
    pltrel_kind: u64,
}

impl RuntimeRelocDynamicInfo {
    const fn empty() -> Self {
        Self {
            strtab_addr: 0,
            strtab_size: 0,
            hash_addr: 0,
            symtab_addr: 0,
            syment_size: 24,
            rela_addr: 0,
            rela_size: 0,
            rela_ent: 24,
            rel_addr: 0,
            rel_size: 0,
            rel_ent: 16,
            jmprel_addr: 0,
            jmprel_size: 0,
            pltrel_kind: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct RuntimeRelocStats {
    total: u32,
    applied: u32,
    unsupported: u32,
    errors: u32,
}

impl RuntimeRelocStats {
    const fn empty() -> Self {
        Self {
            total: 0,
            applied: 0,
            unsupported: 0,
            errors: 0,
        }
    }
}

fn runtime_reloc_image_offset(report: &ElfInspectReport, reloc_offset: u64) -> Option<usize> {
    let rel = reloc_offset.checked_sub(report.span_start)?;
    u64_to_usize(rel)
}

fn runtime_read_u64_from_image(image: &[u8], off: usize) -> Option<u64> {
    let end = off.checked_add(8)?;
    if end > image.len() {
        return None;
    }
    Some(u64::from_le_bytes([
        image[off],
        image[off + 1],
        image[off + 2],
        image[off + 3],
        image[off + 4],
        image[off + 5],
        image[off + 6],
        image[off + 7],
    ]))
}

fn runtime_write_u64_to_image(image: &mut [u8], off: usize, value: u64) -> bool {
    let end = match off.checked_add(8) {
        Some(v) => v,
        None => return false,
    };
    if end > image.len() {
        return false;
    }
    image[off..end].copy_from_slice(&value.to_le_bytes());
    true
}

#[derive(Clone, Copy)]
struct RuntimeDynSymbol {
    name_off: u32,
    info: u8,
    shndx: u16,
    value: u64,
    size: u64,
}

fn runtime_read_dynsym_entry(
    raw: &[u8],
    report: &ElfInspectReport,
    symtab_addr: u64,
    syment_size: u64,
    sym_index: u32,
) -> Option<RuntimeDynSymbol> {
    if symtab_addr == 0 || syment_size < 24 {
        return None;
    }
    let sym_off_vaddr = symtab_addr.checked_add((sym_index as u64).checked_mul(syment_size)?)?;
    let sym_file_off = vaddr_to_file_offset(report.load_segments.as_slice(), sym_off_vaddr)?;
    let (sym_start, _) = checked_range(raw.len(), sym_file_off, syment_size)?;
    let name_off = read_u32_le_at(raw, sym_start)?;
    let info = *raw.get(sym_start + 4)?;
    let shndx = read_u16_le_at(raw, sym_start + 6)?;
    let value = read_u64_le_at(raw, sym_start + 8)?;
    let size = read_u64_le_at(raw, sym_start + 16)?;
    Some(RuntimeDynSymbol {
        name_off,
        info,
        shndx,
        value,
        size,
    })
}

fn runtime_resolve_dyn_symbol_value(load_bias: u64, sym: RuntimeDynSymbol) -> Option<u64> {
    if sym.shndx == 0 {
        return None;
    }
    load_bias.checked_add(sym.value)
}

fn runtime_dyn_symbol_name(
    raw: &[u8],
    report: &ElfInspectReport,
    strtab_addr: u64,
    strtab_size: u64,
    sym: RuntimeDynSymbol,
) -> Option<String> {
    if sym.name_off == 0 {
        return None;
    }
    read_dynamic_string(raw, report, strtab_addr, strtab_size, sym.name_off as u64)
}

fn runtime_symbol_binding(info: u8) -> u8 {
    (info >> 4) & 0x0F
}

fn runtime_dynsym_count_from_hash(
    raw: &[u8],
    report: &ElfInspectReport,
    hash_addr: u64,
) -> Option<usize> {
    if hash_addr == 0 {
        return None;
    }
    let hash_off = vaddr_to_file_offset(report.load_segments.as_slice(), hash_addr)?;
    let hash_start = u64_to_usize(hash_off)?;
    let nchain = read_u32_le_at(raw, hash_start + 4)? as usize;
    if nchain == 0 || nchain > 1_000_000 {
        return None;
    }
    Some(nchain)
}

fn runtime_guess_dynsym_count(
    raw: &[u8],
    report: &ElfInspectReport,
    dyn_info: RuntimeRelocDynamicInfo,
) -> usize {
    if let Some(count) = runtime_dynsym_count_from_hash(raw, report, dyn_info.hash_addr) {
        return count;
    }

    if dyn_info.symtab_addr != 0
        && dyn_info.syment_size >= 24
        && dyn_info.strtab_addr > dyn_info.symtab_addr
    {
        let bytes = dyn_info.strtab_addr - dyn_info.symtab_addr;
        let count = (bytes / dyn_info.syment_size) as usize;
        if count > 0 && count <= 262_144 {
            return count;
        }
    }

    4096
}

const LINUX_SYMBOL_TRACE_MAX: usize = 4096;

struct RuntimeGlobalSymbol {
    name: String,
    value: u64,
    provider: String,
}

fn runtime_reloc_kind_name(r_type: u32) -> &'static str {
    match r_type {
        R_X86_64_64 => "R_X86_64_64",
        R_X86_64_COPY => "R_X86_64_COPY",
        R_X86_64_GLOB_DAT => "R_X86_64_GLOB_DAT",
        R_X86_64_JUMP_SLOT => "R_X86_64_JUMP_SLOT",
        R_X86_64_RELATIVE => "R_X86_64_RELATIVE",
        _ => "R_UNKNOWN",
    }
}

fn runtime_copy_from_ptr_to_image(image: &mut [u8], off: usize, src_addr: u64, len: usize) -> bool {
    if src_addr == 0 {
        return false;
    }
    let end = match off.checked_add(len) {
        Some(v) => v,
        None => return false,
    };
    if end > image.len() {
        return false;
    }
    unsafe {
        ptr::copy_nonoverlapping(src_addr as *const u8, image.as_mut_ptr().add(off), len);
    }
    true
}

fn runtime_push_symbol_trace(
    traces: &mut Vec<LinuxDynSymbolTrace>,
    requestor: &str,
    symbol: &str,
    provider: &str,
    reloc_kind: &str,
    slot_addr: u64,
    value_addr: u64,
) {
    if traces.len() >= LINUX_SYMBOL_TRACE_MAX {
        return;
    }
    traces.push(LinuxDynSymbolTrace {
        requestor: String::from(requestor),
        symbol: String::from(symbol),
        provider: String::from(provider),
        reloc_kind: String::from(reloc_kind),
        slot_addr,
        value_addr,
    });
}

fn runtime_collect_image_exports(
    raw: &[u8],
    image: &RuntimeDynImage,
    provider_label: &str,
    global: &mut Vec<RuntimeGlobalSymbol>,
) {
    let dyn_info = image.dyn_info;
    if dyn_info.symtab_addr == 0
        || dyn_info.syment_size < 24
        || dyn_info.strtab_addr == 0
        || dyn_info.strtab_size == 0
    {
        return;
    }

    let max_count = runtime_guess_dynsym_count(raw, &image.report, dyn_info);
    let mut index = 1usize;
    while index < max_count {
        let Some(sym) = runtime_read_dynsym_entry(
            raw,
            &image.report,
            dyn_info.symtab_addr,
            dyn_info.syment_size,
            index as u32,
        ) else {
            break;
        };

        if sym.shndx == 0 {
            index += 1;
            continue;
        }
        let bind = runtime_symbol_binding(sym.info);
        if bind == 0 {
            index += 1;
            continue;
        }
        let Some(name) = runtime_dyn_symbol_name(
            raw,
            &image.report,
            dyn_info.strtab_addr,
            dyn_info.strtab_size,
            sym,
        ) else {
            index += 1;
            continue;
        };
        if name.is_empty() {
            index += 1;
            continue;
        }
        let Some(value) = runtime_resolve_dyn_symbol_value(image.load_bias, sym) else {
            index += 1;
            continue;
        };
        let exists = global.iter().any(|entry| entry.name == name);
        if !exists {
            global.push(RuntimeGlobalSymbol {
                name,
                value,
                provider: String::from(provider_label),
            });
        }
        index += 1;
    }
}

fn runtime_lookup_global_symbol<'a>(
    global: &'a [RuntimeGlobalSymbol],
    name: &str,
) -> Option<&'a RuntimeGlobalSymbol> {
    for entry in global.iter() {
        if entry.name == name {
            return Some(entry);
        }
    }
    None
}

fn runtime_apply_relative_rela(
    image: &mut [u8],
    raw: &[u8],
    report: &ElfInspectReport,
    load_bias: u64,
    symtab_addr: u64,
    syment_size: u64,
    rela_addr: u64,
    rela_size: u64,
    rela_ent: u64,
    stats: &mut RuntimeRelocStats,
) {
    if rela_addr == 0 || rela_size == 0 {
        return;
    }
    if rela_ent < 24 {
        stats.errors = stats.errors.saturating_add(1);
        return;
    }
    let Some(rela_file_off) = vaddr_to_file_offset(report.load_segments.as_slice(), rela_addr) else {
        stats.errors = stats.errors.saturating_add(1);
        return;
    };
    let Some((rela_start, rela_end)) = checked_range(raw.len(), rela_file_off, rela_size) else {
        stats.errors = stats.errors.saturating_add(1);
        return;
    };
    let entry_size = rela_ent as usize;
    let mut cursor = rela_start;
    while cursor + entry_size <= rela_end {
        let r_offset = match read_u64_le_at(raw, cursor) {
            Some(v) => v,
            None => {
                stats.errors = stats.errors.saturating_add(1);
                break;
            }
        };
        let r_info = match read_u64_le_at(raw, cursor + 8) {
            Some(v) => v,
            None => {
                stats.errors = stats.errors.saturating_add(1);
                break;
            }
        };
        let addend = match read_i64_le_at(raw, cursor + 16) {
            Some(v) => v,
            None => {
                stats.errors = stats.errors.saturating_add(1);
                break;
            }
        };
        stats.total = stats.total.saturating_add(1);
        let r_type = (r_info & 0xFFFF_FFFF) as u32;
        let Some(dst_off) = runtime_reloc_image_offset(report, r_offset) else {
            stats.errors = stats.errors.saturating_add(1);
            cursor += entry_size;
            continue;
        };
        let sym_index = (r_info >> 32) as u32;
        let value_opt = match r_type {
            R_X86_64_RELATIVE => {
                let value_i = (load_bias as i128).saturating_add(addend as i128);
                if value_i < 0 || value_i > u64::MAX as i128 {
                    None
                } else {
                    Some(value_i as u64)
                }
            }
            R_X86_64_COPY => {
                cursor += entry_size;
                continue;
            }
            R_X86_64_64 | R_X86_64_GLOB_DAT | R_X86_64_JUMP_SLOT => {
                let base_opt = runtime_read_dynsym_entry(
                    raw,
                    report,
                    symtab_addr,
                    syment_size,
                    sym_index,
                )
                .and_then(|sym| runtime_resolve_dyn_symbol_value(load_bias, sym));
                if let Some(base) = base_opt {
                    let value_i = (base as i128).saturating_add(addend as i128);
                    if value_i < 0 || value_i > u64::MAX as i128 {
                        None
                    } else {
                        Some(value_i as u64)
                    }
                } else {
                    None
                }
            }
            _ => {
                stats.unsupported = stats.unsupported.saturating_add(1);
                cursor += entry_size;
                continue;
            }
        };

        let Some(value) = value_opt else {
            if r_type == R_X86_64_64 || r_type == R_X86_64_GLOB_DAT || r_type == R_X86_64_JUMP_SLOT
            {
                stats.unsupported = stats.unsupported.saturating_add(1);
            } else {
                stats.errors = stats.errors.saturating_add(1);
            }
            cursor += entry_size;
            continue;
        };

        if runtime_write_u64_to_image(image, dst_off, value) {
            stats.applied = stats.applied.saturating_add(1);
        } else {
            stats.errors = stats.errors.saturating_add(1);
        }
        cursor += entry_size;
    }
}

fn runtime_apply_relative_rel(
    image: &mut [u8],
    raw: &[u8],
    report: &ElfInspectReport,
    load_bias: u64,
    symtab_addr: u64,
    syment_size: u64,
    rel_addr: u64,
    rel_size: u64,
    rel_ent: u64,
    stats: &mut RuntimeRelocStats,
) {
    if rel_addr == 0 || rel_size == 0 {
        return;
    }
    if rel_ent < 16 {
        stats.errors = stats.errors.saturating_add(1);
        return;
    }
    let Some(rel_file_off) = vaddr_to_file_offset(report.load_segments.as_slice(), rel_addr) else {
        stats.errors = stats.errors.saturating_add(1);
        return;
    };
    let Some((rel_start, rel_end)) = checked_range(raw.len(), rel_file_off, rel_size) else {
        stats.errors = stats.errors.saturating_add(1);
        return;
    };
    let entry_size = rel_ent as usize;
    let mut cursor = rel_start;
    while cursor + entry_size <= rel_end {
        let r_offset = match read_u64_le_at(raw, cursor) {
            Some(v) => v,
            None => {
                stats.errors = stats.errors.saturating_add(1);
                break;
            }
        };
        let r_info = match read_u64_le_at(raw, cursor + 8) {
            Some(v) => v,
            None => {
                stats.errors = stats.errors.saturating_add(1);
                break;
            }
        };
        stats.total = stats.total.saturating_add(1);
        let r_type = (r_info & 0xFFFF_FFFF) as u32;
        let Some(dst_off) = runtime_reloc_image_offset(report, r_offset) else {
            stats.errors = stats.errors.saturating_add(1);
            cursor += entry_size;
            continue;
        };
        let Some(implicit_addend) = runtime_read_u64_from_image(image, dst_off) else {
            stats.errors = stats.errors.saturating_add(1);
            cursor += entry_size;
            continue;
        };
        let sym_index = (r_info >> 32) as u32;
        let value = match r_type {
            R_X86_64_RELATIVE => load_bias.saturating_add(implicit_addend),
            R_X86_64_COPY => {
                cursor += entry_size;
                continue;
            }
            R_X86_64_64 | R_X86_64_GLOB_DAT | R_X86_64_JUMP_SLOT => {
                let Some(sym) =
                    runtime_read_dynsym_entry(raw, report, symtab_addr, syment_size, sym_index)
                else {
                    stats.unsupported = stats.unsupported.saturating_add(1);
                    cursor += entry_size;
                    continue;
                };
                let Some(base) = runtime_resolve_dyn_symbol_value(load_bias, sym) else {
                    stats.unsupported = stats.unsupported.saturating_add(1);
                    cursor += entry_size;
                    continue;
                };
                base.saturating_add(implicit_addend)
            }
            _ => {
                stats.unsupported = stats.unsupported.saturating_add(1);
                cursor += entry_size;
                continue;
            }
        };
        if runtime_write_u64_to_image(image, dst_off, value) {
            stats.applied = stats.applied.saturating_add(1);
        } else {
            stats.errors = stats.errors.saturating_add(1);
        }
        cursor += entry_size;
    }
}

fn runtime_apply_global_rela_for_image(
    image: &mut RuntimeDynImage,
    raw: &[u8],
    global_symbols: &[RuntimeGlobalSymbol],
    requestor_label: &str,
    traces: &mut Vec<LinuxDynSymbolTrace>,
    rela_addr: u64,
    rela_size: u64,
    rela_ent: u64,
    stats: &mut RuntimeRelocStats,
) {
    if rela_addr == 0 || rela_size == 0 {
        return;
    }
    if rela_ent < 24 {
        stats.errors = stats.errors.saturating_add(1);
        return;
    }
    let Some(rela_file_off) = vaddr_to_file_offset(image.report.load_segments.as_slice(), rela_addr) else {
        stats.errors = stats.errors.saturating_add(1);
        return;
    };
    let Some((rela_start, rela_end)) = checked_range(raw.len(), rela_file_off, rela_size) else {
        stats.errors = stats.errors.saturating_add(1);
        return;
    };

    let entry_size = rela_ent as usize;
    let mut cursor = rela_start;
    while cursor + entry_size <= rela_end {
        let Some(r_offset) = read_u64_le_at(raw, cursor) else {
            stats.errors = stats.errors.saturating_add(1);
            break;
        };
        let Some(r_info) = read_u64_le_at(raw, cursor + 8) else {
            stats.errors = stats.errors.saturating_add(1);
            break;
        };
        let Some(addend) = read_i64_le_at(raw, cursor + 16) else {
            stats.errors = stats.errors.saturating_add(1);
            break;
        };

        let r_type = (r_info & 0xFFFF_FFFF) as u32;
        if r_type != R_X86_64_64
            && r_type != R_X86_64_COPY
            && r_type != R_X86_64_GLOB_DAT
            && r_type != R_X86_64_JUMP_SLOT
        {
            cursor += entry_size;
            continue;
        }
        let sym_index = (r_info >> 32) as u32;

        let Some(dst_off) = runtime_reloc_image_offset(&image.report, r_offset) else {
            stats.errors = stats.errors.saturating_add(1);
            cursor += entry_size;
            continue;
        };
        let Some(sym) = runtime_read_dynsym_entry(
            raw,
            &image.report,
            image.dyn_info.symtab_addr,
            image.dyn_info.syment_size,
            sym_index,
        ) else {
            stats.errors = stats.errors.saturating_add(1);
            cursor += entry_size;
            continue;
        };
        if sym.shndx != 0 && r_type != R_X86_64_COPY {
            cursor += entry_size;
            continue;
        }
        stats.total = stats.total.saturating_add(1);
        let Some(name) = runtime_dyn_symbol_name(
            raw,
            &image.report,
            image.dyn_info.strtab_addr,
            image.dyn_info.strtab_size,
            sym,
        ) else {
            stats.unsupported = stats.unsupported.saturating_add(1);
            cursor += entry_size;
            continue;
        };
        let Some(global_hit) = runtime_lookup_global_symbol(global_symbols, name.as_str()) else {
            stats.unsupported = stats.unsupported.saturating_add(1);
            cursor += entry_size;
            continue;
        };
        let base = global_hit.value;

        if r_type == R_X86_64_COPY {
            let copy_len = sym.size as usize;
            if copy_len == 0 {
                stats.applied = stats.applied.saturating_add(1);
                cursor += entry_size;
                continue;
            }
            if runtime_copy_from_ptr_to_image(image.image.as_mut_slice(), dst_off, base, copy_len) {
                stats.applied = stats.applied.saturating_add(1);
                runtime_push_symbol_trace(
                    traces,
                    requestor_label,
                    name.as_str(),
                    global_hit.provider.as_str(),
                    runtime_reloc_kind_name(r_type),
                    r_offset,
                    base,
                );
            } else {
                stats.errors = stats.errors.saturating_add(1);
            }
            cursor += entry_size;
            continue;
        }

        let value_i = (base as i128).saturating_add(addend as i128);
        if value_i < 0 || value_i > u64::MAX as i128 {
            stats.errors = stats.errors.saturating_add(1);
            cursor += entry_size;
            continue;
        }
        if runtime_write_u64_to_image(image.image.as_mut_slice(), dst_off, value_i as u64) {
            stats.applied = stats.applied.saturating_add(1);
            runtime_push_symbol_trace(
                traces,
                requestor_label,
                name.as_str(),
                global_hit.provider.as_str(),
                runtime_reloc_kind_name(r_type),
                r_offset,
                value_i as u64,
            );
        } else {
            stats.errors = stats.errors.saturating_add(1);
        }
        cursor += entry_size;
    }
}

fn runtime_apply_global_rel_for_image(
    image: &mut RuntimeDynImage,
    raw: &[u8],
    global_symbols: &[RuntimeGlobalSymbol],
    requestor_label: &str,
    traces: &mut Vec<LinuxDynSymbolTrace>,
    rel_addr: u64,
    rel_size: u64,
    rel_ent: u64,
    stats: &mut RuntimeRelocStats,
) {
    if rel_addr == 0 || rel_size == 0 {
        return;
    }
    if rel_ent < 16 {
        stats.errors = stats.errors.saturating_add(1);
        return;
    }
    let Some(rel_file_off) = vaddr_to_file_offset(image.report.load_segments.as_slice(), rel_addr) else {
        stats.errors = stats.errors.saturating_add(1);
        return;
    };
    let Some((rel_start, rel_end)) = checked_range(raw.len(), rel_file_off, rel_size) else {
        stats.errors = stats.errors.saturating_add(1);
        return;
    };

    let entry_size = rel_ent as usize;
    let mut cursor = rel_start;
    while cursor + entry_size <= rel_end {
        let Some(r_offset) = read_u64_le_at(raw, cursor) else {
            stats.errors = stats.errors.saturating_add(1);
            break;
        };
        let Some(r_info) = read_u64_le_at(raw, cursor + 8) else {
            stats.errors = stats.errors.saturating_add(1);
            break;
        };

        let r_type = (r_info & 0xFFFF_FFFF) as u32;
        if r_type != R_X86_64_64
            && r_type != R_X86_64_COPY
            && r_type != R_X86_64_GLOB_DAT
            && r_type != R_X86_64_JUMP_SLOT
        {
            cursor += entry_size;
            continue;
        }
        let sym_index = (r_info >> 32) as u32;

        let Some(dst_off) = runtime_reloc_image_offset(&image.report, r_offset) else {
            stats.errors = stats.errors.saturating_add(1);
            cursor += entry_size;
            continue;
        };
        let Some(implicit_addend) = runtime_read_u64_from_image(image.image.as_slice(), dst_off) else {
            stats.errors = stats.errors.saturating_add(1);
            cursor += entry_size;
            continue;
        };
        let Some(sym) = runtime_read_dynsym_entry(
            raw,
            &image.report,
            image.dyn_info.symtab_addr,
            image.dyn_info.syment_size,
            sym_index,
        ) else {
            stats.errors = stats.errors.saturating_add(1);
            cursor += entry_size;
            continue;
        };
        if sym.shndx != 0 && r_type != R_X86_64_COPY {
            cursor += entry_size;
            continue;
        }
        stats.total = stats.total.saturating_add(1);
        let Some(name) = runtime_dyn_symbol_name(
            raw,
            &image.report,
            image.dyn_info.strtab_addr,
            image.dyn_info.strtab_size,
            sym,
        ) else {
            stats.unsupported = stats.unsupported.saturating_add(1);
            cursor += entry_size;
            continue;
        };
        let Some(global_hit) = runtime_lookup_global_symbol(global_symbols, name.as_str()) else {
            stats.unsupported = stats.unsupported.saturating_add(1);
            cursor += entry_size;
            continue;
        };
        let base = global_hit.value;

        if r_type == R_X86_64_COPY {
            let copy_len = sym.size as usize;
            if copy_len == 0 {
                stats.applied = stats.applied.saturating_add(1);
                cursor += entry_size;
                continue;
            }
            if runtime_copy_from_ptr_to_image(image.image.as_mut_slice(), dst_off, base, copy_len) {
                stats.applied = stats.applied.saturating_add(1);
                runtime_push_symbol_trace(
                    traces,
                    requestor_label,
                    name.as_str(),
                    global_hit.provider.as_str(),
                    runtime_reloc_kind_name(r_type),
                    r_offset,
                    base,
                );
            } else {
                stats.errors = stats.errors.saturating_add(1);
            }
            cursor += entry_size;
            continue;
        }
        let value = base.saturating_add(implicit_addend);
        if runtime_write_u64_to_image(image.image.as_mut_slice(), dst_off, value) {
            stats.applied = stats.applied.saturating_add(1);
            runtime_push_symbol_trace(
                traces,
                requestor_label,
                name.as_str(),
                global_hit.provider.as_str(),
                runtime_reloc_kind_name(r_type),
                r_offset,
                value,
            );
        } else {
            stats.errors = stats.errors.saturating_add(1);
        }
        cursor += entry_size;
    }
}

fn runtime_apply_global_symbol_relocations(
    image: &mut RuntimeDynImage,
    raw: &[u8],
    global_symbols: &[RuntimeGlobalSymbol],
    requestor_label: &str,
    traces: &mut Vec<LinuxDynSymbolTrace>,
) -> RuntimeRelocStats {
    let mut stats = RuntimeRelocStats::empty();
    let dyn_info = image.dyn_info;
    if dyn_info.symtab_addr == 0 || dyn_info.syment_size < 24 {
        return stats;
    }

    runtime_apply_global_rela_for_image(
        image,
        raw,
        global_symbols,
        requestor_label,
        traces,
        dyn_info.rela_addr,
        dyn_info.rela_size,
        dyn_info.rela_ent,
        &mut stats,
    );
    runtime_apply_global_rel_for_image(
        image,
        raw,
        global_symbols,
        requestor_label,
        traces,
        dyn_info.rel_addr,
        dyn_info.rel_size,
        dyn_info.rel_ent,
        &mut stats,
    );
    if dyn_info.jmprel_addr != 0 && dyn_info.jmprel_size != 0 {
        if dyn_info.pltrel_kind == DT_PLTREL_RELA {
            runtime_apply_global_rela_for_image(
                image,
                raw,
                global_symbols,
                requestor_label,
                traces,
                dyn_info.jmprel_addr,
                dyn_info.jmprel_size,
                dyn_info.rela_ent,
                &mut stats,
            );
        } else if dyn_info.pltrel_kind == DT_PLTREL_REL {
            runtime_apply_global_rel_for_image(
                image,
                raw,
                global_symbols,
                requestor_label,
                traces,
                dyn_info.jmprel_addr,
                dyn_info.jmprel_size,
                dyn_info.rel_ent,
                &mut stats,
            );
        }
    }
    stats
}

fn stage_runtime_dyn_image(raw: &[u8]) -> Result<RuntimeDynImage, &'static str> {
    let report = inspect_elf64(raw)?;
    if report.machine != EM_X86_64 {
        return Err("runtime phase2: requiere ELF x86_64.");
    }
    if report.e_type != ET_DYN {
        return Err("runtime phase2: requiere ELF ET_DYN.");
    }
    if report.has_tls {
        // PT_TLS is supported — block will be allocated below
    }

    let span_size = report
        .span_end
        .checked_sub(report.span_start)
        .ok_or("runtime phase2: span invalido.")?;
    if span_size == 0 {
        return Err("runtime phase2: span vacio.");
    }
    if span_size > ELF_MAX_STAGED_IMAGE_BYTES as u64 {
        return Err("runtime phase2: imagen demasiado grande.");
    }

    let image_size = u64_to_usize(span_size).ok_or("runtime phase2: span fuera de rango.")?;
    let mut image = alloc::vec![0u8; image_size];

    for seg in report.load_segments.iter() {
        if seg.file_size == 0 {
            continue;
        }
        let (src_start, src_end) =
            checked_range(raw.len(), seg.file_offset, seg.file_size).ok_or("PT_LOAD invalido.")?;
        let rel = seg
            .vaddr
            .checked_sub(report.span_start)
            .ok_or("runtime phase2: PT_LOAD fuera de span.")?;
        let dst_start = u64_to_usize(rel).ok_or("runtime phase2: offset PT_LOAD invalido.")?;
        let copy_len = u64_to_usize(seg.file_size).ok_or("runtime phase2: PT_LOAD size invalido.")?;
        let dst_end = dst_start
            .checked_add(copy_len)
            .ok_or("runtime phase2: overflow al copiar PT_LOAD.")?;
        if dst_end > image.len() {
            return Err("runtime phase2: PT_LOAD excede imagen.");
        }
        image[dst_start..dst_end].copy_from_slice(&raw[src_start..src_end]);
    }

    let image_start = image.as_ptr() as usize as u64;
    let load_bias = image_start
        .checked_sub(report.span_start)
        .ok_or("runtime phase2: base invalida para load bias.")?;
    let entry_virt = report
        .entry
        .checked_add(load_bias)
        .ok_or("runtime phase2: overflow en entry.")?;
    let entry_offset = report
        .entry
        .checked_sub(report.span_start)
        .ok_or("runtime phase2: entry fuera de span.")?;
    let entry_offset_usize =
        u64_to_usize(entry_offset).ok_or("runtime phase2: entry fuera de rango.")?;
    if entry_offset_usize >= image.len() {
        return Err("runtime phase2: entry fuera de imagen.");
    }

    let sample_len = core::cmp::min(256usize, image.len().saturating_sub(entry_offset_usize));
    let mut hash = 2166136261u32;
    let mut i = 0usize;
    while i < sample_len {
        hash ^= image[entry_offset_usize + i] as u32;
        hash = hash.wrapping_mul(16777619);
        i += 1;
    }

    let phoff = read_u64_le_at(raw, 32).ok_or("runtime phase2: e_phoff invalido.")?;
    let phent = read_u16_le_at(raw, 54).ok_or("runtime phase2: e_phentsize invalido.")?;
    let phnum = read_u16_le_at(raw, 56).ok_or("runtime phase2: e_phnum invalido.")?;
    if phnum == 0 || phent < 56 {
        return Err("runtime phase2: tabla PH invalida.");
    }

    let phdr_len = (phent as usize)
        .checked_mul(phnum as usize)
        .ok_or("runtime phase2: overflow en PH.")?;
    let (phdr_start, phdr_end) =
        checked_range(raw.len(), phoff, phdr_len as u64).ok_or("runtime phase2: PH fuera de rango.")?;
    let phdr_blob = raw[phdr_start..phdr_end].to_vec();

    let mut phdr_addr = 0u64;
    let phoff_end = phoff
        .checked_add(phdr_len as u64)
        .ok_or("runtime phase2: overflow en PH range.")?;
    for seg in report.load_segments.iter() {
        if seg.file_size == 0 {
            continue;
        }
        let seg_file_end = seg
            .file_offset
            .checked_add(seg.file_size)
            .ok_or("runtime phase2: overflow en segmento.")?;
        if phoff >= seg.file_offset && phoff_end <= seg_file_end {
            let rel = phoff
                .checked_sub(seg.file_offset)
                .ok_or("runtime phase2: PH fuera de segmento.")?;
            phdr_addr = load_bias
                .checked_add(seg.vaddr)
                .and_then(|v| v.checked_add(rel))
                .ok_or("runtime phase2: overflow en PH runtime.")?;
            break;
        }
    }
    let phdr_addr = if phdr_addr == 0 {
        phdr_blob.as_ptr() as usize as u64
    } else {
        phdr_addr
    };

    let mut dyn_range: Option<(usize, usize)> = None;
    let mut dyn_info = RuntimeRelocDynamicInfo::empty();
    let mut i = 0usize;
    while i < phnum as usize {
        let off = phdr_start + i * phent as usize;
        let p_type = read_u32_le_at(raw, off).ok_or("runtime phase2: p_type invalido.")?;
        if p_type == PT_DYNAMIC {
            let p_offset = read_u64_le_at(raw, off + 8).ok_or("runtime phase2: p_offset dinamico invalido.")?;
            let p_filesz = read_u64_le_at(raw, off + 32).ok_or("runtime phase2: p_filesz dinamico invalido.")?;
            if p_filesz > 0 {
                dyn_range = Some(
                    checked_range(raw.len(), p_offset, p_filesz)
                        .ok_or("runtime phase2: PT_DYNAMIC fuera de rango.")?,
                );
            }
            break;
        }
        i += 1;
    }
    if let Some((dyn_start, dyn_end)) = dyn_range {
        let mut cursor = dyn_start;
        while cursor + 16 <= dyn_end {
            let tag = read_i64_le_at(raw, cursor).ok_or("runtime phase2: DT tag invalido.")?;
            let val = read_u64_le_at(raw, cursor + 8).ok_or("runtime phase2: DT value invalido.")?;
            cursor += 16;
            if tag == DT_NULL {
                break;
            }
            match tag {
                DT_HASH => dyn_info.hash_addr = val,
                DT_STRTAB => dyn_info.strtab_addr = val,
                DT_STRSZ => dyn_info.strtab_size = val,
                DT_SYMTAB => dyn_info.symtab_addr = val,
                DT_SYMENT => dyn_info.syment_size = if val == 0 { 24 } else { val },
                DT_RELA => dyn_info.rela_addr = val,
                DT_RELASZ => dyn_info.rela_size = val,
                DT_RELAENT => dyn_info.rela_ent = if val == 0 { 24 } else { val },
                DT_REL => dyn_info.rel_addr = val,
                DT_RELSZ => dyn_info.rel_size = val,
                DT_RELENT => dyn_info.rel_ent = if val == 0 { 16 } else { val },
                DT_JMPREL => dyn_info.jmprel_addr = val,
                DT_PLTRELSZ => dyn_info.jmprel_size = val,
                DT_PLTREL => dyn_info.pltrel_kind = val,
                _ => {}
            }
        }
    }

    let mut reloc_stats = RuntimeRelocStats::empty();
    runtime_apply_relative_rela(
        image.as_mut_slice(),
        raw,
        &report,
        load_bias,
        dyn_info.symtab_addr,
        dyn_info.syment_size,
        dyn_info.rela_addr,
        dyn_info.rela_size,
        dyn_info.rela_ent,
        &mut reloc_stats,
    );
    runtime_apply_relative_rel(
        image.as_mut_slice(),
        raw,
        &report,
        load_bias,
        dyn_info.symtab_addr,
        dyn_info.syment_size,
        dyn_info.rel_addr,
        dyn_info.rel_size,
        dyn_info.rel_ent,
        &mut reloc_stats,
    );
    if dyn_info.jmprel_addr != 0 && dyn_info.jmprel_size != 0 {
        if dyn_info.pltrel_kind == DT_PLTREL_RELA {
            runtime_apply_relative_rela(
                image.as_mut_slice(),
                raw,
                &report,
                load_bias,
                dyn_info.symtab_addr,
                dyn_info.syment_size,
                dyn_info.jmprel_addr,
                dyn_info.jmprel_size,
                dyn_info.rela_ent,
                &mut reloc_stats,
            );
        } else if dyn_info.pltrel_kind == DT_PLTREL_REL {
            runtime_apply_relative_rel(
                image.as_mut_slice(),
                raw,
                &report,
                load_bias,
                dyn_info.symtab_addr,
                dyn_info.syment_size,
                dyn_info.jmprel_addr,
                dyn_info.jmprel_size,
                dyn_info.rel_ent,
                &mut reloc_stats,
            );
        }
    }

    let sample_len = core::cmp::min(256usize, image.len().saturating_sub(entry_offset_usize));
    let mut hash_idx = 0usize;
    while hash_idx < sample_len {
        hash ^= image[entry_offset_usize + hash_idx] as u32;
        hash = hash.wrapping_mul(16777619);
        hash_idx += 1;
    }

    // --- TLS block allocation (Variant II, glibc x86_64 layout) ---
    // Layout: [TLS init data (tls_memsz aligned)] [TCB self-pointer (8 bytes)]
    //          <-- negative offsets               ^--- FS:0 points here
    let (tls_block, tls_tcb_addr) = if report.has_tls && report.tls_memsz > 0 {
        let tls_align = if report.tls_align == 0 { 8u64 } else { report.tls_align.max(8) };
        let aligned_memsz = align_up(report.tls_memsz, tls_align)
            .ok_or("runtime phase2: TLS align overflow.")?;
        let total_size = u64_to_usize(
            aligned_memsz.checked_add(8).ok_or("runtime phase2: TLS size overflow.")?
        ).ok_or("runtime phase2: TLS size out of range.")?;

        let mut tls_buf = alloc::vec![0u8; total_size];

        // Copy initialization data from the ELF file
        if report.tls_filesz > 0 {
            let tls_file_off = u64_to_usize(report.tls_offset)
                .ok_or("runtime phase2: TLS offset out of range.")?;
            let tls_copy_len = u64_to_usize(report.tls_filesz)
                .ok_or("runtime phase2: TLS filesz out of range.")?;
            let tls_src_end = tls_file_off.checked_add(tls_copy_len)
                .ok_or("runtime phase2: TLS file overflow.")?;
            if tls_src_end > raw.len() {
                return Err("runtime phase2: TLS init data fuera de archivo.");
            }
            let aligned_memsz_usize = u64_to_usize(aligned_memsz)
                .ok_or("runtime phase2: TLS memsz out of range.")?;
            // TLS data goes at the start of the block (negative offset from TCB)
            if tls_copy_len > aligned_memsz_usize {
                return Err("runtime phase2: TLS filesz > memsz.");
            }
            // Place init data right-aligned to the TCB position
            let tls_data_start = aligned_memsz_usize - tls_copy_len;
            tls_buf[tls_data_start..aligned_memsz_usize]
                .copy_from_slice(&raw[tls_file_off..tls_src_end]);
        }

        // Self-pointer at the TCB position (last 8 bytes)
        let aligned_memsz_usize = u64_to_usize(aligned_memsz)
            .ok_or("runtime phase2: TLS memsz out of range.")?;
        let tcb_ptr_val = tls_buf.as_ptr() as usize as u64 + aligned_memsz as u64;
        tls_buf[aligned_memsz_usize..aligned_memsz_usize + 8]
            .copy_from_slice(&tcb_ptr_val.to_le_bytes());

        (tls_buf, tcb_ptr_val)
    } else {
        (Vec::new(), 0u64)
    };

    Ok(RuntimeDynImage {
        report,
        dyn_info,
        image,
        phdr_blob,
        tls_block,
        tls_tcb_addr,
        image_start,
        image_size: span_size,
        load_bias,
        entry_virt,
        phdr_addr,
        phent: phent as u64,
        phnum: phnum as u64,
        sample_hash: hash,
        reloc_total: reloc_stats.total,
        reloc_applied: reloc_stats.applied,
        reloc_unsupported: reloc_stats.unsupported,
        reloc_errors: reloc_stats.errors,
    })
}

fn stack_place_cstr(stack: &mut [u8], stack_base: u64, top: &mut usize, text: &str) -> Option<u64> {
    let bytes = text.as_bytes();
    let total = bytes.len().checked_add(1)?;
    let start = top.checked_sub(total)?;
    let end = start.checked_add(bytes.len())?;
    if end >= stack.len() {
        return None;
    }
    stack[start..end].copy_from_slice(bytes);
    stack[end] = 0;
    *top = start;
    stack_base.checked_add(start as u64)
}

fn stack_place_aligned_bytes(
    stack: &mut [u8],
    stack_base: u64,
    top: &mut usize,
    data: &[u8],
    align: usize,
) -> Option<u64> {
    if align == 0 || !align.is_power_of_two() {
        return None;
    }
    let mut start = top.checked_sub(data.len())?;
    start &= !(align - 1);
    let end = start.checked_add(data.len())?;
    if end > stack.len() {
        return None;
    }
    stack[start..end].copy_from_slice(data);
    *top = start;
    stack_base.checked_add(start as u64)
}

fn build_linux_initial_stack(
    main: &RuntimeDynImage,
    interp: &RuntimeDynImage,
    argv_items: &[&str],
    execfn: &str,
    extra_env_items: &[&str],
) -> Result<(Vec<u8>, u64, usize, usize, usize), &'static str> {
    let mut stack = alloc::vec![0u8; LINUX_STACK_SIZE];
    let stack_base = stack.as_ptr() as usize as u64;
    let mut top = stack.len();

    let argv0_default = argv_items
        .first()
        .copied()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("/app");
    let execfn_value = execfn.trim();
    let execfn_value = if execfn_value.is_empty() {
        argv0_default
    } else {
        execfn_value
    };

    let execfn_ptr =
        stack_place_cstr(stack.as_mut_slice(), stack_base, &mut top, execfn_value).ok_or("stack: execfn.")?;

    // X11-oriented defaults so userland GUI apps can target the internal bridge.
    let base_env_items = [
        "LANG=C",
        "TERM=reduxos",
        "PATH=/",
        "REDUXOS=1",
        "DISPLAY=:0",
        "XDG_SESSION_TYPE=x11",
        // Force common GUI toolkits to prefer X11 inside the shim.
        "GDK_BACKEND=x11",
        "QT_QPA_PLATFORM=xcb",
        "SDL_VIDEODRIVER=x11",
        "WINIT_UNIX_BACKEND=x11",
        "MOZ_ENABLE_WAYLAND=0",
        "WAYLAND_DISPLAY=",
    ];
    let mut env_ptrs: Vec<u64> = Vec::new();
    for item in base_env_items.iter() {
        let ptr = stack_place_cstr(stack.as_mut_slice(), stack_base, &mut top, item)
            .ok_or("stack: envp.")?;
        env_ptrs.push(ptr);
    }
    for item in extra_env_items.iter() {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        let ptr = stack_place_cstr(stack.as_mut_slice(), stack_base, &mut top, trimmed)
            .ok_or("stack: envp.")?;
        env_ptrs.push(ptr);
    }

    let mut argv_ptrs: Vec<u64> = Vec::new();
    for item in argv_items.iter() {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        let ptr = stack_place_cstr(stack.as_mut_slice(), stack_base, &mut top, trimmed)
            .ok_or("stack: argv.")?;
        argv_ptrs.push(ptr);
    }
    if argv_ptrs.is_empty() {
        let ptr = stack_place_cstr(stack.as_mut_slice(), stack_base, &mut top, argv0_default)
            .ok_or("stack: argv0.")?;
        argv_ptrs.push(ptr);
    }

    let mut random_seed = [0u8; 16];
    let mut seed = (main.sample_hash as u64) << 32 | (interp.sample_hash as u64);
    let mut i = 0usize;
    while i < random_seed.len() {
        seed ^= seed >> 12;
        seed ^= seed << 25;
        seed ^= seed >> 27;
        random_seed[i] = (seed.wrapping_mul(0x2545F4914F6CDD1D) & 0xFF) as u8;
        i += 1;
    }
    let random_ptr = stack_place_aligned_bytes(
        stack.as_mut_slice(),
        stack_base,
        &mut top,
        random_seed.as_slice(),
        16,
    )
    .ok_or("stack: random.")?;

    let auxv = [
        (AT_PHDR, main.phdr_addr),
        (AT_PHENT, main.phent),
        (AT_PHNUM, main.phnum),
        (AT_PAGESZ, PAGE_SIZE),
        (AT_BASE, interp.load_bias),
        (AT_FLAGS, 0),
        (AT_ENTRY, main.entry_virt),
        (AT_UID, 0),
        (AT_EUID, 0),
        (AT_GID, 0),
        (AT_EGID, 0),
        (AT_SECURE, 0),
        (AT_RANDOM, random_ptr),
        (AT_EXECFN, execfn_ptr),
    ];

    let mut words: Vec<u64> = Vec::new();
    words.push(argv_ptrs.len() as u64);
    for ptr in argv_ptrs.iter() {
        words.push(*ptr);
    }
    words.push(0);
    for env in env_ptrs.iter() {
        words.push(*env);
    }
    words.push(0);
    for (key, value) in auxv.iter() {
        words.push(*key);
        words.push(*value);
    }
    words.push(AT_NULL);
    words.push(0);

    let words_bytes = words
        .len()
        .checked_mul(core::mem::size_of::<u64>())
        .ok_or("stack: overflow en words.")?;
    let unaligned_start = top
        .checked_sub(words_bytes)
        .ok_or("stack: insuficiente para metadata.")?;
    let words_start = unaligned_start & !0xFusize;
    if words_start + words_bytes > stack.len() {
        return Err("stack: layout fuera de rango.");
    }

    let mut cursor = words_start;
    for word in words.iter() {
        let end = cursor
            .checked_add(core::mem::size_of::<u64>())
            .ok_or("stack: cursor overflow.")?;
        stack[cursor..end].copy_from_slice(&word.to_le_bytes());
        cursor = end;
    }

    let stack_ptr = stack_base
        .checked_add(words_start as u64)
        .ok_or("stack: pointer overflow.")?;
    Ok((stack, stack_ptr, argv_ptrs.len(), env_ptrs.len(), auxv.len()))
}

pub struct LinuxDynDependencyInput<'a> {
    pub soname: &'a str,
    pub raw: &'a [u8],
}

fn prepare_phase2_interp_launch_with_deps_core(
    main_raw: &[u8],
    interp_raw: &[u8],
    deps: &[LinuxDynDependencyInput<'_>],
    argv_items: &[&str],
    execfn: &str,
    extra_env_items: &[&str],
) -> Result<LinuxDynLaunchPlan, &'static str> {
    let main_report = inspect_elf64(main_raw)?;
    let dynamic = inspect_dynamic_elf64(main_raw)?;
    phase2_dynamic_compatibility(&main_report, &dynamic)?;

    let mut main_image = stage_runtime_dyn_image(main_raw)?;
    let mut interp_image = stage_runtime_dyn_image(interp_raw)?;
    let mut symbol_traces: Vec<LinuxDynSymbolTrace> = Vec::new();
    let mut global_symbols: Vec<RuntimeGlobalSymbol> = Vec::new();
    runtime_collect_image_exports(main_raw, &main_image, "main", &mut global_symbols);
    let interp_provider = dynamic
        .interp_path
        .as_deref()
        .unwrap_or("interp");
    runtime_collect_image_exports(interp_raw, &interp_image, interp_provider, &mut global_symbols);
    let mut dep_images: Vec<RuntimeDynImage> = Vec::new();
    for dep in deps.iter() {
        if dep.raw.is_empty() {
            continue;
        }
        let dep_image = stage_runtime_dyn_image(dep.raw)?;
        let provider = if dep.soname.is_empty() {
            "dep"
        } else {
            dep.soname
        };
        runtime_collect_image_exports(dep.raw, &dep_image, provider, &mut global_symbols);
        dep_images.push(dep_image);
    }

    let main_global = runtime_apply_global_symbol_relocations(
        &mut main_image,
        main_raw,
        global_symbols.as_slice(),
        "main",
        &mut symbol_traces,
    );
    let interp_requestor = dynamic
        .interp_path
        .as_deref()
        .unwrap_or("interp");
    let interp_global = runtime_apply_global_symbol_relocations(
        &mut interp_image,
        interp_raw,
        global_symbols.as_slice(),
        interp_requestor,
        &mut symbol_traces,
    );
    {
        let mut dep_iter = dep_images.iter_mut();
        for dep in deps.iter() {
            if dep.raw.is_empty() {
                continue;
            }
            if let Some(dep_image) = dep_iter.next() {
                let requestor = if dep.soname.is_empty() {
                    "dep"
                } else {
                    dep.soname
                };
                let _ = runtime_apply_global_symbol_relocations(
                    dep_image,
                    dep.raw,
                    global_symbols.as_slice(),
                    requestor,
                    &mut symbol_traces,
                );
            }
        }
    }
    main_image.reloc_total = main_image.reloc_total.saturating_add(main_global.total);
    main_image.reloc_applied = main_image.reloc_applied.saturating_add(main_global.applied);
    main_image.reloc_unsupported = main_image
        .reloc_unsupported
        .saturating_sub(main_global.applied)
        .saturating_add(main_global.unsupported);
    main_image.reloc_errors = main_image.reloc_errors.saturating_add(main_global.errors);
    interp_image.reloc_total = interp_image.reloc_total.saturating_add(interp_global.total);
    interp_image.reloc_applied = interp_image.reloc_applied.saturating_add(interp_global.applied);
    interp_image.reloc_unsupported = interp_image
        .reloc_unsupported
        .saturating_sub(interp_global.applied)
        .saturating_add(interp_global.unsupported);
    interp_image.reloc_errors = interp_image.reloc_errors.saturating_add(interp_global.errors);

    let (stack_image, stack_ptr, argv_count, env_count, aux_pairs) =
        build_linux_initial_stack(
            &main_image,
            &interp_image,
            argv_items,
            execfn,
            extra_env_items,
        )?;

    let tls_tcb_addr = main_image.tls_tcb_addr;

    Ok(LinuxDynLaunchPlan {
        main_base: main_image.load_bias,
        main_entry: main_image.entry_virt,
        interp_base: interp_image.load_bias,
        interp_entry: interp_image.entry_virt,
        stack_ptr,
        stack_bytes: stack_image.len(),
        argv_count,
        env_count,
        aux_pairs,
        main_hash: main_image.sample_hash,
        interp_hash: interp_image.sample_hash,
        tls_tcb_addr,
        main_reloc_total: main_image.reloc_total,
        main_reloc_applied: main_image.reloc_applied,
        main_reloc_unsupported: main_image.reloc_unsupported,
        main_reloc_errors: main_image.reloc_errors,
        interp_reloc_total: interp_image.reloc_total,
        interp_reloc_applied: interp_image.reloc_applied,
        interp_reloc_unsupported: interp_image.reloc_unsupported,
        interp_reloc_errors: interp_image.reloc_errors,
        symbol_traces,
        main_image,
        interp_image,
        stack_image,
    })
}

pub fn prepare_phase2_interp_launch_with_deps_and_argv(
    main_raw: &[u8],
    interp_raw: &[u8],
    deps: &[LinuxDynDependencyInput<'_>],
    argv_items: &[&str],
    execfn: &str,
    extra_env_items: &[&str],
) -> Result<LinuxDynLaunchPlan, &'static str> {
    prepare_phase2_interp_launch_with_deps_core(
        main_raw,
        interp_raw,
        deps,
        argv_items,
        execfn,
        extra_env_items,
    )
}

pub fn prepare_phase2_interp_launch_with_deps(
    main_raw: &[u8],
    interp_raw: &[u8],
    deps: &[LinuxDynDependencyInput<'_>],
    argv0: &str,
    execfn: &str,
) -> Result<LinuxDynLaunchPlan, &'static str> {
    let argv_items = [argv0];
    prepare_phase2_interp_launch_with_deps_core(
        main_raw,
        interp_raw,
        deps,
        argv_items.as_slice(),
        execfn,
        &[],
    )
}

pub fn prepare_phase2_interp_launch(
    main_raw: &[u8],
    interp_raw: &[u8],
    argv0: &str,
    execfn: &str,
) -> Result<LinuxDynLaunchPlan, &'static str> {
    let argv_items = [argv0];
    prepare_phase2_interp_launch_with_deps_core(
        main_raw,
        interp_raw,
        &[],
        argv_items.as_slice(),
        execfn,
        &[],
    )
}

#[cfg(target_arch = "x86_64")]
unsafe fn transfer_to_interp(entry: u64, stack_ptr: u64, tls_tcb_addr: u64) -> ! {
    // Set FS base to TLS TCB if TLS is configured
    if tls_tcb_addr != 0 {
        // Write IA32_FS_BASE MSR (0xC0000100) with the TCB address
        let msr: u32 = 0xC000_0100;
        let lo: u32 = tls_tcb_addr as u32;
        let hi: u32 = (tls_tcb_addr >> 32) as u32;
        asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") lo,
            in("edx") hi,
            options(nomem, nostack)
        );
    }

    asm!(
        "cli",
        "mov rsp, {stack}",
        "xor rbp, rbp",
        "xor rax, rax",
        "xor rbx, rbx",
        "xor rcx, rcx",
        "xor rdx, rdx",
        "xor rsi, rsi",
        "xor rdi, rdi",
        "xor r8, r8",
        "xor r9, r9",
        "xor r10, r10",
        "xor r11, r11",
        "xor r12, r12",
        "xor r13, r13",
        "xor r14, r14",
        "xor r15, r15",
        "jmp {entry}",
        stack = in(reg) stack_ptr,
        entry = in(reg) entry,
        options(noreturn)
    );
}

pub fn launch_phase2_interp(plan: LinuxDynLaunchPlan) -> ! {
    let entry = plan.interp_entry;
    let stack_ptr = plan.stack_ptr;
    let tls_tcb_addr = plan.tls_tcb_addr;
    core::mem::forget(plan);
    #[cfg(target_arch = "x86_64")]
    unsafe {
        transfer_to_interp(entry, stack_ptr, tls_tcb_addr);
    }

    #[cfg(not(target_arch = "x86_64"))]
    loop {}
}
