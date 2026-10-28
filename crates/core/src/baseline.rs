// SPDX-License-Identifier: Apache-2.0
//! Optional local, dated reference-price baseline.
//!
//! The file is read-only local CSV. Nothing is downloaded and no baseline is
//! assumed: without a dated, instrument-specific file the price comparison is
//! reported as unavailable.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::display::escape_text;
use crate::time::{civil_from_days, days_from_civil};
use crate::{PreviewError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineSpec {
    /// Local path supplied by the user's settings file.
    pub path: String,
    /// Instrument this file is licensed/valid for.
    pub instrument: Option<String>,
    /// Licence identifier/terms under which this baseline is used.
    #[serde(default)]
    pub licence: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReferenceBaseline {
    pub instrument: String,
    pub licence: String,
    pub source_label: String,
    pub points: BTreeMap<i64, f64>,
    pub duplicate_dates_ignored: usize,
    pub skipped_rows: usize,
}

impl ReferenceBaseline {
    pub fn load(
        spec: &BaselineSpec,
        profile_instrument: Option<&str>,
        max_bytes: u64,
    ) -> Result<Self> {
        if spec.path.trim().is_empty() {
            return Err(PreviewError::Baseline(
                "baseline path is empty".to_string(),
            ));
        }
        if let (Some(a), Some(b)) = (spec.instrument.as_deref(), profile_instrument) {
            if !a.eq_ignore_ascii_case(b) {
                return Err(PreviewError::Baseline(
                    "baseline instrument does not match the profile instrument".to_string(),
                ));
            }
        }
        let instrument = spec
            .instrument
            .clone()
            .or_else(|| profile_instrument.map(str::to_string))
            .ok_or_else(|| {
                PreviewError::Baseline(
                    "a baseline needs an explicit instrument in the settings or profile"
                        .to_string(),
                )
            })?;
        let licence = spec
            .licence
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                PreviewError::Baseline(
                    "a baseline needs an explicit licence identifier; an unlicensed baseline is not compared"
                        .to_string(),
                )
            })?
            .chars()
            .take(160)
            .collect::<String>();

        let path = Path::new(&spec.path);
        let source_label = path
            .file_name()
            .map(|name| escape_text(&name.to_string_lossy(), 80))
            .unwrap_or_else(|| "baseline.csv".to_string());
        let file = std::fs::File::open(path).map_err(|e| {
            PreviewError::Baseline(format!("cannot open the baseline file: {e}"))
        })?;
        let mut bytes = Vec::new();
        file.take(max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|e| PreviewError::Baseline(format!("cannot read the baseline file: {e}")))?;
        if bytes.len() as u64 > max_bytes {
            return Err(PreviewError::Baseline(format!(
                "baseline exceeds the {max_bytes}-byte limit"
            )));
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| PreviewError::Baseline("baseline is not UTF-8 text".to_string()))?;
        let text = text.trim_start_matches('\u{FEFF}');
        Self::parse_csv(text, &instrument, &licence, source_label)
    }

    fn parse_csv(
        text: &str,
        instrument: &str,
        licence: &str,
        source_label: String,
    ) -> Result<Self> {
        let mut lines = text.lines().enumerate().filter_map(|(index, line)| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                None
            } else {
                Some((index + 1, trimmed))
            }
        });
        let Some((header_line, header)) = lines.next() else {
            return Err(PreviewError::Baseline(
                "baseline is empty; expected a date,price header".to_string(),
            ));
        };
        let header = split_csv(header);
        let date_index = header
            .iter()
            .position(|h| h.eq_ignore_ascii_case("date"));
        let price_index = header
            .iter()
            .position(|h| h.eq_ignore_ascii_case("price") || h.eq_ignore_ascii_case("close"));
        let instrument_index = header
            .iter()
            .position(|h| h.eq_ignore_ascii_case("instrument"));
        let (Some(date_index), Some(price_index)) = (date_index, price_index) else {
            return Err(PreviewError::Baseline(format!(
                "baseline header (line {header_line}) must contain date and price columns"
            )));
        };

        let mut points = BTreeMap::new();
        let mut skipped = 0usize;
        let mut duplicates = 0usize;
        for (_line, row) in lines {
            let fields = split_csv(row);
            if fields.len() <= date_index.max(price_index) {
                skipped += 1;
                continue;
            }
            if let Some(index) = instrument_index {
                match fields.get(index) {
                    Some(value) if value.eq_ignore_ascii_case(instrument) => {}
                    _ => {
                        skipped += 1;
                        continue;
                    }
                }
            }
            let Some(day) = parse_iso_date(fields[date_index].trim()) else {
                skipped += 1;
                continue;
            };
            let Some(price) = fields[price_index].trim().parse::<f64>().ok() else {
                skipped += 1;
                continue;
            };
            if !price.is_finite() || price <= 0.0 {
                skipped += 1;
                continue;
            }
            if points.insert(day, price).is_some() {
                duplicates += 1;
            }
        }
        if points.is_empty() {
            return Err(PreviewError::Baseline(
                "baseline has no usable date/price rows for this instrument".to_string(),
            ));
        }
        Ok(Self {
            instrument: instrument.to_string(),
            licence: licence.to_string(),
            source_label,
            points,
            duplicate_dates_ignored: duplicates,
            skipped_rows: skipped,
        })
    }

    pub fn range_label(&self) -> String {
        let first = self.points.keys().next().copied();
        let last = self.points.keys().next_back().copied();
        match (first, last) {
            (Some(a), Some(b)) => format!("{} to {}", format_iso_date(a), format_iso_date(b)),
            _ => "empty".to_string(),
        }
    }
}

fn split_csv(line: &str) -> Vec<&str> {
    // The baseline format is deliberately simple: no quoted commas.
    line.split(',').map(str::trim).collect()
}

pub fn parse_iso_date(text: &str) -> Option<i64> {
    let mut parts = text.split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let days = days_from_civil(year, month, day);
    if civil_from_days(days) == (year, month, day) {
        Some(days)
    } else {
        None
    }
}

pub fn format_iso_date(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_iso_dates_strictly() {
        assert_eq!(parse_iso_date("2024-01-01"), Some(19_723));
        assert_eq!(parse_iso_date("2024-02-29"), Some(19_782));
        assert_eq!(parse_iso_date("2023-02-29"), None);
        assert_eq!(parse_iso_date("2024-13-01"), None);
        assert_eq!(parse_iso_date("2024-1-1"), Some(days_from_civil(2024, 1, 1)));
        assert_eq!(parse_iso_date("not-a-date"), None);
    }

    #[test]
    fn parses_instrument_specific_csv() {
        let csv = "instrument,date,price\nEURUSD,2024-01-01,1.10\nGBPUSD,2024-01-01,1.27\nEURUSD,2024-01-02,1.11\n";
        let baseline = ReferenceBaseline::parse_csv(csv, "EURUSD", "test", "test.csv".into()).unwrap();
        assert_eq!(baseline.points.len(), 2);
        assert_eq!(baseline.points.get(&19_723), Some(&1.10));
        assert_eq!(baseline.range_label(), "2024-01-01 to 2024-01-02");
    }

    #[test]
    fn rejects_missing_columns_and_bad_prices() {
        assert!(ReferenceBaseline::parse_csv("date,value\n", "X", "test", "t".into()).is_err());
        assert!(ReferenceBaseline::parse_csv(
            "date,price\n2024-01-01,\n",
            "X",
            "test",
            "t".into()
        )
        .is_err());
        assert!(ReferenceBaseline::parse_csv(
            "date,price\n2024-01-01,-1\n",
            "X",
            "test",
            "t".into()
        )
        .is_err());
    }

    #[test]
    fn missing_licence_blocks_a_base_line() {
        let spec = BaselineSpec {
            path: "unused.csv".into(),
            instrument: Some("EURUSD".into()),
            licence: None,
        };
        let error = ReferenceBaseline::load(&spec, None, 1024).unwrap_err();
        assert!(error.to_string().contains("licence"));
    }

    #[test]
    fn last_duplicate_wins_and_is_counted() {
        let csv = "date,price\n2024-01-01,1.0\n2024-01-01,1.2\n";
        let baseline = ReferenceBaseline::parse_csv(csv, "X", "test", "t".into()).unwrap();
        assert_eq!(baseline.points[&19_723], 1.2);
        assert_eq!(baseline.duplicate_dates_ignored, 1);
    }
}
