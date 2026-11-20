use crate::error::{CpkgError, Result};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Name,
    Size,
}

#[derive(Debug, Clone, Default)]
pub struct GlobalFlags {
    pub force: bool,
    pub dry_run: bool,
    pub verbose: bool,
    pub quiet: bool,
    pub no_checksum: bool,
    pub ignore_perms: bool,
    pub json: bool,
}

#[derive(Debug)]
pub enum Command {
    Help,
    Version,
    Pack {
        archive: PathBuf,
        targets: Vec<PathBuf>,
        level: i32,
        excludes: Vec<String>,
        extensions: Vec<String>,
        regex_pattern: Option<String>,
        note_text: Option<String>,
        note_file: Option<PathBuf>,
        note_tag: String,
        flags: GlobalFlags,
    },
    Unpack {
        archive: PathBuf,
        output_dir: PathBuf,
        filter_prefix: Option<String>,
        flags: GlobalFlags,
    },
    List {
        archive: PathBuf,
        sort_by: SortBy,
        regex_pattern: Option<String>,
        verbose: bool,
    },
    Info {
        archive: PathBuf,
        json: bool,
    },
    Check {
        archive: PathBuf,
        sha256_file: Option<PathBuf>,
    },
    Sha256Sum {
        targets: Vec<PathBuf>,
        output_file: Option<PathBuf>,
    },
    Note {
        archive: PathBuf,
        output_file: Option<PathBuf>,
        json: bool,
    },
}

fn parse_level(val: &str) -> Result<i32> {
    let lvl: i32 = val
        .parse()
        .map_err(|_| CpkgError::InvalidArgs("Level must be an integer between 1 and 22".into()))?;
    if !(1..=22).contains(&lvl) {
        return Err(CpkgError::InvalidArgs(
            "Compression level must be in range 1..=22".into(),
        ));
    }
    Ok(lvl)
}

pub fn parse_args(args: &[String]) -> Result<Command> {
    if args.len() < 2 {
        return Ok(Command::Help);
    }

    match args[1].as_str() {
        "-h" | "--help" | "help" => return Ok(Command::Help),
        "-v" | "--version" | "version" => return Ok(Command::Version),
        _ => {}
    }

    let action = args[1].as_str();
    let mut flags = GlobalFlags::default();
    let mut positionals: Vec<String> = Vec::new();
    let mut level: i32 = 9;
    let mut excludes: Vec<String> = Vec::new();
    let mut extensions: Vec<String> = Vec::new();
    let mut output_override: Option<PathBuf> = None;
    let mut filter_prefix: Option<String> = None;
    let mut sha256_file: Option<PathBuf> = None;
    let mut regex_pattern: Option<String> = None;
    let mut note_text: Option<String> = None;
    let mut note_file: Option<PathBuf> = None;
    let mut note_tag: String = "release".to_string();
    let mut sort_by = SortBy::Name;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-v" | "--version" => return Ok(Command::Version),
            "-f" | "--force" => flags.force = true,
            "-n" | "--dry-run" => flags.dry_run = true,
            "-V" | "--verbose" => flags.verbose = true,
            "-q" | "--quiet" => flags.quiet = true,
            "--no-checksum" => flags.no_checksum = true,
            "--ignore-perms" => flags.ignore_perms = true,
            "--json" => flags.json = true,
            "-l" | "--level" => {
                let val = args
                    .get(i + 1)
                    .ok_or_else(|| CpkgError::InvalidArgs("Missing value after -l/--level".into()))?;
                level = parse_level(val)?;
                i += 1;
            }
            "-e" | "--exclude" => {
                let val = args.get(i + 1).ok_or_else(|| {
                    CpkgError::InvalidArgs("Missing pattern after -e/--exclude".into())
                })?;
                excludes.push(val.clone());
                i += 1;
            }
            "--ext" => {
                let val = args.get(i + 1).ok_or_else(|| {
                    CpkgError::InvalidArgs("Missing extension after --ext".into())
                })?;
                extensions.push(val.trim_start_matches('.').to_lowercase());
                i += 1;
            }
            "--regex" => {
                let val = args.get(i + 1).ok_or_else(|| {
                    CpkgError::InvalidArgs("Missing pattern after --regex".into())
                })?;
                regex_pattern = Some(val.clone());
                i += 1;
            }
            "-o" | "--output" => {
                let val = args.get(i + 1).ok_or_else(|| {
                    CpkgError::InvalidArgs("Missing path after -o/--output".into())
                })?;
                output_override = Some(PathBuf::from(val));
                i += 1;
            }
            "-p" | "--prefix" => {
                let val = args.get(i + 1).ok_or_else(|| {
                    CpkgError::InvalidArgs("Missing prefix after -p/--prefix".into())
                })?;
                filter_prefix = Some(val.clone());
                i += 1;
            }
            "--sha256" => {
                let val = args.get(i + 1).ok_or_else(|| {
                    CpkgError::InvalidArgs("Missing path after --sha256".into())
                })?;
                sha256_file = Some(PathBuf::from(val));
                i += 1;
            }
            "--note" => {
                let val = args.get(i + 1).ok_or_else(|| {
                    CpkgError::InvalidArgs("Missing text after --note".into())
                })?;
                note_text = Some(val.clone());
                i += 1;
            }
            "--note-file" => {
                let val = args.get(i + 1).ok_or_else(|| {
                    CpkgError::InvalidArgs("Missing path after --note-file".into())
                })?;
                note_file = Some(PathBuf::from(val));
                i += 1;
            }
            "--tag" => {
                let val = args.get(i + 1).ok_or_else(|| {
                    CpkgError::InvalidArgs("Missing value after --tag".into())
                })?;
                note_tag = val.clone();
                i += 1;
            }
            "-s" | "--sort" => {
                let val = args.get(i + 1).ok_or_else(|| {
                    CpkgError::InvalidArgs("Missing sort mode after -s/--sort".into())
                })?;
                sort_by = match val.as_str() {
                    "name" => SortBy::Name,
                    "size" => SortBy::Size,
                    other => {
                        return Err(CpkgError::InvalidArgs(format!(
                            "Unknown sort mode '{}', use 'name' or 'size'",
                            other
                        )))
                    }
                };
                i += 1;
            }
            arg if arg.starts_with('-') => {
                return Err(CpkgError::InvalidArgs(format!("Unknown flag: {}", arg)));
            }
            arg => positionals.push(arg.to_string()),
        }
        i += 1;
    }

    match action {
        "c" | "pack" => {
            if positionals.len() < 2 {
                return Err(CpkgError::InvalidArgs(
                    "Usage: cpkg pack <archive.cpkg> <target...> [flags]".into(),
                ));
            }
            let archive = PathBuf::from(&positionals[0]);
            let targets = positionals[1..].iter().map(PathBuf::from).collect();
            Ok(Command::Pack {
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
            })
        }
        "x" | "unpack" => {
            if positionals.is_empty() {
                return Err(CpkgError::InvalidArgs(
                    "Usage: cpkg unpack <archive.cpkg> [output_dir] [flags]".into(),
                ));
            }
            let archive = PathBuf::from(&positionals[0]);
            let output_dir = output_override
                .or_else(|| positionals.get(1).map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from("."));
            Ok(Command::Unpack {
                archive,
                output_dir,
                filter_prefix,
                flags,
            })
        }
        "l" | "list" | "t" => {
            if positionals.is_empty() {
                return Err(CpkgError::InvalidArgs(
                    "Usage: cpkg list <archive.cpkg> [-s name|size] [--regex <pat>] [-V]".into(),
                ));
            }
            Ok(Command::List {
                archive: PathBuf::from(&positionals[0]),
                sort_by,
                regex_pattern,
                verbose: flags.verbose,
            })
        }
        "i" | "info" => {
            if positionals.is_empty() {
                return Err(CpkgError::InvalidArgs(
                    "Usage: cpkg info <archive.cpkg> [--json]".into(),
                ));
            }
            Ok(Command::Info {
                archive: PathBuf::from(&positionals[0]),
                json: flags.json,
            })
        }
        "k" | "check" => {
            if positionals.is_empty() {
                return Err(CpkgError::InvalidArgs(
                    "Usage: cpkg check <archive.cpkg> [--sha256 <SHA256_file>]".into(),
                ));
            }
            Ok(Command::Check {
                archive: PathBuf::from(&positionals[0]),
                sha256_file,
            })
        }
        "sum" | "sha256sum" => {
            if positionals.is_empty() {
                return Err(CpkgError::InvalidArgs(
                    "Usage: cpkg sha256sum <file_or_dir...> [-o <SHA256_file>]".into(),
                ));
            }
            let targets = positionals.iter().map(PathBuf::from).collect();
            Ok(Command::Sha256Sum {
                targets,
                output_file: output_override,
            })
        }
        "n" | "note" => {
            if positionals.is_empty() {
                return Err(CpkgError::InvalidArgs(
                    "Usage: cpkg note <archive.cpkg> [-o <file>] [--json]".into(),
                ));
            }
            Ok(Command::Note {
                archive: PathBuf::from(&positionals[0]),
                output_file: output_override,
                json: flags.json,
            })
        }
        other => Err(CpkgError::InvalidArgs(format!(
            "Unknown command: {}",
            other
        ))),
    }
}
