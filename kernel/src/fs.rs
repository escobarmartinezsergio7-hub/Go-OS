use core::str;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    File,
    Directory,
}

#[derive(Debug, Clone, Copy)]
pub struct DirEntry {
    pub name: [u8; 11], // 8.3 format for simplicity
    pub display_name: [u8; 255], // UTF-8 (LFN if available)
    pub display_len: u16,
    pub size: u32,
    pub file_type: FileType,
    pub cluster: u32,
    pub valid: bool,
}

impl DirEntry {
    pub const fn empty() -> Self {
        Self {
            name: [0; 11],
            display_name: [0; 255],
            display_len: 0,
            size: 0,
            file_type: FileType::File,
            cluster: 0,
            valid: false,
        }
    }

    pub fn name_as_str(&self) -> &str {
        // Basic conversion, trimming spaces
        let len = self.name.iter().position(|&c| c == 0).unwrap_or(self.name.len());
        unsafe { str::from_utf8_unchecked(&self.name[0..len]) }.trim()
    }

    fn short_name(&self) -> alloc::string::String {
        let mut name = alloc::string::String::new();
        // Name part (indices 0..8)
        let name_part = &self.name[0..8];
        let name_len = name_part.iter().position(|&c| c == b' ' || c == 0).unwrap_or(8);
        name.push_str(unsafe { str::from_utf8_unchecked(&name_part[0..name_len]) });

        if self.file_type == FileType::File {
            // Extension part (indices 8..11)
            let ext_part = &self.name[8..11];
            let ext_len = ext_part.iter().position(|&c| c == b' ' || c == 0).unwrap_or(3);
            if ext_len > 0 {
                name.push('.');
                name.push_str(unsafe { str::from_utf8_unchecked(&ext_part[0..ext_len]) });
            }
        }
        name
    }

    pub fn full_name(&self) -> alloc::string::String {
        if self.display_len > 0 {
            let len = (self.display_len as usize).min(self.display_name.len());
            if let Ok(s) = core::str::from_utf8(&self.display_name[..len]) {
                if !s.is_empty() {
                    return alloc::string::String::from(s);
                }
            }
        }
        self.short_name()
    }

    pub fn set_display_name(&mut self, text: &str) {
        let mut n = text.len().min(self.display_name.len());
        while n > 0 && !text.is_char_boundary(n) {
            n -= 1;
        }

        self.display_name = [0; 255];
        self.display_name[..n].copy_from_slice(&text.as_bytes()[..n]);
        self.display_len = n as u16;
    }

    pub fn matches_name(&self, target: &str) -> bool {
        if self.full_name().eq_ignore_ascii_case(target) {
            return true;
        }
        self.name_as_str().eq_ignore_ascii_case(target)
    }
}

pub trait FileSystem {
    fn init(&mut self) -> bool;
    fn root_dir(&mut self) -> Result<u32, &'static str>; // Returns cluster of root
    fn read_dir(&mut self, cluster: u32) -> Result<[DirEntry; 16], &'static str>; // Fixed size for now
    fn read_file(&mut self, cluster: u32, buffer: &mut [u8]) -> Result<usize, &'static str>;
}

// Global VFS instance (simplified)
pub struct Vfs {
    // In a real OS, this would be dynamic.
    // Here we will just hold a single optional FS.
}
