# Fixtures

Generated deterministically by `parquet-preview-fixtures`; do not edit by hand.

```text
cargo run -p parquet-preview-fixtures -- tests/fixtures
(cd tests/fixtures && find . -type f -name '*.parquet' -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS)
```

| File | Purpose |
| --- | --- |
| `trades.parquet` | 60 rows, 3 row groups, nullable timestamp/string, nested struct and list, Snappy |
| `multi_row_groups.parquet` | 100 rows, 5 row groups, date32 and microsecond timestamp, Zstd |
| `missing_stats.parquet` | Statistics disabled, so bounds must not be claimed complete |
| `nested.parquet` | Nullable struct/list, decimal, binary, date, unadjusted timestamp, null column |
| `constant_market.parquet` | Deliberately suspicious market series |
| `empty.parquet` | Zero rows |
| `codec_*.parquet` | Snappy, Gzip, Zstd, Brotli and LZ4_RAW |
| `baseline.csv` | Dated reference prices for diagnostics tests |
| `malformed/` | Truncated, wrong magic/header, oversized footer, encrypted magic, empty, short |

All data is synthetic. The market series is not real market data and the
heuristics are not financial advice.
