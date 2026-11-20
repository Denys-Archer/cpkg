use std::fmt;
use std::io;

#[derive(Debug)]
pub enum CpkgError {
    Io(io::Error),
    InvalidMagic,
    InvalidFormat(String),
    InvalidArgs(String),
    WalkDir(walkdir::Error),
    AlreadyExists(String),
    ChecksumMismatch(String),
    Internal(anyhow::Error),
}

impl fmt::Display for CpkgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CpkgError::Io(err) => write!(f, "I/O error: {}", err),
            CpkgError::InvalidMagic => write!(f, "Invalid file format: not a .cpkg archive"),
            CpkgError::InvalidFormat(msg) => write!(f, "Corrupted archive: {}", msg),
            CpkgError::InvalidArgs(msg) => write!(f, "Argument error: {}", msg),
            CpkgError::WalkDir(err) => write!(f, "Directory walk error: {}", err),
            CpkgError::AlreadyExists(path) => {
                write!(f, "Target already exists (use -f / --force): {}", path)
            }
            CpkgError::ChecksumMismatch(msg) => write!(f, "SHA-256 mismatch: {}", msg),
            CpkgError::Internal(err) => write!(f, "Internal error: {}", err),
        }
    }
}

impl std::error::Error for CpkgError {}

impl From<io::Error> for CpkgError {
    fn from(err: io::Error) -> Self {
        CpkgError::Io(err)
    }
}

impl From<walkdir::Error> for CpkgError {
    fn from(err: walkdir::Error) -> Self {
        CpkgError::WalkDir(err)
    }
}

impl From<anyhow::Error> for CpkgError {
    fn from(err: anyhow::Error) -> Self {
        CpkgError::Internal(err)
    }
}

pub type Result<T> = std::result::Result<T, CpkgError>;
