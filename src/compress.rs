use crate::cpkg::{compute_file_sha256, write_entry_header, write_global_header, FileEntryHeader};
use crate::error::{CpkgError, Result};
use crate::keys::GlobalFlags;
use crate::note::ArchiveNote;
use crate::warning::{info, warn};
use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::Regex;
use std::fs::{self, File};
use std::io::{self, BufWriter};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use zstd::stream::write::Encoder;

pub struct PackStats {
    pub original_size: u64,
    pub compressed_size: u64,
    pub files_count: usize,
    pub skipped_count: usize,
    pub archive_sha256: String,
}

fn build_excludes(excludes: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in excludes {
        let glob = Glob::new(pattern).map_err(|e| {
            CpkgError::InvalidArgs(format!("Invalid glob pattern '{}': {}", pattern, e))
        })?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|e| CpkgError::InvalidArgs(format!("Failed to compile glob patterns: {}", e)))
}

fn should_skip(
    path: &Path,
    rel_path: &str,
    exclude_set: &GlobSet,
    extensions: &[String],
    regex_filter: Option<&Regex>,
) -> bool {
    if exclude_set.is_match(path) || exclude_set.is_match(rel_path) {
        return true;
    }
    if let Some(re) = regex_filter {
        if !re.is_match(rel_path) {
            return true;
        }
    }
    if !extensions.is_empty() {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());
        match ext {
            Some(e) if extensions.contains(&e) => {}
            _ => return true,
        }
    }
    false
}

pub fn pack_files(
    archive_path: &Path,
    targets: &[PathBuf],
    level: i32,
    excludes: &[String],
    extensions: &[String],
    regex_pattern: Option<&str>,
    note: Option<&ArchiveNote>,
    flags: &GlobalFlags,
) -> Result<PackStats> {
    if archive_path.exists() && !flags.force && !flags.dry_run {
        return Err(CpkgError::AlreadyExists(
            archive_path.to_string_lossy().into_owned(),
        ));
    }

    let exclude_set = build_excludes(excludes)?;
    let regex_filter = match regex_pattern {
        Some(pat) => Some(
            Regex::new(pat)
                .map_err(|e| CpkgError::InvalidArgs(format!("Invalid regex '{}': {}", pat, e)))?,
        ),
        None => None,
    };

    if flags.dry_run {
        let mut original_size: u64 = 0;
        let mut files_count: usize = 0;
        let mut skipped_count: usize = 0;

        for target in targets {
            if !target.exists() {
                warn(&format!("Target does not exist: {:?}", target));
                continue;
            }
            let base_parent = target.parent().unwrap_or_else(|| Path::new(""));
            for entry in WalkDir::new(target) {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    let rel_path = path
                        .strip_prefix(base_parent)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .into_owned();
                    if should_skip(
                        path,
                        &rel_path,
                        &exclude_set,
                        extensions,
                        regex_filter.as_ref(),
                    ) {
                        skipped_count += 1;
                        continue;
                    }
                    let size = fs::metadata(path)?.len();
                    if flags.verbose && !flags.quiet {
                        info(&format!("would pack: {} ({} B)", rel_path, size));
                    }
                    original_size += size;
                    files_count += 1;
                }
            }
        }

        return Ok(PackStats {
            original_size,
            compressed_size: 0,
            files_count,
            skipped_count,
            archive_sha256: String::new(),
        });
    }

    let out_file = File::create(archive_path)?;
    let mut buf_writer = BufWriter::new(out_file);

    write_global_header(&mut buf_writer, note)?;

    let mut encoder = Encoder::new(&mut buf_writer, level)?;
    if !flags.no_checksum {
        encoder.include_checksum(true)?;
    }

    let mut original_size: u64 = 0;
    let mut files_count: usize = 0;
    let mut skipped_count: usize = 0;

    for target in targets {
        if !target.exists() {
            warn(&format!("Skipping missing target: {:?}", target));
            continue;
        }
        let base_parent = target.parent().unwrap_or_else(|| Path::new(""));

        for entry in WalkDir::new(target) {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                let rel_path = path
                    .strip_prefix(base_parent)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .into_owned();

                if should_skip(
                    path,
                    &rel_path,
                    &exclude_set,
                    extensions,
                    regex_filter.as_ref(),
                ) {
                    skipped_count += 1;
                    continue;
                }

                let metadata = fs::metadata(path)?;
                let file_size = metadata.len();
                let mode = if flags.ignore_perms {
                    0o644
                } else {
                    metadata.permissions().mode()
                };

                if flags.verbose && !flags.quiet {
                    info(&format!("packing: {} ({} B)", rel_path, file_size));
                }

                let header = FileEntryHeader {
                    path: rel_path,
                    mode,
                    size: file_size,
                };

                write_entry_header(&mut encoder, &header)?;

                let mut src_file = File::open(path)?;
                io::copy(&mut src_file, &mut encoder)?;

                original_size += file_size;
                files_count += 1;
            }
        }
    }

    encoder.finish()?;
    drop(buf_writer);

    let compressed_size = fs::metadata(archive_path)?.len();
    let archive_sha256 = compute_file_sha256(archive_path)?;

    if original_size > 0 && compressed_size >= original_size && !flags.quiet {
        warn("Archive is not smaller than original data (already compressed or tiny files)");
    }

    Ok(PackStats {
        original_size,
        compressed_size,
        files_count,
        skipped_count,
        archive_sha256,
    })
}
