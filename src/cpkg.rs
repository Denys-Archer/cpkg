use crate::error::{CpkgError, Result};
use crate::keys::SortBy;
use crate::note::ArchiveNote;
use anyhow::Context;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use zstd::stream::read::Decoder;

pub const MAGIC: &[u8; 4] = b"CPKG";
pub const VERSION: u8 = 2;
pub const FLAG_HAS_NOTE: u8 = 0b0000_0001;

#[derive(Debug, Clone, Serialize)]
pub struct FileEntryHeader {
    pub path: String,
    pub mode: u32,
    pub size: u64,
}

#[derive(Debug, Serialize)]
pub struct ArchiveSummary {
    pub version: u8,
    pub entries: Vec<FileEntryHeader>,
    pub total_uncompressed: u64,
    pub compressed_size: u64,
    pub archive_sha256: String,
    pub note: Option<ArchiveNote>,
}

pub fn write_global_header<W: Write>(writer: &mut W, note: Option<&ArchiveNote>) -> Result<()> {
    writer.write_all(MAGIC)?;
    writer.write_all(&[VERSION])?;

    let mut flags: u8 = 0;
    let encoded_note = match note {
        Some(n) => {
            flags |= FLAG_HAS_NOTE;
            Some(n.to_bytes()?)
        }
        None => None,
    };

    writer.write_all(&[flags])?;
    if let Some(bytes) = encoded_note {
        let len = u32::try_from(bytes.len())
            .map_err(|_| CpkgError::InvalidFormat("Encoded note too large".into()))?;
        writer.write_all(&len.to_le_bytes())?;
        writer.write_all(&bytes)?;
    }

    Ok(())
}

pub fn read_global_header<R: Read>(reader: &mut R) -> Result<(u8, Option<ArchiveNote>)> {
    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(CpkgError::InvalidMagic);
    }

    let mut ver = [0u8; 1];
    reader.read_exact(&mut ver)?;
    if ver[0] != VERSION {
        return Err(CpkgError::InvalidFormat(format!(
            "Unsupported archive version: {}",
            ver[0]
        )));
    }

    let mut flags_buf = [0u8; 1];
    reader.read_exact(&mut flags_buf)?;
    let flags = flags_buf[0];
    if flags & !FLAG_HAS_NOTE != 0 {
        return Err(CpkgError::InvalidFormat(format!("Unknown global flags: {:#04x}", flags)));
    }

    let note = if (flags & FLAG_HAS_NOTE) != 0 {
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)?;
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut note_buf = vec![0u8; len];
        reader.read_exact(&mut note_buf)?;
        Some(ArchiveNote::from_bytes(&note_buf)?)
    } else {
        None
    };

    Ok((ver[0], note))
}

pub fn write_entry_header<W: Write>(writer: &mut W, header: &FileEntryHeader) -> Result<()> {
    let path_bytes = header.path.as_bytes();
    let path_len = u16::try_from(path_bytes.len())
        .map_err(|_| CpkgError::InvalidFormat("Path exceeds 65535 bytes".into()))?;

    writer.write_all(&path_len.to_le_bytes())?;
    writer.write_all(path_bytes)?;
    writer.write_all(&header.mode.to_le_bytes())?;
    writer.write_all(&header.size.to_le_bytes())?;
    Ok(())
}

pub fn read_entry_header<R: Read>(reader: &mut R) -> Result<Option<FileEntryHeader>> {
    let mut path_len_buf = [0u8; 2];
    // EOF is valid only when no bytes of the next entry header are present.
    // A partially written length field means the archive is truncated.
    let mut first = [0u8; 1];
    match reader.read(&mut first)? {
        0 => return Ok(None),
        1 => path_len_buf[0] = first[0],
        _ => unreachable!(),
    }
    reader.read_exact(&mut path_len_buf[1..])?;

    let path_len = u16::from_le_bytes(path_len_buf) as usize;
    let mut path_buf = vec![0u8; path_len];
    reader.read_exact(&mut path_buf)?;
    let path = String::from_utf8(path_buf)
        .map_err(|_| CpkgError::InvalidFormat("Invalid UTF-8 in file path".into()))?;

    let mut mode_buf = [0u8; 4];
    reader.read_exact(&mut mode_buf)?;
    let mode = u32::from_le_bytes(mode_buf);

    let mut size_buf = [0u8; 8];
    reader.read_exact(&mut size_buf)?;
    let size = u64::from_le_bytes(size_buf);

    Ok(Some(FileEntryHeader { path, mode, size }))
}

// Compute SHA-256 hex digest matching sha256sum output
pub fn compute_file_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)
        .with_context(|| format!("Failed to open {:?} for SHA-256", path))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];

    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(hex::encode(hasher.finalize()))
}

// Generate sha256sum-formatted lines: "<hex>  <path>"
pub fn generate_sha256sum_lines(targets: &[PathBuf]) -> Result<Vec<String>> {
    let mut lines = Vec::new();

    for target in targets {
        for entry in WalkDir::new(target) {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let digest = compute_file_sha256(path)?;
                let label = path.to_string_lossy();
                lines.push(format!("{}  {}", digest, label));
            }
        }
    }

    Ok(lines)
}

// Parse a sha256sum-compatible file into path -> expected lowercase hex digest
pub fn parse_sha256sum_file(sha_path: &Path) -> Result<HashMap<String, String>> {
    let content = fs::read_to_string(sha_path)
        .with_context(|| format!("Failed to read SHA256 file {:?}", sha_path))?;
    let mut map = HashMap::new();

    for line in content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let mut parts = line.splitn(2, char::is_whitespace);
        let digest = parts
            .next()
            .ok_or_else(|| CpkgError::InvalidFormat("Missing SHA-256 digest".into()))?
            .trim()
            .to_lowercase();
        let raw_path = parts
            .next()
            .ok_or_else(|| CpkgError::InvalidFormat("Missing file path in SHA256 line".into()))?
            .trim()
            .trim_start_matches('*')
            .to_string();

        map.insert(raw_path, digest);
    }

    Ok(map)
}

pub fn inspect_archive(archive_path: &Path, sort_by: SortBy) -> Result<ArchiveSummary> {
    let compressed_size = fs::metadata(archive_path)?.len();
    let archive_sha256 = compute_file_sha256(archive_path)?;

    let file = File::open(archive_path)?;
    let mut reader = BufReader::new(file);
    let (version, note) = read_global_header(&mut reader)?;

    let mut decoder = Decoder::new(&mut reader)?;
    let mut entries = Vec::new();
    let mut total_uncompressed: u64 = 0;

    while let Some(header) = read_entry_header(&mut decoder)? {
        let copied = io::copy(&mut (&mut decoder).take(header.size), &mut io::sink())?;
        if copied != header.size {
            return Err(CpkgError::InvalidFormat(format!(
                "Truncated entry '{}' in archive",
                header.path
            )));
        }
        total_uncompressed = total_uncompressed.checked_add(header.size)
            .ok_or_else(|| CpkgError::InvalidFormat("Uncompressed size overflow".into()))?;
        entries.push(header);
    }

    match sort_by {
        SortBy::Name => entries.sort_by(|a, b| a.path.cmp(&b.path)),
        SortBy::Size => {
            entries.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)))
        }
    }

    Ok(ArchiveSummary {
        version,
        entries,
        total_uncompressed,
        compressed_size,
        archive_sha256,
        note,
    })
}

pub fn verify_archive(archive_path: &Path, sha256_file: Option<&Path>) -> Result<ArchiveSummary> {
    let summary = inspect_archive(archive_path, SortBy::Name)?;

    if let Some(sha_path) = sha256_file {
        let expected_map = parse_sha256sum_file(sha_path)?;
        let archive_str = archive_path.to_string_lossy().to_string();
        let archive_name = archive_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let expected = expected_map
            .get(&archive_str)
            .or_else(|| expected_map.get(&archive_name))
            .ok_or_else(|| {
                CpkgError::InvalidFormat(format!(
                    "No SHA-256 entry found for {:?} in {:?}",
                    archive_path, sha_path
                ))
            })?;

        if &summary.archive_sha256 != expected {
            return Err(CpkgError::ChecksumMismatch(format!(
                "{:?}: expected {}, got {}",
                archive_path, expected, summary.archive_sha256
            )));
        }
    }

    Ok(summary)
}
