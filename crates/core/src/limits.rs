// SPDX-License-Identifier: Apache-2.0
//! Hard bounds applied to every inspection phase.
//!
//! The worker never accepts limit overrides from a preview request; these
//! values are compiled in. Tests may construct smaller limits to exercise the
//! truncation paths.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Limits {
    /// Largest file (or spooled stream) the worker will accept.
    pub max_file_bytes: u64,
    /// Largest Thrift footer the core will hand to the Parquet reader.
    pub max_footer_bytes: u64,
    /// Row groups listed in the summary (the total count is still reported).
    pub max_row_groups: usize,
    /// Schema leaves listed in the summary (the total count is still reported).
    pub max_columns: usize,
    /// Rows a single sample request may return.
    pub max_sample_rows: usize,
    /// Largest row offset a virtualized sample request may use.
    pub max_sample_offset: u64,
    /// Decoded Arrow bytes a single sample request may materialize.
    pub max_sample_decoded_bytes: u64,
    /// Compressed bytes of row groups considered for one sample request.
    pub max_sample_compressed_bytes: u64,
    /// Wall-clock budget for one sample request.
    pub max_sample_millis: u64,
    /// Top-level columns materialized for one sample request.
    pub max_sample_columns: usize,
    /// Rows a full scan may decode.
    pub max_scan_rows: u64,
    /// Decoded Arrow bytes a full scan may materialize.
    pub max_scan_decoded_bytes: u64,
    /// Compressed bytes a full scan may read.
    pub max_scan_compressed_bytes: u64,
    /// Wall-clock budget for a full scan.
    pub max_scan_millis: u64,
    /// Characters rendered for one cell before truncation.
    pub max_cell_chars: usize,
    /// Nested list entries, map entries or struct fields shown per value.
    pub max_nested_items: usize,
    /// Maximum recursion depth when rendering nested values.
    pub max_nested_depth: usize,
    /// Rows decoded for one market diagnostic request.
    pub max_diagnostic_rows: usize,
    /// Wall-clock budget for one market diagnostic request.
    pub max_diagnostic_millis: u64,
    /// Largest reference-price CSV accepted.
    pub max_baseline_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_file_bytes: 16 * 1024 * 1024 * 1024,
            max_footer_bytes: 16 * 1024 * 1024,
            max_row_groups: 10_000,
            max_columns: 2_000,
            max_sample_rows: 500,
            max_sample_offset: 1_000_000,
            max_sample_decoded_bytes: 16 * 1024 * 1024,
            max_sample_compressed_bytes: 128 * 1024 * 1024,
            max_sample_millis: 10_000,
            max_sample_columns: 200,
            max_scan_rows: 20_000_000,
            max_scan_decoded_bytes: 1024 * 1024 * 1024,
            max_scan_compressed_bytes: 2 * 1024 * 1024 * 1024,
            max_scan_millis: 30_000,
            max_cell_chars: 512,
            max_nested_items: 20,
            max_nested_depth: 3,
            max_diagnostic_rows: 50_000,
            max_diagnostic_millis: 10_000,
            max_baseline_bytes: 16 * 1024 * 1024,
        }
    }
}

impl Limits {
    /// Return the stricter of two limit sets, field by field.
    pub fn stricter(&self, other: &Limits) -> Limits {
        Limits {
            max_file_bytes: self.max_file_bytes.min(other.max_file_bytes),
            max_footer_bytes: self.max_footer_bytes.min(other.max_footer_bytes),
            max_row_groups: self.max_row_groups.min(other.max_row_groups),
            max_columns: self.max_columns.min(other.max_columns),
            max_sample_rows: self.max_sample_rows.min(other.max_sample_rows),
            max_sample_offset: self.max_sample_offset.min(other.max_sample_offset),
            max_sample_decoded_bytes: self
                .max_sample_decoded_bytes
                .min(other.max_sample_decoded_bytes),
            max_sample_compressed_bytes: self
                .max_sample_compressed_bytes
                .min(other.max_sample_compressed_bytes),
            max_sample_millis: self.max_sample_millis.min(other.max_sample_millis),
            max_sample_columns: self.max_sample_columns.min(other.max_sample_columns),
            max_scan_rows: self.max_scan_rows.min(other.max_scan_rows),
            max_scan_decoded_bytes: self
                .max_scan_decoded_bytes
                .min(other.max_scan_decoded_bytes),
            max_scan_compressed_bytes: self
                .max_scan_compressed_bytes
                .min(other.max_scan_compressed_bytes),
            max_scan_millis: self.max_scan_millis.min(other.max_scan_millis),
            max_cell_chars: self.max_cell_chars.min(other.max_cell_chars),
            max_nested_items: self.max_nested_items.min(other.max_nested_items),
            max_nested_depth: self.max_nested_depth.min(other.max_nested_depth),
            max_diagnostic_rows: self.max_diagnostic_rows.min(other.max_diagnostic_rows),
            max_diagnostic_millis: self
                .max_diagnostic_millis
                .min(other.max_diagnostic_millis),
            max_baseline_bytes: self.max_baseline_bytes.min(other.max_baseline_bytes),
        }
    }

    /// Reject configuration that would defeat the point of a bound.
    pub fn validate(&self) -> crate::Result<()> {
        let invalid = self.max_file_bytes < 12
            || self.max_footer_bytes < 8
            || self.max_sample_rows == 0
            || self.max_sample_decoded_bytes == 0
            || self.max_sample_millis == 0
            || self.max_sample_columns == 0
            || self.max_scan_rows == 0
            || self.max_scan_decoded_bytes == 0
            || self.max_scan_compressed_bytes == 0
            || self.max_cell_chars == 0
            || self.max_nested_items == 0
            || self.max_nested_depth == 0
            || self.max_diagnostic_rows == 0
            || self.max_diagnostic_millis == 0;
        if invalid {
            return Err(crate::PreviewError::Limit(
                "invalid limit configuration".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        Limits::default().validate().unwrap();
    }

    #[test]
    fn stricter_takes_field_wise_minimum() {
        let a = Limits::default();
        let mut b = Limits::default();
        b.max_sample_rows = 7;
        b.max_scan_millis = 1;
        let c = a.stricter(&b);
        assert_eq!(c.max_sample_rows, 7);
        assert_eq!(c.max_scan_millis, 1);
        assert_eq!(c.max_file_bytes, a.max_file_bytes);
    }

    #[test]
    fn zero_budget_is_rejected() {
        let mut limits = Limits::default();
        limits.max_sample_rows = 0;
        assert!(limits.validate().is_err());
    }
}
