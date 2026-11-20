mod compress;
mod cpkg;
mod error;
mod keys;
mod note;
mod unpack;
mod warning;

use anyhow::Context;
use compress::pack_files;
use cpkg::{generate_sha256sum_lines, inspect_archive, verify_archive};
use indicatif::{ProgressBar, ProgressStyle};
use keys::{parse_args, Command};
use note::{load_note_from_file, save_note_to_file, ArchiveNote};
use regex::Regex;
use serde::Serialize;
use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::process;
use std::time::Duration;
use unpack::unpack_archive;

const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize)]
struct InfoPayload<'a> {
    file: String,
    version: u8,
    entries: usize,
    raw_size: u64,
    packed_size: u64,
    savings_percent: f64,
    sha256: &'a str,
    note: Option<&'a ArchiveNote>,
}

fn paint_blue(text: &str) -> String {
    if io::stdout().is_terminal() {
        format!("\x1b[1;34m{}\x1b[0m", text)
    } else {
        text.to_string()
    }
}

fn spinner(quiet: bool, msg: &str) -> ProgressBar {
    if quiet || !io::stdout().is_terminal() {
        return ProgressBar::hidden();
    }
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("\x1b[1;34m[{spinner}]\x1b[0m {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

fn print_version() {
    println!("{} {}", paint_blue("cpkg"), PKG_VERSION);
    println!("COPYRIGHT (DENYS LAPTEV)");
    println!("License: GPL-3.0 (GNU General Public License v3.0 or later)");	
}

fn print_help() {
    println!("{} {} - .cpkg archive utility", paint_blue("cpkg"), PKG_VERSION);
    println!();
    println!("{}", paint_blue("Commands:"));
    println!("  pack, c       <archive.cpkg> <target...>   Create archive");
    println!("  unpack, x     <archive.cpkg> [out_dir]     Extract archive");
    println!("  list, l, t    <archive.cpkg>               List archive contents");
    println!("  info, i       <archive.cpkg>               Show archive statistics");
    println!("  check, k      <archive.cpkg>               Verify archive & SHA-256");
    println!("  note, n       <archive.cpkg> [-o <file>]   Show or export embedded note");
    println!("  sha256sum     <target...>                  Generate sha256sum lines");
    println!("  version, v                                 Show version");
    println!("  help, h                                    Show this help");
    println!();
    println!("{}", paint_blue("Options:"));
    println!("  -h, --help                 Show help");
    println!("  -v, --version              Show version");
    println!("  -l, --level <1..22>        Zstd compression level (default: 9)");
    println!("  -o, --output <path>        Output directory, SHA256 or note file");
    println!("  -p, --prefix <path>        Unpack only entries starting with prefix");
    println!("  -e, --exclude <glob>       Skip paths matching glob pattern");
    println!("  --ext <ext>                Include only files with extension");
    println!("  --regex <pattern>          Filter entries by regular expression");
    println!("  --note <text>              Embed text note inside archive");
    println!("  --note-file <path>         Embed note read from file");
    println!("  --tag <name>               Note tag (default: release)");
    println!("  --sha256 <file>            Verify archive against SHA256 file");
    println!("  -s, --sort <name|size>     Sort entries in list command");
    println!("  --json                     Output JSON where supported");
    println!("  -f, --force                Overwrite existing archive or files");
    println!("  -n, --dry-run              Simulate without writing changes");
    println!("  --no-checksum              Disable zstd frame checksum");
    println!("  --ignore-perms             Do not preserve/apply Unix permissions");
    println!("  -V, --verbose              Verbose output");
    println!("  -q, --quiet                Suppress summary output");
}

fn resolve_note(
    note_text: Option<String>,
    note_file: Option<std::path::PathBuf>,
    tag: &str,
) -> Result<Option<ArchiveNote>, error::CpkgError> {
    let author = env::var("USER").unwrap_or_else(|_| "cpkg".to_string());
    if let Some(path) = note_file {
        return Ok(Some(load_note_from_file(&path, &author, tag)?));
    }
    if let Some(text) = note_text {
        return Ok(Some(ArchiveNote::new(author, tag, text)));
    }
    Ok(None)
}

fn run() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();

    let cmd = parse_args(&args).map_err(|e| anyhow::anyhow!("{}", e))?;

    match cmd {
        Command::Help => print_help(),
        Command::Version => print_version(),
        Command::Pack {
            archive,
            targets,
            level,
            excludes,
            extensions,
            regex_pattern,
            note_text,
            note_file,
            note_tag,
            flags,
        } => {
            let note = resolve_note(note_text, note_file, &note_tag)
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            let pb = spinner(flags.quiet, "Packing archive...");
            let stats = pack_files(
                &archive,
                &targets,
                level,
                &excludes,
                &extensions,
                regex_pattern.as_deref(),
                note.as_ref(),
                &flags,
            )
            .map_err(|e| anyhow::anyhow!("{}", e))?;
            pb.finish_and_clear();

            if flags.quiet {
                return Ok(());
            }
            let tag = paint_blue("PACK");
            if flags.dry_run {
                println!(
                    "[{tag}] Dry-run: {} files ({} B), {} skipped",
                    stats.files_count, stats.original_size, stats.skipped_count
                );
                return Ok(());
            }
            let saved = if stats.original_size > 0 {
                100.0 - (stats.compressed_size as f64 / stats.original_size as f64) * 100.0
            } else {
                0.0
            };
            println!(
                "[{tag}] {:?} <- {} files ({} skipped)",
                archive, stats.files_count, stats.skipped_count
            );
            println!(
                "[{tag}] Size: {} B -> {} B ({:.2}% saved, level {})",
                stats.original_size, stats.compressed_size, saved, level
            );
            println!("[{tag}] SHA-256: {}", stats.archive_sha256);
        }
        Command::Unpack {
            archive,
            output_dir,
            filter_prefix,
            flags,
        } => {
            let pb = spinner(flags.quiet, "Unpacking archive...");
            let count = unpack_archive(&archive, &output_dir, filter_prefix.as_deref(), &flags)
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            pb.finish_and_clear();

            if flags.quiet {
                return Ok(());
            }
            let tag = paint_blue("UNPACK");
            if flags.dry_run {
                println!("[{tag}] Dry-run: {} files from {:?}", count, archive);
            } else {
                println!("[{tag}] {} files -> {:?}", count, output_dir);
            }
        }
        Command::List {
            archive,
            sort_by,
            regex_pattern,
            verbose,
        } => {
            let compiled = match regex_pattern {
                Some(pat) => Some(Regex::new(&pat).context("Invalid list --regex pattern")?),
                None => None,
            };
            let pb = spinner(false, "Reading archive entries...");
            let summary =
                inspect_archive(&archive, sort_by).map_err(|e| anyhow::anyhow!("{}", e))?;
            pb.finish_and_clear();

            let tag = paint_blue("LIST");
            let filtered: Vec<_> = summary
                .entries
                .into_iter()
                .filter(|e| compiled.as_ref().is_none_or(|re| re.is_match(&e.path)))
                .collect();
            println!("[{tag}] {:?} ({} entries)", archive, filtered.len());
            for entry in filtered {
                if verbose {
                    println!(
                        "  {:04o}  {:>10} B  {}",
                        entry.mode & 0o7777,
                        entry.size,
                        entry.path
                    );
                } else {
                    println!("  {:>10} B  {}", entry.size, entry.path);
                }
            }
        }
        Command::Info { archive, json } => {
            let pb = spinner(json, "Inspecting archive...");
            let summary = inspect_archive(&archive, keys::SortBy::Name)
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            pb.finish_and_clear();

            let ratio = if summary.total_uncompressed > 0 {
                100.0
                    - (summary.compressed_size as f64 / summary.total_uncompressed as f64)
                        * 100.0
            } else {
                0.0
            };

            if json {
                let payload = InfoPayload {
                    file: archive.to_string_lossy().into_owned(),
                    version: summary.version,
                    entries: summary.entries.len(),
                    raw_size: summary.total_uncompressed,
                    packed_size: summary.compressed_size,
                    savings_percent: ratio,
                    sha256: &summary.archive_sha256,
                    note: summary.note.as_ref(),
                };
                println!("{}", serde_json::to_string_pretty(&payload)?);
                return Ok(());
            }

            let tag = paint_blue("INFO");
            println!("[{tag}] File:      {:?}", archive);
            println!("[{tag}] Version:   v{}", summary.version);
            println!("[{tag}] Entries:   {}", summary.entries.len());
            println!("[{tag}] Raw size:  {} B", summary.total_uncompressed);
            println!("[{tag}] Packed:    {} B", summary.compressed_size);
            println!("[{tag}] Savings:   {:.2}%", ratio);
            println!("[{tag}] SHA-256:   {}", summary.archive_sha256);
            if let Some(note) = summary.note {
                println!(
                    "[{tag}] Note:      [{}] ({}): {}",
                    note.tag, note.author, note.body
                );
            }
        }
        Command::Check {
            archive,
            sha256_file,
        } => {
            let pb = spinner(false, "Verifying archive...");
            let summary = verify_archive(&archive, sha256_file.as_deref())
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            pb.finish_and_clear();

            let tag = paint_blue("CHECK");
            println!(
                "[{tag}] OK: {:?} ({} entries, {} B uncompressed)",
                archive,
                summary.entries.len(),
                summary.total_uncompressed
            );
            println!("[{tag}] SHA-256: {}", summary.archive_sha256);
        }
        Command::Sha256Sum {
            targets,
            output_file,
        } => {
            let pb = spinner(false, "Computing SHA-256 digests...");
            let lines =
                generate_sha256sum_lines(&targets).map_err(|e| anyhow::anyhow!("{}", e))?;
            pb.finish_and_clear();

            let mut out = lines.join("\n");
            if !out.is_empty() {
                out.push('\n');
            }
            if let Some(dest) = output_file {
                fs::write(&dest, out)
                    .with_context(|| format!("Failed to write {:?}", dest))?;
                println!(
                    "[{}] Wrote {} entries to {:?}",
                    paint_blue("SHA256"),
                    lines.len(),
                    dest
                );
            } else {
                print!("{}", out);
            }
        }
        Command::Note {
            archive,
            output_file,
            json,
        } => {
            let pb = spinner(json, "Reading archive note...");
            let summary = inspect_archive(&archive, keys::SortBy::Name)
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            pb.finish_and_clear();

            match summary.note {
                Some(note) => {
                    if let Some(dest) = output_file {
                        save_note_to_file(&note, &dest, json)
                            .map_err(|e| anyhow::anyhow!("{}", e))?;
                        println!("[{}] Saved note to {:?}", paint_blue("NOTE"), dest);
                    } else if json {
                        println!("{}", note.to_json().map_err(|e| anyhow::anyhow!("{}", e))?);
                    } else {
                        let tag = paint_blue("NOTE");
                        println!("[{tag}] Tag:    {}", note.tag);
                        println!("[{tag}] Author: {}", note.author);
                        println!("[{tag}] Body:\n{}", note.body);
                    }
                }
                None => {
                    println!(
                        "[{}] No embedded note in {:?}",
                        paint_blue("NOTE"),
                        archive
                    );
                }
            }
        }
    }

    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {:#}", err);
        process::exit(1);
    }
}
