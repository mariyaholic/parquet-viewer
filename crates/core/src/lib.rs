// SPDX-License-Identifier: Apache-2.0
//! Read-only Parquet inspection core for the Explorer preview handler.
//!
//! The core is platform independent. The Windows COM handler lives in a
//! separate crate so this crate builds and tests on Linux.

pub mod about;
pub mod baseline;
pub mod date_bounds;
pub mod display;
pub mod error;
pub mod footer;
pub mod limits;
pub mod market;
pub mod report;
pub mod sample;
pub mod scan;
pub mod session;
pub mod settings;
pub mod source;
pub mod summary;
pub mod time;

pub use date_bounds::{CoverageStatus, DateBounds, DateColumn, DateKind};
pub use error::{PreviewError, Result};
pub use footer::{parse_footer, FooterInfo};
pub use limits::Limits;
pub use report::{PreviewOptions, PreviewReport};
pub use sample::{CellValue, SampleReport, SampleStatus};
pub use scan::{CancelToken, ScanReport, ScanStatus};
pub use session::Session;
pub use settings::Settings;
pub use source::{BoxSource, ReadSeek, SharedSource};
pub use summary::{
    CountSource, FileSummary, RowCount, RowGroupInfo, RowGroupsSummary, SchemaField, SchemaSummary,
    Summary,
};
