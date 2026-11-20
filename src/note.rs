use crate::error::{CpkgError, Result};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const MAX_NOTE_BYTES: usize = 65535;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveNote {
    pub author: String,
    pub tag: String,
    pub body: String,
}

impl ArchiveNote {
    pub fn new(author: impl Into<String>, tag: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            author: author.into(),
            tag: tag.into(),
            body: body.into(),
        }
    }

    // Encode note into a compact framed buffer:
    // [author_len: u16][author][tag_len: u16][tag][body_len: u32][body]
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let a_bytes = self.author.as_bytes();
        let t_bytes = self.tag.as_bytes();
        let b_bytes = self.body.as_bytes();

        let a_len = u16::try_from(a_bytes.len())
            .map_err(|_| CpkgError::InvalidFormat("Note author exceeds 65535 bytes".into()))?;
        let t_len = u16::try_from(t_bytes.len())
            .map_err(|_| CpkgError::InvalidFormat("Note tag exceeds 65535 bytes".into()))?;
        let b_len = u32::try_from(b_bytes.len())
            .map_err(|_| CpkgError::InvalidFormat("Note body exceeds u32 max".into()))?;

        if b_bytes.len() > MAX_NOTE_BYTES {
            return Err(CpkgError::InvalidFormat(format!(
                "Note exceeds maximum size of {} bytes",
                MAX_NOTE_BYTES
            )));
        }

        let mut buf = Vec::with_capacity(2 + a_bytes.len() + 2 + t_bytes.len() + 4 + b_bytes.len());
        buf.extend_from_slice(&a_len.to_le_bytes());
        buf.extend_from_slice(a_bytes);
        buf.extend_from_slice(&t_len.to_le_bytes());
        buf.extend_from_slice(t_bytes);
        buf.extend_from_slice(&b_len.to_le_bytes());
        buf.extend_from_slice(b_bytes);
        Ok(buf)
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let mut pos = 0usize;

        let read_u16 = |buf: &[u8], p: &mut usize| -> Result<usize> {
            if *p + 2 > buf.len() {
                return Err(CpkgError::InvalidFormat("Truncated note length field".into()));
            }
            let val = u16::from_le_bytes([buf[*p], buf[*p + 1]]) as usize;
            *p += 2;
            Ok(val)
        };

        let read_u32 = |buf: &[u8], p: &mut usize| -> Result<usize> {
            if *p + 4 > buf.len() {
                return Err(CpkgError::InvalidFormat("Truncated note body length".into()));
            }
            let val = u32::from_le_bytes([buf[*p], buf[*p + 1], buf[*p + 2], buf[*p + 3]]) as usize;
            *p += 4;
            Ok(val)
        };

        let a_len = read_u16(data, &mut pos)?;
        if pos + a_len > data.len() {
            return Err(CpkgError::InvalidFormat("Truncated note author".into()));
        }
        let author = String::from_utf8(data[pos..pos + a_len].to_vec())
            .map_err(|_| CpkgError::InvalidFormat("Invalid UTF-8 in note author".into()))?;
        pos += a_len;

        let t_len = read_u16(data, &mut pos)?;
        if pos + t_len > data.len() {
            return Err(CpkgError::InvalidFormat("Truncated note tag".into()));
        }
        let tag = String::from_utf8(data[pos..pos + t_len].to_vec())
            .map_err(|_| CpkgError::InvalidFormat("Invalid UTF-8 in note tag".into()))?;
        pos += t_len;

        let b_len = read_u32(data, &mut pos)?;
        if pos + b_len > data.len() {
            return Err(CpkgError::InvalidFormat("Truncated note body".into()));
        }
        let body = String::from_utf8(data[pos..pos + b_len].to_vec())
            .map_err(|_| CpkgError::InvalidFormat("Invalid UTF-8 in note body".into()))?;

        Ok(Self { author, tag, body })
    }

    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .context("Failed to serialize note as JSON")
            .map_err(CpkgError::from)
    }
}

pub fn load_note_from_file(path: &Path, author: &str, tag: &str) -> Result<ArchiveNote> {
    let body = fs::read_to_string(path)
        .with_context(|| format!("Failed to read note file {:?}", path))?;
    Ok(ArchiveNote::new(author, tag, body.trim_end()))
}

pub fn save_note_to_file(note: &ArchiveNote, path: &Path, as_json: bool) -> Result<()> {
    let content = if as_json {
        format!("{}\n", note.to_json()?)
    } else {
        format!(
            "# tag: {}\n# author: {}\n{}\n",
            note.tag, note.author, note.body
        )
    };
    fs::write(path, content)
        .with_context(|| format!("Failed to write note file {:?}", path))?;
    Ok(())
}
