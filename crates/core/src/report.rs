// SPDX-License-Identifier: Apache-2.0
//! Composable preview report and bounded plain-text rendering.
//!
//! The same report feeds the Windows preview pane, the headless tool and the
//! worker protocol. Text output is capped so a hostile schema cannot produce an
//! unbounded payload.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::about::{about_text, MADE_BY, STORE_COPY};
use crate::baseline::BaselineSpec;
use crate::display::escape_text;
use crate::market::{CheckStatus, MarketProfile, MarketReport};
use crate::sample::{SampleReport, SampleStatus};
use crate::scan::{ScanReport, ScanStatus};
use crate::summary::Summary;
use crate::{CancelToken, Result, Session};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewOptions {
    /// File name shown in the header. Never a full path.
    pub display_name: Option<String>,
    pub sample_offset: u64,
    pub sample_limit: usize,
    /// Run the optional full decoded scan.
    pub scan: bool,
    /// Optional market diagnostics profile.
    pub market_profile: Option<MarketProfile>,
    /// Optional local dated baseline.
    pub baseline: Option<BaselineSpec>,
    /// Requested sample size for diagnostics.
    pub diagnostic_rows: usize,
}

impl Default for PreviewOptions {
    fn default() -> Self {
        Self {
            display_name: None,
            sample_offset: 0,
            sample_limit: 100,
            scan: false,
            market_profile: None,
            baseline: None,
            diagnostic_rows: 50_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewReport {
    pub product: String,
    pub version: String,
    pub credit: String,
    pub licence: String,
    pub display_name: Option<String>,
    pub summary: Summary,
    pub sample: Option<SampleReport>,
    pub scan: Option<ScanReport>,
    pub market: Option<MarketReport>,
    pub notices: Vec<String>,
}

impl Session {
    /// Build a preview, retaining the footer summary even when an optional
    /// phase fails. Only opening the file is fatal.
    pub fn preview(&self, options: &PreviewOptions) -> Result<PreviewReport> {
        let mut notices = Vec::new();
        let sample = match self.sample(options.sample_offset, options.sample_limit) {
            Ok(sample) => Some(sample),
            Err(error) => {
                notices.push(format!("sample unavailable: {error}"));
                None
            }
        };
        let scan = if options.scan {
            match self.scan(&CancelToken::new()) {
                Ok(scan) => Some(scan),
                Err(error) => {
                    notices.push(format!("full scan failed: {error}"));
                    None
                }
            }
        } else {
            None
        };
        let market = if let Some(profile) = &options.market_profile {
            match self.diagnose(
                profile,
                options.baseline.as_ref(),
                options.diagnostic_rows,
                &CancelToken::new(),
            ) {
                Ok(market) => Some(market),
                Err(error) => {
                    notices.push(format!("market diagnostics unavailable: {error}"));
                    None
                }
            }
        } else {
            None
        };
        Ok(PreviewReport {
            product: crate::about::PRODUCT.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            credit: MADE_BY.to_string(),
            licence: STORE_COPY.to_string(),
            display_name: options
                .display_name
                .as_deref()
                .map(|name| escape_text(name, 120)),
            summary: self.summary().clone(),
            sample,
            scan,
            market,
            notices,
        })
    }

    pub fn render_preview(&self, options: &PreviewOptions) -> Result<String> {
        let report = self.preview(options)?;
        Ok(render(&report))
    }
}

/// Render the report to plain text, capped at roughly 120k characters.
pub fn render(report: &PreviewReport) -> String {
    let mut out = String::with_capacity(8192);
    let mut budget = TextBudget::new(120_000, 2_000);

    budget.line(&mut out, &format!("{} {}", report.product, report.version));
    budget.line(
        &mut out,
        &format!(
            "File: {}",
            report
                .display_name
                .clone()
                .unwrap_or_else(|| "selected stream".to_string())
        ),
    );
    budget.line(
        &mut out,
        &format!(
            "Size: {} bytes | footer metadata: {} bytes | format v{}",
            report.summary.file.file_bytes,
            report.summary.file.footer_bytes,
            report.summary.file.format_version
        ),
    );
    if let Some(created_by) = &report.summary.file.created_by {
        budget.line(&mut out, &format!("Created by: {created_by}"));
    }
    budget.blank(&mut out);

    budget.heading(
        &mut out,
        &format!(
            "Rows: {} ({})",
            report
                .scan
                .as_ref()
                .and_then(|scan| scan.exact_row_count)
                .unwrap_or(report.summary.row_count.value),
            row_count_label(report)
        ),
    );
    budget.line(
        &mut out,
        &format!(
            "Row groups: {} | leaf columns: {}",
            report.summary.row_groups.total, report.summary.file.leaf_column_count
        ),
    );
    if !report.summary.row_groups.listed.is_empty() {
        let shown = report.summary.row_groups.listed.len().min(10);
        for group in report.summary.row_groups.listed.iter().take(shown) {
            budget.line(
                &mut out,
                &format!(
                    "  group {}: {} rows, {} compressed bytes, {} columns",
                    group.index, group.rows, group.compressed_bytes, group.columns
                ),
            );
        }
        if report.summary.row_groups.listed.len() > shown {
            budget.line(
                &mut out,
                &format!(
                    "  ... {} more row group(s)",
                    report.summary.row_groups.listed.len() - shown
                ),
            );
        }
    }
    budget.blank(&mut out);

    budget.heading(&mut out, "Schema");
    render_schema(&mut out, report, &mut budget);

    budget.heading(&mut out, "Date and timestamp bounds");
    if report.summary.date_bounds.is_empty() {
        budget.line(
            &mut out,
            "No top-level date or timestamp columns; bounds not applicable.",
        );
    } else {
        let bounds = report
            .scan
            .as_ref()
            .filter(|scan| scan.status == ScanStatus::Complete)
            .map(|scan| scan.date_bounds.as_slice())
            .unwrap_or(report.summary.date_bounds.as_slice());
        for bound in bounds {
            let status = match bound.status {
                crate::CoverageStatus::Complete => "complete",
                crate::CoverageStatus::Partial => "partial",
                crate::CoverageStatus::Unavailable => "unavailable",
            };
            budget.line(
                &mut out,
                &format!(
                    "  {} ({}): {} | {} to {} | {}/{} row groups, {}/{} rows | {}",
                    bound.column,
                    bound.kind,
                    status,
                    bound.min.as_deref().unwrap_or("n/a"),
                    bound.max.as_deref().unwrap_or("n/a"),
                    bound.covered_row_groups,
                    bound.total_row_groups,
                    bound.rows_covered,
                    bound.total_rows,
                    bound.reason
                ),
            );
        }
    }
    for notice in &report.summary.notices {
        budget.line(&mut out, &format!("Note: {notice}"));
    }
    budget.blank(&mut out);

    budget.heading(&mut out, "Sample");
    render_sample(&mut out, report, &mut budget);

    budget.heading(&mut out, "Full scan");
    match &report.scan {
        None => {
            budget.line(
                &mut out,
                "Not run. Footer counts and statistics remain metadata-derived, not verified.",
            );
        }
        Some(scan) => {
            let status = match scan.status {
                ScanStatus::Complete => "complete",
                ScanStatus::Incomplete => "incomplete",
                ScanStatus::Cancelled => "cancelled",
            };
            budget.line(
                &mut out,
                &format!(
                    "{}: {} rows decoded, {}/{} row groups, {} decoded bytes, {} ms",
                    status,
                    scan.rows_scanned,
                    scan.row_groups_scanned,
                    scan.row_groups_total,
                    scan.decoded_bytes,
                    scan.elapsed_millis
                ),
            );
            budget.line(&mut out, &format!("  {}", scan.message));
            if !scan.skipped_row_groups.is_empty() {
                budget.line(
                    &mut out,
                    &format!(
                        "  skipped row groups: {}",
                        scan.skipped_row_groups
                            .iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                );
            }
            if scan.exact_row_count.is_some() {
                budget.line(
                    &mut out,
                    "  Row count and date bounds are exact after the complete scan.",
                );
            }
        }
    }
    budget.blank(&mut out);

    budget.heading(&mut out, "Market diagnostics");
    render_market(&mut out, report, &mut budget);

    if !report.notices.is_empty() {
        budget.heading(&mut out, "Notices");
        for notice in &report.notices {
            budget.line(&mut out, &format!("  - {notice}"));
        }
    }

    budget.blank(&mut out);
    budget.line(
        &mut out,
        &format!(
            "{} - {}.",
            report.credit,
            report.licence
        ),
    );
    budget.line(
        &mut out,
        "Read-only. Not affiliated with MetaQuotes; MetaTrader 5 and MT5 are trademarks of MetaQuotes Ltd.",
    );
    if budget.truncated {
        out.push_str("\n[text truncated]\n");
    }
    out
}

fn row_count_label(report: &PreviewReport) -> String {
    if let Some(scan) = &report.scan {
        if scan.exact_row_count.is_some() {
            return "exact after complete decoded scan".to_string();
        }
        return format!(
            "metadata-derived; scan verified {} rows but did not complete",
            scan.rows_scanned
        );
    }
    report.summary.row_count.label.clone()
}

fn render_schema(out: &mut String, report: &PreviewReport, budget: &mut TextBudget) {
    fn walk(
        field: &crate::SchemaField,
        depth: usize,
        out: &mut String,
        budget: &mut TextBudget,
    ) {
        if budget.is_full() || depth > 8 {
            return;
        }
        let indent = "  ".repeat(depth);
        let repetition = if field.nullable { "optional" } else { "required" };
        if field.leaf {
            let mut line = format!(
                "{indent}{} : {} [{}]",
                field.name,
                field.physical_type.as_deref().unwrap_or("?"),
                repetition
            );
            if let Some(logical) = &field.logical_type {
                let _ = write!(line, " logical={logical}");
            }
            budget.line(out, &line);
        } else {
            let mut line = format!("{indent}{} : group [{}]", field.name, repetition);
            if let Some(logical) = &field.logical_type {
                let _ = write!(line, " logical={logical}");
            }
            budget.line(out, &line);
            for child in &field.children {
                walk(child, depth + 1, out, budget);
            }
            if field.children_omitted > 0 {
                budget.line(
                    out,
                    &format!("{indent}  ... {} child leaf column(s) omitted", field.children_omitted),
                );
            }
        }
    }

    for field in &report.summary.schema.fields {
        walk(field, 0, out, budget);
    }
    if report.summary.schema.fields.is_empty() {
        budget.line(out, "  (empty schema)");
    }
}

fn render_sample(out: &mut String, report: &PreviewReport, budget: &mut TextBudget) {
    let Some(sample) = &report.sample else {
        budget.line(out, "Sample unavailable.");
        return;
    };
    match sample.status {
        SampleStatus::Complete => {
            let range = if sample.returned == 0 {
                format!("No rows at offset {}", sample.offset)
            } else {
                format!(
                    "Rows {}-{}",
                    sample.offset,
                    sample.offset + sample.returned as u64 - 1
                )
            };
            budget.line(
                out,
                &format!(
                    "{} of {} ({} returned, {} columns{}); {}",
                    range,
                    sample.total_rows_metadata,
                    sample.returned,
                    sample.columns.len(),
                    if sample.omitted_columns > 0 {
                        format!(", {} omitted", sample.omitted_columns)
                    } else {
                        String::new()
                    },
                    sample.message
                ),
            );
        }
        SampleStatus::Truncated | SampleStatus::Unavailable => {
            budget.line(
                out,
                &format!("{} ({} returned): {}", status_label(sample.status), sample.returned, sample.message),
            );
        }
    }
    if sample.rows.is_empty() {
        return;
    }
    let shown_columns = sample.columns.len().min(12);
    let mut header = String::from("  ");
    for (index, name) in sample.columns.iter().take(shown_columns).enumerate() {
        if index > 0 {
            header.push_str(" | ");
        }
        header.push_str(&truncate(name, 24));
    }
    if sample.columns.len() > shown_columns {
        header.push_str(" | ...");
    }
    budget.line(out, &header);
    for row in &sample.rows {
        let mut line = String::from("  ");
        for (index, value) in row.values.iter().take(shown_columns).enumerate() {
            if index > 0 {
                line.push_str(" | ");
            }
            line.push_str(&truncate(&value.render(), 40));
        }
        if row.values.len() > shown_columns {
            line.push_str(" | ...");
        }
        if !budget.line(out, &line) {
            break;
        }
    }
    if sample.omitted_columns > 0 {
        budget.line(
            out,
            &format!("  ... {} column(s) not shown", sample.omitted_columns),
        );
    }
}

fn render_market(out: &mut String, report: &PreviewReport, budget: &mut TextBudget) {
    let Some(market) = &report.market else {
        budget.line(
            out,
            "Disabled. Configure an explicit field mapping in the optional local settings file to enable heuristics.",
        );
        return;
    };
    budget.line(
        out,
        &format!(
            "{} | {} | {} row(s) used",
            status_label_market(market.status),
            market.coverage,
            market.rows_used
        ),
    );
    if let Some(instrument) = &market.instrument {
        budget.line(out, &format!("Instrument: {instrument}"));
    }
    let thresholds = &market.thresholds;
    budget.line(
        out,
        &format!(
            "Thresholds: spread distinct <= {} | open/prev-close <= {:.0}% | |excess kurtosis| > {:.2} | zero returns > {:.0}% | invalid OHLC > {:.0}% | baseline diff > {:.2}%",
            thresholds.constant_spread_max_distinct,
            thresholds.open_equals_prev_close_max_ratio * 100.0,
            thresholds.max_excess_kurtosis,
            thresholds.max_zero_return_ratio * 100.0,
            thresholds.max_invalid_price_ratio * 100.0,
            thresholds.baseline_max_relative_diff * 100.0,
        ),
    );
    if let Some(range) = &market.time_range {
        budget.line(
            out,
            &format!(
                "Time: {} to {} | null rows: {} | non-monotonic: {} | duplicates: {}",
                range.min,
                range.max,
                range.null_rows,
                range.non_monotonic_steps,
                range.duplicate_steps
            ),
        );
    }
    for check in &market.checks {
        budget.line(
            out,
            &format!(
                "  [{}] {}: {}",
                status_label_market(check.status),
                check.title,
                check.summary
            ),
        );
    }
    if let Some(baseline) = &market.baseline {
        budget.line(
            out,
            &format!(
                "Baseline: {} ({}, licence {}) matched {} of {} data day(s), max relative diff {} vs {}",
                baseline.instrument,
                baseline.source,
                baseline.licence,
                baseline.matched_days,
                baseline.data_days,
                baseline.max_relative_diff.as_deref().unwrap_or("n/a"),
                baseline.threshold
            ),
        );
    }
    for note in &market.notes {
        budget.line(out, &format!("  Note: {note}"));
    }
}

fn status_label(status: SampleStatus) -> &'static str {
    match status {
        SampleStatus::Complete => "complete",
        SampleStatus::Truncated => "truncated",
        SampleStatus::Unavailable => "unavailable",
    }
}

fn status_label_market(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Ok => "ok",
        CheckStatus::Warning => "warning",
        CheckStatus::Unavailable => "unavailable",
        CheckStatus::NotRun => "not run",
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out = text.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}

struct TextBudget {
    chars_left: usize,
    lines_left: usize,
    truncated: bool,
}

impl TextBudget {
    fn new(chars: usize, lines: usize) -> Self {
        Self {
            chars_left: chars,
            lines_left: lines,
            truncated: false,
        }
    }

    fn is_full(&self) -> bool {
        self.chars_left == 0 || self.lines_left == 0
    }

    fn line(&mut self, out: &mut String, line: &str) -> bool {
        if self.is_full() {
            self.truncated = true;
            return false;
        }
        let line = line.replace('\r', " ").replace('\n', " ");
        let line_chars = line.chars().count();
        let mut take = line.chars().take(self.chars_left).collect::<String>();
        self.chars_left = self.chars_left.saturating_sub(take.chars().count());
        if take.chars().count() < line_chars {
            self.truncated = true;
        }
        self.lines_left = self.lines_left.saturating_sub(1);
        if self.lines_left == 0 {
            take.push_str(" ...");
        }
        out.push_str(&take);
        out.push('\n');
        true
    }

    fn heading(&mut self, out: &mut String, heading: &str) {
        self.blank(out);
        self.line(out, heading);
    }

    fn blank(&mut self, out: &mut String) {
        if !self.is_full() && !out.ends_with("\n\n") {
            out.push('\n');
        }
    }
}

/// Convenience for an About screen / dialog.
pub fn about_screen() -> String {
    about_text()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_helper() {
        assert_eq!(truncate("abcdef", 3), "abc...");
        assert_eq!(truncate("abc", 3), "abc");
    }

    #[test]
    fn text_budget_bounds_output() {
        let mut out = String::new();
        let mut budget = TextBudget::new(10, 3);
        budget.line(&mut out, "0123456789abcdef");
        budget.line(&mut out, "second");
        budget.line(&mut out, "third");
        budget.line(&mut out, "fourth");
        assert!(out.chars().count() < 40);
        assert!(budget.truncated);
    }
}
