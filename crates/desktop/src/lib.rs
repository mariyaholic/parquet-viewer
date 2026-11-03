// SPDX-License-Identifier: Apache-2.0
use parquet_preview_protocol::{
    decode_control, read_frame, write_control, write_stream_chunk, FrameLimits, Request, Response,
    PROTOCOL_VERSION,
};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use viewer_shell::{supervisor::Adapter, Table};
pub fn adapter() -> Adapter {
    Adapter {
        title: "Parquet Viewer",
        worker: "parquet-preview-worker",
        csv_spool: false,
        inspect,
    }
}
fn receive(mut out: &mut dyn Read) -> Result<Response, String> {
    let f = read_frame(&mut out, &FrameLimits::default())
        .map_err(|e| e.to_string())?
        .ok_or("Worker closed unexpectedly")?;
    let response: Response = decode_control(f).map_err(|e| e.to_string())?;
    if let Response::Error { message, .. } = response {
        return Err(message);
    }
    Ok(response)
}
fn inspect(
    file: &mut File,
    _spool: &Path,
    mut input: &mut dyn Write,
    output: &mut dyn Read,
    cancel: &AtomicBool,
) -> Result<Table, String> {
    write_control(
        &mut input,
        &Request::Hello {
            protocol_version: PROTOCOL_VERSION,
            client: Some("desktop".into()),
        },
    )
    .map_err(|e| e.to_string())?;
    let max = match receive(output)? {
        Response::Welcome {
            protocol_version,
            worker_version,
            max_file_bytes,
            ..
        } if protocol_version == PROTOCOL_VERSION
            && worker_version == env!("CARGO_PKG_VERSION") =>
        {
            max_file_bytes
        }
        _ => return Err("Mismatched worker version".into()),
    };
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if size > max {
        return Err("File exceeds worker transfer limit".into());
    }
    write_control(
        &mut input,
        &Request::OpenStream {
            declared_size: size,
        },
    )
    .map_err(|e| e.to_string())?;
    let mut remaining = size;
    let mut buf = [0u8; 65536];
    while remaining > 0 {
        if cancel.load(Ordering::SeqCst) {
            return Err("Cancelled".into());
        }
        let n = file
            .read(&mut buf[..remaining.min(65536) as usize])
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("Source ended during transfer".into());
        }
        write_stream_chunk(&mut input, &buf[..n]).map_err(|e| e.to_string())?;
        remaining -= n as u64;
    }
    match receive(output)? {
        Response::StreamAccepted { bytes } if bytes == size => {}
        _ => return Err("Worker did not accept transfer".into()),
    }
    write_control(&mut input, &Request::Inspect).map_err(|e| e.to_string())?;
    let summary = match receive(output)? {
        Response::Summary { summary } => summary,
        _ => return Err("Worker did not return metadata".into()),
    };
    write_control(
        &mut input,
        &Request::Sample {
            offset: 0,
            limit: 200,
        },
    )
    .map_err(|e| e.to_string())?;
    let sample = match receive(output)? {
        Response::Sample { sample } => sample,
        _ => return Err("Worker did not return a sample".into()),
    };
    let details = format!(
        "Metadata (not independently verified):\n{}\n\nTyped sample:\n{}",
        serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?,
        serde_json::to_string_pretty(&sample).map_err(|e| e.to_string())?
    );
    let coverage = format!(
        "{} sampled rows; {} metadata-reported rows (unverified). {:?}: {}",
        sample.returned, sample.total_rows_metadata, sample.status, sample.message
    );
    Ok(Table {
        columns: sample.columns,
        rows: sample
            .rows
            .into_iter()
            .map(|r| r.values.iter().map(value).collect())
            .collect(),
        coverage,
        details,
        page: None,
    }
    .bounded())
}
fn value(v: &parquet_preview_core::sample::CellValue) -> String {
    use parquet_preview_core::sample::CellValue;
    match v {
        CellValue::Null => "<null>".into(),
        CellValue::Text(s) => format!("{s:?}"),
        _ => v.render(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn null_is_not_text_null_or_empty() {
        use parquet_preview_core::sample::CellValue::*;
        assert_ne!(value(&Null), value(&Text("<null>".into())));
        assert_ne!(value(&Null), value(&Text(String::new())));
    }
}

#[cfg(test)]
mod browse_tests {
    use super::*;
    #[test]
    fn desktop_page_has_real_source_offsets_and_detects_stale_sources() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/trades.parquet");
        let request = viewer_shell::browse::Request {
            offset: 1,
            ..Default::default()
        };
        let page = browse(&path, &request).unwrap();
        assert_eq!(page.page.as_ref().unwrap().source_rows[0], 1);
        let mut request = viewer_shell::browse::Request {
            query: "definitely absent literal.*".into(),
            ..Default::default()
        };
        let missing = browse(&path, &request).unwrap();
        assert!(missing.rows.is_empty());
        assert!(missing.page.unwrap().next.is_none());
        request.expected = Some(viewer_shell::browse::Stamp {
            bytes: 0,
            modified_ns: None,
        });
        assert!(browse(&path, &request).unwrap_err().contains("changed"));
    }
}

/// Desktop-only seekable read. Explorer Low-IL/stream protocol is unchanged.
pub fn browse(path: &Path, request: &viewer_shell::browse::Request) -> Result<Table, String> {
    request.validate()?;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    let stamp = viewer_shell::browse::Stamp::from_metadata(&meta);
    if !meta.is_file() {
        return Err("Choose a regular local file".into());
    }
    if request.expected.as_ref().is_some_and(|s| *s != stamp) {
        return Err("Source changed; Reload before continuing".into());
    }

    let limits = parquet_preview_core::Limits {
        max_sample_columns: 64,
        max_sample_offset: u64::MAX,
        ..Default::default()
    };
    let session = parquet_preview_core::Session::open(
        Box::new(file.try_clone().map_err(|e| e.to_string())?),
        &limits,
    )
    .map_err(|e| e.to_string())?;
    let total = session.summary().row_count.value;
    let mut cursor = request.offset;
    let mut rows = Vec::new();
    let mut source_rows = Vec::new();
    let mut retained = 0usize;
    let needle = request.query.to_lowercase();
    let began = std::time::Instant::now();
    let mut scanned = 0u64;
    'pages: while cursor < total
        && rows.len() < 200
        && scanned < 20_000
        && began.elapsed() < std::time::Duration::from_secs(20)
    {
        let sample = session.sample(cursor, 200).map_err(|e| e.to_string())?;
        if sample.offset != cursor || sample.rows.is_empty() {
            return Err(format!(
                "Cannot decode source row {}: {}",
                cursor.saturating_add(1),
                sample.message
            ));
        }
        for row in sample.rows {
            let values: Vec<String> = row.values.iter().map(value).collect();
            let matches =
                needle.is_empty() || values.iter().any(|v| v.to_lowercase().contains(&needle));
            let cost: usize = values.iter().map(String::len).sum();
            if matches && retained + cost > 4 * 1024 * 1024 {
                break 'pages;
            }
            if matches {
                retained += cost;
                rows.push(values);
                source_rows.push(cursor);
            }
            cursor += 1;
            scanned += 1;
            if rows.len() == 200 {
                break;
            }
        }
        if retained >= 4 * 1024 * 1024 {
            break;
        }
    }
    if viewer_shell::browse::Stamp::from_metadata(&file.metadata().map_err(|e| e.to_string())?)
        != stamp
    {
        return Err("Source changed while reading; Reload".into());
    }
    let next = (cursor < total).then_some(cursor);
    let first = session
        .sample(request.offset, 1)
        .map_err(|e| e.to_string())?;
    let coverage=format!("{} page rows; scanned {} rows from {}; {}. {} metadata-reported rows (unverified). Search covers bounded displayed values / first 64 columns.",rows.len(),scanned,request.offset.saturating_add(1),if next.is_some(){"Next continues reading/search"}else{"metadata row range exhausted"},total);
    let details = serde_json::to_string_pretty(session.summary()).map_err(|e| e.to_string())?;
    Ok(Table {
        columns: first.columns,
        rows,
        coverage,
        details,
        page: Some(viewer_shell::browse::Page {
            start: request.offset,
            next,
            source_rows,
            stamp,
            next_byte: None,
            scanned_to: cursor,
        }),
    }
    .bounded())
}
