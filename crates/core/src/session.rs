// SPDX-License-Identifier: Apache-2.0
//! Open a bounded, read-only inspection session.

use std::path::Path;
use std::sync::Arc;

use parquet::file::metadata::ParquetMetaData;
use parquet::file::reader::{FileReader, SerializedFileReader};

use crate::footer::{parse_footer, FooterInfo};
use crate::summary::{summarize, Summary};
use crate::{BoxSource, Limits, PreviewError, Result, SharedSource};

pub struct Session {
    source: SharedSource,
    metadata: Arc<ParquetMetaData>,
    footer: FooterInfo,
    limits: Limits,
    summary: Summary,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("file_bytes", &self.footer.file_bytes)
            .field("footer_bytes", &self.footer.footer_bytes)
            .field("row_groups", &self.summary.row_groups.total)
            .finish()
    }
}

impl Session {
    /// Open from a caller-supplied seekable stream. The stream is never
    /// written to, and no filesystem path is assumed.
    pub fn open(source: BoxSource, limits: &Limits) -> Result<Self> {
        limits.validate()?;
        let mut shared = SharedSource::new(source)?;
        if shared.len() > limits.max_file_bytes {
            return Err(PreviewError::FileTooLarge {
                size: shared.len(),
                limit: limits.max_file_bytes,
            });
        }
        let file_bytes = shared.len();
        let footer = parse_footer(&mut shared, file_bytes, limits.max_footer_bytes)?;
        let reader = SerializedFileReader::new(shared.clone())?;
        let metadata = Arc::new(reader.metadata().clone());
        let summary = summarize(&metadata, footer.file_bytes, footer.footer_bytes, limits);
        Ok(Self {
            source: shared,
            metadata,
            footer,
            limits: limits.clone(),
            summary,
        })
    }

    /// Convenience for the headless tool and tests. Opens read-only.
    pub fn open_path(path: impl AsRef<Path>, limits: &Limits) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        Self::open(Box::new(file), limits)
    }

    pub fn summary(&self) -> &Summary {
        &self.summary
    }

    pub(crate) fn source(&self) -> &SharedSource {
        &self.source
    }

    pub(crate) fn limits(&self) -> &Limits {
        &self.limits
    }

    pub fn metadata(&self) -> &Arc<ParquetMetaData> {
        &self.metadata
    }

    pub fn footer(&self) -> &FooterInfo {
        &self.footer
    }
}
