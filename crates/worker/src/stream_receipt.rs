// SPDX-License-Identifier: Apache-2.0
//! Completion is an edge, not a condition that can be acknowledged repeatedly.

pub(crate) fn newly_complete(before: u64, after: u64, declared: u64) -> bool {
    before < declared && after == declared
}

#[cfg(test)]
mod tests {
    use super::newly_complete;

    #[test]
    fn exactly_one_ack_per_nonempty_stream_even_with_empty_chunks() {
        for size in 1..=1000_u64 {
            let mut received = 0;
            let mut acknowledgements = 0;
            for step in 0..size {
                let after = step + 1;
                acknowledgements += newly_complete(received, after, size) as u32;
                received = after;
                for _ in 0..3 {
                    acknowledgements += newly_complete(received, received, size) as u32;
                }
            }
            assert_eq!(acknowledgements, 1, "size {size}");
        }
        assert!(!newly_complete(0, 0, 0));
    }
}
