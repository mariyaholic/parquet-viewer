// SPDX-License-Identifier: Apache-2.0
//! About text shared by the preview pane and the headless tool.

pub const PRODUCT: &str = "Parquet viewer and preview";
pub const MADE_BY: &str = "Made by Maria";
pub const STORE_COPY: &str = "free and open source (Apache-2.0)";

pub fn about_text() -> String {
    format!(
        "{PRODUCT}\n\
         {MADE_BY}\n\
         Version {version}\n\
         This product is {STORE_COPY}.\n\
         \n\
         Read-only: files are inspected locally and never modified, uploaded or traded.\n\
         Footer counts and statistics are file-provided metadata; they are labelled and only become exact after a complete scan.\n\
         Market diagnostics are optional heuristics, not proof of fraud or authenticity, and not financial advice.\n\
         \n\
         Not affiliated with MetaQuotes. MetaTrader 5 and MT5 are trademarks of MetaQuotes Ltd.\n\
         No code signing yet; Windows SmartScreen may warn.",
        version = env!("CARGO_PKG_VERSION")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_has_required_credits_and_copy() {
        let text = about_text();
        assert!(text.starts_with(PRODUCT));
        assert!(text.contains(MADE_BY));
        assert!(text.contains(STORE_COPY));
        assert!(text.contains("Read-only"));
        assert!(text.contains("Not affiliated with MetaQuotes"));
    }
}
