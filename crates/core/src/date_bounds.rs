// SPDX-License-Identifier: Apache-2.0
//! Transparent date/timestamp bounds coverage.
//!
//! Bounds are only labelled complete when every row group carries trustworthy,
//! compatible statistics. Missing, truncated or inconsistent statistics make
//! the result partial or unavailable. A complete decoded scan can replace the
//! statistical bounds with exact observed values.

use parquet::file::metadata::{ParquetMetaData, RowGroupMetaData};
use parquet::file::statistics::Statistics;
use parquet::schema::types::Type as SchemaType;
use serde::{Deserialize, Serialize};

use crate::time::{format_date, format_timestamp, TimeUnit};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DateKind {
    Date,
    Timestamp {
        unit: TimeUnit,
        adjusted_to_utc: bool,
    },
}

impl DateKind {
    pub fn label(&self) -> &'static str {
        match self {
            DateKind::Date => "date",
            DateKind::Timestamp { .. } => "timestamp",
        }
    }

    pub fn format(&self, value: i64) -> String {
        match self {
            DateKind::Date => format_date(value),
            DateKind::Timestamp {
                unit,
                adjusted_to_utc,
            } => {
                let mut text = format_timestamp(value, *unit, *adjusted_to_utc);
                if !adjusted_to_utc {
                    text.push_str(" (unadjusted)");
                }
                text
            }
        }
    }

    fn range_from_stats(&self, stats: &Statistics) -> Option<(Option<i64>, Option<i64>)> {
        match (self, stats) {
            (DateKind::Date, Statistics::Int32(s)) => Some((
                s.min_opt().copied().map(i64::from),
                s.max_opt().copied().map(i64::from),
            )),
            (DateKind::Timestamp { .. }, Statistics::Int64(s)) => {
                Some((s.min_opt().copied(), s.max_opt().copied()))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateColumn {
    pub path: String,
    pub kind: DateKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageStatus {
    Complete,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateBounds {
    pub column: String,
    pub kind: String,
    pub status: CoverageStatus,
    pub min: Option<String>,
    pub max: Option<String>,
    pub covered_row_groups: usize,
    pub total_row_groups: usize,
    pub rows_covered: u64,
    pub total_rows: u64,
    pub missing_stats_row_groups: usize,
    pub untrusted_stats_row_groups: usize,
    pub verified_by_scan: bool,
    pub reason: String,
}

/// Top-level primitive date/timestamp columns, in schema order.
pub fn top_level_date_columns(root: &SchemaType) -> Vec<DateColumn> {
    root.get_fields()
        .iter()
        .filter(|field| field.is_primitive())
        .filter_map(|field| {
            crate::summary::date_kind_of(field).map(|kind| DateColumn {
                path: field.name().to_string(),
                kind,
            })
        })
        .collect()
}

/// Derive bounds from row-group statistics.
pub fn statistical_date_bounds(metadata: &ParquetMetaData) -> Vec<DateBounds> {
    let root = metadata.file_metadata().schema_descr().root_schema();
    let columns = top_level_date_columns(root);
    let total_row_groups = metadata.row_groups().len();
    let total_rows = metadata
        .row_groups()
        .iter()
        .map(|g| u64::try_from(g.num_rows()).unwrap_or(0))
        .sum::<u64>();

    columns
        .into_iter()
        .map(|column| {
            evaluate_column(
                &column,
                metadata.row_groups(),
                total_row_groups,
                total_rows,
            )
        })
        .collect()
}

fn evaluate_column(
    column: &DateColumn,
    groups: &[RowGroupMetaData],
    total_row_groups: usize,
    total_rows: u64,
) -> DateBounds {
    let mut covered_groups = 0usize;
    let mut covered_rows = 0u64;
    let mut missing = 0usize;
    let mut untrusted = 0usize;
    let mut any_values = false;
    let mut min: Option<i64> = None;
    let mut max: Option<i64> = None;

    for group in groups {
        let rows = u64::try_from(group.num_rows()).unwrap_or(0);
        let chunk = group
            .columns()
            .iter()
            .find(|c| c.column_path().string() == column.path);
        let Some(chunk) = chunk else {
            missing += 1;
            continue;
        };
        let Some(stats) = chunk.statistics() else {
            missing += 1;
            continue;
        };
        let nulls = stats.null_count_opt();
        let all_null = nulls == Some(rows) && rows > 0;
        if all_null {
            covered_groups += 1;
            covered_rows = covered_rows.saturating_add(rows);
            continue;
        }
        let Some((group_min, group_max)) = column.kind.range_from_stats(stats) else {
            // INT96 timestamps and other physical encodings cannot be compared
            // reliably here.
            untrusted += 1;
            continue;
        };
        let (Some(group_min), Some(group_max)) = (group_min, group_max) else {
            missing += 1;
            continue;
        };
        let trustworthy = stats.min_is_exact()
            && stats.max_is_exact()
            && !stats.is_min_max_deprecated()
            && group_min <= group_max;
        if !trustworthy {
            untrusted += 1;
            continue;
        }
        covered_groups += 1;
        covered_rows = covered_rows.saturating_add(rows);
        any_values = true;
        min = Some(min.map_or(group_min, |current| current.min(group_min)));
        max = Some(max.map_or(group_max, |current| current.max(group_max)));
    }

    let (status, reason) = if total_row_groups == 0 || total_rows == 0 {
        (
            CoverageStatus::Unavailable,
            "no rows: the file has no row groups with data".to_string(),
        )
    } else if covered_groups == total_row_groups {
        (
            CoverageStatus::Complete,
            if any_values {
                "all row groups have trustworthy, compatible statistics".to_string()
            } else {
                "all rows are null in this column".to_string()
            },
        )
    } else if covered_groups == 0 {
        (
            CoverageStatus::Unavailable,
            format!(
                "no trustworthy statistics: {missing} row group(s) lack bounds, {untrusted} have unusable bounds"
            ),
        )
    } else {
        (
            CoverageStatus::Partial,
            format!(
                "coverage is partial: {covered_groups} of {total_row_groups} row groups usable ({missing} missing, {untrusted} unusable)"
            ),
        )
    };

    DateBounds {
        column: column.path.clone(),
        kind: column.kind.label().to_string(),
        status,
        min: min.map(|v| column.kind.format(v)),
        max: max.map(|v| column.kind.format(v)),
        covered_row_groups: covered_groups,
        total_row_groups,
        rows_covered: covered_rows,
        total_rows,
        missing_stats_row_groups: missing,
        untrusted_stats_row_groups: untrusted,
        verified_by_scan: false,
        reason,
    }
}

/// Build an exact bounds entry from a complete decoded scan.
pub fn exact_bounds(
    column: &DateColumn,
    min: Option<i64>,
    max: Option<i64>,
    total_row_groups: usize,
    total_rows: u64,
) -> DateBounds {
    DateBounds {
        column: column.path.clone(),
        kind: column.kind.label().to_string(),
        status: CoverageStatus::Complete,
        min: min.map(|v| column.kind.format(v)),
        max: max.map(|v| column.kind.format(v)),
        covered_row_groups: total_row_groups,
        total_row_groups,
        rows_covered: total_rows,
        total_rows,
        missing_stats_row_groups: 0,
        untrusted_stats_row_groups: 0,
        verified_by_scan: true,
        reason: if min.is_some() {
            "exact bounds from a complete decoded scan".to_string()
        } else {
            "all rows are null in this column (complete decoded scan)".to_string()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_formats_date_and_timestamp() {
        assert_eq!(DateKind::Date.format(0), "1970-01-01");
        assert_eq!(
            DateKind::Timestamp {
                unit: TimeUnit::Millis,
                adjusted_to_utc: true
            }
            .format(0),
            "1970-01-01T00:00:00Z"
        );
        assert_eq!(
            DateKind::Timestamp {
                unit: TimeUnit::Millis,
                adjusted_to_utc: false
            }
            .format(0),
            "1970-01-01T00:00:00 (unadjusted)"
        );
    }

    #[test]
    fn exact_bounds_are_marked_verified() {
        let column = DateColumn {
            path: "ts".into(),
            kind: DateKind::Date,
        };
        let bounds = exact_bounds(&column, Some(2), Some(9), 3, 30);
        assert_eq!(bounds.status, CoverageStatus::Complete);
        assert!(bounds.verified_by_scan);
        assert_eq!(bounds.min.as_deref(), Some("1970-01-03"));
        assert_eq!(bounds.rows_covered, 30);
    }
}
