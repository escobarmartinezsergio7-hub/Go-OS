use crate::println;

// Simple fixed-size array to mock a HashMap for no_std
#[derive(Clone, Copy)]
struct QuotaEntry {
    app_id_hash: u64, // Simple hash of the string
    limit: u64,
    usage: u64,
}

pub struct QuotaManager {
    entries: [QuotaEntry; 16], // Track up to 16 apps for now
}

impl QuotaManager {
    pub const fn new() -> Self {
        Self {
            entries: [QuotaEntry { app_id_hash: 0, limit: 0, usage: 0 }; 16],
        }
    }

    fn hash(s: &str) -> u64 {
        let mut h = 0u64;
        for b in s.bytes() {
            h = h.wrapping_add(b as u64);
        }
        h
    }

    pub fn set_limit(&mut self, app_id: &str, limit: u64) {
        let h = Self::hash(app_id);
        for entry in self.entries.iter_mut() {
            if entry.app_id_hash == 0 || entry.app_id_hash == h {
                entry.app_id_hash = h;
                entry.limit = limit;
                return;
            }
        }
        println("QuotaManager: Table full!");
    }

    pub fn check_write(&mut self, app_id: &str, size: u64) -> bool {
        let h = Self::hash(app_id);
        for entry in self.entries.iter_mut() {
            if entry.app_id_hash == h {
                if entry.usage + size > entry.limit {
                    println("Quota Exceeded for App!");
                    return false;
                }
                entry.usage += size;
                return true;
            }
        }
        // If app not found, assume no limit? Or default limit?
        // For safety, let's say true but warn.
        // println("Quota: Unknown app, allowing.");
        true
    }
    
    pub fn get_usage(&self, app_id: &str) -> u64 {
        let h = Self::hash(app_id);
        for entry in self.entries.iter() {
            if entry.app_id_hash == h {
                return entry.usage;
            }
        }
        0
    }
}

static mut GLOBAL_QUOTA: QuotaManager = QuotaManager::new();

pub fn init() {
    println("QuotaManager: Initialized.");
    unsafe {
        GLOBAL_QUOTA.set_limit("system", 1024 * 1024 * 10); // 10MB for system
        GLOBAL_QUOTA.set_limit("user_data", 1024 * 1024 * 100); // 100MB for user
    }
}

pub fn test_quota() {
    unsafe {
        if GLOBAL_QUOTA.check_write("system", 1024 * 1024) {
            println("Quota Test: Write 1MB to system [OK]");
        } else {
             println("Quota Test: Write 1MB to system [FAIL]");
        }

        // Try to overflow
        if GLOBAL_QUOTA.check_write("system", 1024 * 1024 * 10) {
             println("Quota Test: Write 10MB to system [FAIL - Should exceed]");
        } else {
             println("Quota Test: Write 10MB to system [OK - Blocked]");
        }
    }
}
