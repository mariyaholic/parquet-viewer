// SPDX-License-Identifier: Apache-2.0
//! Error type shared by the read-only inspection core.

use std::io;

/// Every failure surfaced by the core. Messages avoid file paths and must stay
/// safe to display in a preview pane or log.
#[derive(Debug, thiserror::Error)]
pub enum PreviewError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("not a Parquet file: {0}")]
    NotParquet(String),

    #[error("encrypted Parquet files are not supported in v1")]
    Encrypted,

    #[error("file is too large: {size} bytes exceeds the {limit}-byte limit")]
    FileTooLarge { size: u64, limit: u64 },

    #[error("Parquet footer metadata is too large: {size} bytes exceeds the {limit}-byte limit")]
    FooterTooLarge { size: u64, limit: u64 },

    #[error("malformed Parquet footer: {0}")]
    MalformedFooter(String),

    #[error("Parquet reader error: {0}")]
    Parquet(#[from] parquet::errors::ParquetError),

    #[error("Arrow reader error: {0}")]
    Arrow(#[from] arrow_schema::ArrowError),

    #[error("inspection limit reached: {0}")]
    Limit(String),

    #[error("invalid market profile: {0}")]
    Profile(String),

    #[error("reference-price baseline error: {0}")]
    Baseline(String),

    #[error("the supplied stream could not be read: {0}")]
    Stream(String),

    #[error("inspection was cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, PreviewError>;
