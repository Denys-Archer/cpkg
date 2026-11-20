use crate::cpkg::{read_entry_header, read_global_header};
use crate::error::{CpkgError, Result};
use crate::keys::GlobalFlags;
use crate::warning::info;
use std::fs::{self, File};
use std::io::{self, BufReader, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use zstd::stream::read::Decoder;

/// Accept only portable relative paths. Blocks absolute paths and traversal.
fn safe_relative_path(raw: &str) -> Result<PathBuf> {
    let path = Path::new(raw);
    if raw.is_empty() || raw.contains('\\') || raw.contains(':') || path.components().any(|part| !matches!(part, Component::Normal(_))) {
        return Err(CpkgError::InvalidFormat(format!("Unsafe path in archive: {}", raw)));
    }
    Ok(path.to_path_buf())
}

pub fn unpack_archive(
    archive_path: &Path,
    output_dir: &Path,
    filter_prefix: Option<&str>,
    flags: &GlobalFlags,
) -> Result<usize> {
    let in_file = File::open(archive_path)?;
    let mut buf_reader = BufReader::new(in_file);

    let (_version, _note) = read_global_header(&mut buf_reader)?;
    let mut decoder = Decoder::new(&mut buf_reader)?;

    if !flags.dry_run {
        fs::create_dir_all(output_dir)?;
    }

    let mut extracted_count = 0;

    while let Some(header) = read_entry_header(&mut decoder)? {
        let relative_path = safe_relative_path(&header.path)?;

        if let Some(prefix) = filter_prefix {
            if !header.path.starts_with(prefix) {
                io::copy(&mut (&mut decoder).take(header.size), &mut io::sink())?;
                continue;
            }
        }

        let dest_path = output_dir.join(&relative_path);

        // Do not follow pre-existing symlinks in the extraction tree.
        let mut current = output_dir.to_path_buf();
        for component in relative_path.parent().into_iter().flat_map(Path::components) {
            current.push(component);
            if let Ok(meta) = fs::symlink_metadata(&current) {
                if meta.file_type().is_symlink() || !meta.is_dir() {
                    return Err(CpkgError::InvalidFormat(format!(
                        "Unsafe extraction directory: {}", current.display()
                    )));
                }
            }
        }

        if dest_path.exists() && !flags.force && !flags.dry_run {
            return Err(CpkgError::AlreadyExists(
                dest_path.to_string_lossy().into_owned(),
            ));
        }

        if flags.dry_run {
            io::copy(&mut (&mut decoder).take(header.size), &mut io::sink())?;
            if flags.verbose && !flags.quiet {
                info(&format!("would unpack: {}", header.path));
            }
            extracted_count += 1;
            continue;
        }

        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut out_file = File::create(&dest_path)?;
        let mut limited_reader = (&mut decoder).take(header.size);
        let copied = io::copy(&mut limited_reader, &mut out_file)?;

        if copied != header.size {
            return Err(CpkgError::InvalidFormat(format!(
                "Unexpected end of file while unpacking {}",
                header.path
            )));
        }

        if !flags.ignore_perms {
            let perms = fs::Permissions::from_mode(header.mode);
            fs::set_permissions(&dest_path, perms)?;
        }

        if flags.verbose && !flags.quiet {
            info(&format!("unpacked: {}", header.path));
        }

        extracted_count += 1;
    }

    Ok(extracted_count)
}

#[cfg(test)]
mod tests {
    use super::safe_relative_path;

    #[test]
    fn accepts_normal_relative_paths() {
        assert!(safe_relative_path("usr/bin/cpkg").is_ok());
        assert!(safe_relative_path("share/../escape").is_err());
        assert!(safe_relative_path("../escape").is_err());
        assert!(safe_relative_path("/etc/passwd").is_err());
        assert!(safe_relative_path("C:\\Windows\\system.ini").is_err());
        assert!(safe_relative_path("dir\\file").is_err());
    }
}
