use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const RUNTIME_COPY_MAX_BYTES: usize = 256 * 1024 * 1024;
const LINUXRT_BUNDLE_MAGIC: &[u8; 6] = b"RLTB1\0";

#[derive(Clone, Copy, PartialEq, Eq)]
enum RuntimeBucket {
    Lib = 0,
    Lib64 = 1,
    UsrLib = 2,
    UsrLib64 = 3,
}

#[derive(Clone)]
struct RuntimeBundleEntry {
    short_name: [u8; 11],
    source_path: String,
    bucket: RuntimeBucket,
    content: Vec<u8>,
}

fn find_simpleservo_archive(lib_dir: &Path) -> Option<PathBuf> {
    let candidates = [
        lib_dir.join("libsimpleservo.a"),
        lib_dir.join("simpleservo.lib"),
    ];
    for candidate in candidates {
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn find_vaev_archive(lib_dir: &Path) -> Option<PathBuf> {
    let candidates = [
        lib_dir.join("libvaevbridge.a"),
        lib_dir.join("vaevbridge.lib"),
    ];
    for candidate in candidates {
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn parse_runtime_manifest_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    if trimmed.eq_ignore_ascii_case("LINUXRT INSTALL") {
        return None;
    }

    let split = trimmed.find("<-")?;
    let left = trimmed[..split].trim();
    let right = trimmed[split + 2..].trim();
    if right.is_empty() {
        return None;
    }

    let mut parts = left.split_whitespace();
    let _index = parts.next()?;
    let short = parts.next()?.trim();
    if short.is_empty() {
        return None;
    }

    Some((short.to_string(), right.to_string()))
}

fn to_short_name_11(name: &str) -> Option<[u8; 11]> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (stem_raw, ext_raw) = if let Some(dot) = trimmed.rfind('.') {
        (&trimmed[..dot], &trimmed[dot + 1..])
    } else {
        (trimmed, "")
    };
    if stem_raw.is_empty() {
        return None;
    }

    let mut out = [b' '; 11];
    let mut stem_len = 0usize;
    for b in stem_raw.bytes() {
        if stem_len >= 8 {
            break;
        }
        let c = b.to_ascii_uppercase();
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' || c == b'~' || c == b'$' {
            out[stem_len] = c;
        } else {
            out[stem_len] = b'_';
        }
        stem_len += 1;
    }

    let mut ext_len = 0usize;
    for b in ext_raw.bytes() {
        if ext_len >= 3 {
            break;
        }
        let c = b.to_ascii_uppercase();
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' {
            out[8 + ext_len] = c;
        } else {
            out[8 + ext_len] = b'_';
        }
        ext_len += 1;
    }

    Some(out)
}

fn runtime_bucket_from_source_path(source_path: &str) -> RuntimeBucket {
    let normalized = source_path
        .replace('\\', "/")
        .to_ascii_lowercase();

    if normalized.starts_with("usr/lib64/") || normalized.contains("/usr/lib64/") {
        RuntimeBucket::UsrLib64
    } else if normalized.starts_with("usr/lib/") || normalized.contains("/usr/lib/") {
        RuntimeBucket::UsrLib
    } else if normalized.starts_with("lib64/") || normalized.contains("/lib64/") {
        RuntimeBucket::Lib64
    } else {
        RuntimeBucket::Lib
    }
}

fn runtime_bucket_rel_dir(bucket: RuntimeBucket) -> &'static str {
    match bucket {
        RuntimeBucket::Lib => "LIB",
        RuntimeBucket::Lib64 => "LIB64",
        RuntimeBucket::UsrLib => "USR/LIB",
        RuntimeBucket::UsrLib64 => "USR/LIB64",
    }
}

fn runtime_bucket_source_prefix(bucket: RuntimeBucket) -> &'static str {
    match bucket {
        RuntimeBucket::Lib => "lib",
        RuntimeBucket::Lib64 => "lib64",
        RuntimeBucket::UsrLib => "usr/lib",
        RuntimeBucket::UsrLib64 => "usr/lib64",
    }
}

fn ascii_lower_owned(text: &str) -> String {
    text.as_bytes()
        .iter()
        .map(|b| b.to_ascii_lowercase() as char)
        .collect()
}

fn push_runtime_entry(
    out: &mut Vec<RuntimeBundleEntry>,
    total_bytes: &mut usize,
    short_name: [u8; 11],
    source_path: String,
    bucket: RuntimeBucket,
    content: Vec<u8>,
) {
    if content.is_empty() || content.len() > u32::MAX as usize {
        return;
    }
    if out
        .iter()
        .any(|existing| existing.short_name == short_name && existing.bucket == bucket)
    {
        return;
    }
    if total_bytes.saturating_add(content.len()) > RUNTIME_COPY_MAX_BYTES {
        return;
    }

    *total_bytes += content.len();
    out.push(RuntimeBundleEntry {
        short_name,
        source_path,
        bucket,
        content,
    });
}

fn load_runtime_entries_from_manifest(runtime_root: &Path) -> Vec<RuntimeBundleEntry> {
    let manifest_path = runtime_root.join("RTBASE.LST");
    let manifest_text = match fs::read_to_string(manifest_path) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut out = Vec::new();
    let mut total_bytes = 0usize;

    for line in manifest_text.lines() {
        let Some((short_name_text, source_path)) = parse_runtime_manifest_line(line) else {
            continue;
        };
        let Some(short_name) = to_short_name_11(&short_name_text) else {
            continue;
        };
        let bucket = runtime_bucket_from_source_path(&source_path);
        let file_path = runtime_root
            .join(runtime_bucket_rel_dir(bucket))
            .join(&short_name_text);
        let Ok(content) = fs::read(file_path) else {
            continue;
        };
        push_runtime_entry(
            &mut out,
            &mut total_bytes,
            short_name,
            source_path,
            bucket,
            content,
        );
    }

    out
}

fn scan_runtime_bucket(
    runtime_root: &Path,
    bucket: RuntimeBucket,
    out: &mut Vec<RuntimeBundleEntry>,
    total_bytes: &mut usize,
) {
    let dir_path = runtime_root.join(runtime_bucket_rel_dir(bucket));
    let Ok(entries) = fs::read_dir(dir_path) else {
        return;
    };

    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_file() {
            continue;
        }
        let Some(file_name) = entry.file_name().to_str().map(|s| s.to_string()) else {
            continue;
        };
        let Some(short_name) = to_short_name_11(&file_name) else {
            continue;
        };
        let Ok(content) = fs::read(entry.path()) else {
            continue;
        };
        let source_path = format!(
            "{}/{}",
            runtime_bucket_source_prefix(bucket),
            ascii_lower_owned(&file_name)
        );
        push_runtime_entry(out, total_bytes, short_name, source_path, bucket, content);
    }
}

fn load_runtime_entries(runtime_root: &Path) -> Vec<RuntimeBundleEntry> {
    let mut out = load_runtime_entries_from_manifest(runtime_root);
    if !out.is_empty() {
        out.sort_by_key(|e| (e.bucket as u8, e.short_name, e.source_path.clone()));
        return out;
    }

    let mut total_bytes = 0usize;
    scan_runtime_bucket(runtime_root, RuntimeBucket::Lib, &mut out, &mut total_bytes);
    scan_runtime_bucket(runtime_root, RuntimeBucket::Lib64, &mut out, &mut total_bytes);
    scan_runtime_bucket(runtime_root, RuntimeBucket::UsrLib, &mut out, &mut total_bytes);
    scan_runtime_bucket(runtime_root, RuntimeBucket::UsrLib64, &mut out, &mut total_bytes);
    out.sort_by_key(|e| (e.bucket as u8, e.short_name, e.source_path.clone()));
    out
}

fn write_runtime_bundle(bundle_path: &Path, entries: &[RuntimeBundleEntry]) -> io::Result<()> {
    let mut data = Vec::new();
    data.extend_from_slice(LINUXRT_BUNDLE_MAGIC);
    data.extend_from_slice(&(entries.len() as u32).to_le_bytes());

    for entry in entries {
        let source = entry.source_path.as_bytes();
        let source_len_u16 = u16::try_from(source.len()).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "runtime source path is too long for bundle format",
            )
        })?;

        data.extend_from_slice(&entry.short_name);
        data.push(entry.bucket as u8);
        data.extend_from_slice(&source_len_u16.to_le_bytes());
        data.extend_from_slice(&(entry.content.len() as u32).to_le_bytes());
        data.extend_from_slice(source);
        data.extend_from_slice(&entry.content);
    }

    let mut file = fs::File::create(bundle_path)?;
    file.write_all(&data)?;
    Ok(())
}

fn build_linuxrt_bundle(manifest_dir: &Path, out_dir: &Path) -> PathBuf {
    let workspace_root = manifest_dir
        .parent()
        .map_or_else(|| manifest_dir.to_path_buf(), Path::to_path_buf);
    let runtime_root = workspace_root.join("LINUXRT");
    let bundle_path = out_dir.join("linuxrt.bundle");

    println!("cargo:rerun-if-changed={}", runtime_root.display());

    let entries = if runtime_root.is_dir() {
        load_runtime_entries(runtime_root.as_path())
    } else {
        Vec::new()
    };

    if let Err(err) = write_runtime_bundle(bundle_path.as_path(), entries.as_slice()) {
        println!(
            "cargo:warning=failed to write embedded linuxrt bundle ({}). using empty bundle.",
            err
        );
        let _ = write_runtime_bundle(bundle_path.as_path(), &[]);
    }

    let total_bytes = entries
        .iter()
        .fold(0usize, |acc, e| acc.saturating_add(e.content.len()));
    println!(
        "cargo:warning=embedded linuxrt bundle entries={} bytes={}",
        entries.len(),
        total_bytes
    );

    bundle_path
}

fn main() {
    println!("cargo:rustc-check-cfg=cfg(servo_external_unavailable)");
    println!("cargo:rustc-check-cfg=cfg(vaev_external_unavailable)");
    println!("cargo:rerun-if-env-changed=SERVO_LIB_DIR");
    println!("cargo:rerun-if-env-changed=VAEV_LIB_DIR");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into()));
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap_or_else(|_| ".".into()));
    let linuxrt_bundle = build_linuxrt_bundle(manifest_dir.as_path(), out_dir.as_path());
    println!("cargo:rustc-env=REDUX_LINUXRT_BUNDLE={}", linuxrt_bundle.display());

    if env::var_os("CARGO_FEATURE_SERVO_EXTERNAL").is_none() {
    } else {
        let default_dir = manifest_dir.join("third_party").join("servo").join("lib");
        let lib_dir = env::var_os("SERVO_LIB_DIR")
            .map(PathBuf::from)
            .unwrap_or(default_dir);

        println!("cargo:rerun-if-changed={}", lib_dir.display());
        println!(
            "cargo:rerun-if-changed={}",
            lib_dir.join("libsimpleservo.a").display()
        );
        println!(
            "cargo:rerun-if-changed={}",
            lib_dir.join("simpleservo.lib").display()
        );

        if let Some(archive) = find_simpleservo_archive(lib_dir.as_path()) {
            println!("cargo:rustc-link-search=native={}", lib_dir.display());
            println!("cargo:rustc-link-lib=static=simpleservo");
            println!(
                "cargo:warning=servo_external: linking {}",
                archive.display()
            );
        } else {
            println!("cargo:rustc-cfg=servo_external_unavailable");
            println!(
                "cargo:warning=servo_external enabled but libsimpleservo was not found in {}. Falling back to integrated shim.",
                lib_dir.display()
            );
        }
    }

    if env::var_os("CARGO_FEATURE_VAEV_EXTERNAL").is_none() {
        return;
    }

    let default_dir = manifest_dir.join("third_party").join("vaev").join("lib");
    let lib_dir = env::var_os("VAEV_LIB_DIR")
        .map(PathBuf::from)
        .unwrap_or(default_dir);

    println!("cargo:rerun-if-changed={}", lib_dir.display());
    println!(
        "cargo:rerun-if-changed={}",
        lib_dir.join("libvaevbridge.a").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        lib_dir.join("vaevbridge.lib").display()
    );

    if let Some(archive) = find_vaev_archive(lib_dir.as_path()) {
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        println!("cargo:rustc-link-lib=static=vaevbridge");
        println!(
            "cargo:warning=vaev_external: linking {}",
            archive.display()
        );
    } else {
        println!("cargo:rustc-cfg=vaev_external_unavailable");
        println!(
            "cargo:warning=vaev_external enabled but libvaevbridge was not found in {}. Falling back to integrated shim.",
            lib_dir.display()
        );
    }
}
