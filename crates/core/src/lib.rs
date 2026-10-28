// SPDX-License-Identifier: Apache-2.0
//! Read-only Parquet inspection core for the Explorer preview handler.
//!
//! The core is platform independent. The Windows COM handler lives in a
//! separate crate so this crate builds and tests on Linux.

pub mod error;
pub mod display;
pub mod date_bounds;
pub mod footer;
pub mod limits;
pub mod sample;
pub mod session;
pub mod source;
pub mod summary;
pub mod time;

pub use date_bounds::{CoverageStatus, DateBounds, DateColumn, DateKind};
pub use error::{PreviewError, Result};
pub use footer::{parse_footer, FooterInfo};
pub use limits::Limits;
pub use sample::{CellValue, SampleReport, SampleStatus};
pub use session::Session;
pub use source::{BoxSource, ReadSeek, SharedSource};
pub use summary::{
    CountSource, FileSummary, RowCount, RowGroupInfo, RowGroupsSummary, SchemaField, SchemaSummary,
    Summary,
};
