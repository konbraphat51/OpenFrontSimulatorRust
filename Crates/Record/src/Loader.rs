// SPDX-License-Identifier: AGPL-3.0-only
//! Loading of records from `.json` and `.json.gz` files or bytes.

use crate::Records::GameRecord;
use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::io::Read;
use std::path::Path;

impl GameRecord {
    /// Parses a record from JSON bytes, transparently gunzipping when the gzip magic is present.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.starts_with(&[0x1f, 0x8b]) {
            let mut text = Vec::new();
            GzDecoder::new(bytes)
                .read_to_end(&mut text)
                .context("gunzip record")?;
            Ok(serde_json::from_slice(&text)?)
        } else {
            Ok(serde_json::from_slice(bytes)?)
        }
    }

    /// Loads a record from a `.json` or `.json.gz` file (sparse turns, as stored).
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        Self::from_bytes(&bytes).with_context(|| format!("parsing {}", path.display()))
    }

    /// Loads a record and expands its sparse turns, as the replay path does.
    pub fn load_decompressed(path: &Path) -> Result<Self> {
        let mut record = Self::load(path)?;
        record.decompress();
        Ok(record)
    }
}
