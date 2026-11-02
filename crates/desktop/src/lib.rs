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
