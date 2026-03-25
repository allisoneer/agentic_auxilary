//! Sparse n-gram extraction and weighting.
//!
//! MVP note: this phase uses deterministic dense and sparse 3-byte grams plus a
//! stable hash-based weight. It does not yet ship the blog's ideal precomputed
//! corpus-frequency weighting table, but preserves the gram abstraction so the
//! weighting strategy can be upgraded later without reworking callers.

/// Opaque gram key used by the instant-grep index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GramKey(pub u64);

const KIND_DENSE: u8 = 0;
const KIND_SKIP_1: u8 = 1;
const KIND_SKIP_2: u8 = 2;

fn pack(kind: u8, a: u8, b: u8, c: u8) -> GramKey {
    GramKey((u64::from(kind) << 56) | (u64::from(a) << 16) | (u64::from(b) << 8) | u64::from(c))
}

/// Emit all overlapping dense 3-byte grams.
pub fn dense_grams(bytes: &[u8]) -> impl Iterator<Item = GramKey> + '_ {
    bytes
        .windows(3)
        .map(|window| pack(KIND_DENSE, window[0], window[1], window[2]))
}

/// Emit a small sparse gram set from 4-byte windows.
///
/// For every 4-byte window `[a, b, c, d]`, emit:
/// - `a ? c d` (skip position 1)
/// - `a b ? d` (skip position 2)
pub fn sparse_grams(bytes: &[u8]) -> impl Iterator<Item = GramKey> + '_ {
    bytes.windows(4).flat_map(|window| {
        [
            pack(KIND_SKIP_1, window[0], window[2], window[3]),
            pack(KIND_SKIP_2, window[0], window[1], window[3]),
        ]
    })
}

/// Emit the MVP gram set for indexing/query planning.
pub fn all_grams(bytes: &[u8]) -> impl Iterator<Item = GramKey> + '_ {
    dense_grams(bytes).chain(sparse_grams(bytes))
}

/// Deterministic MVP gram weight.
///
/// Higher values are considered better/rarer anchors by later planning code.
/// This is an explicit temporary approximation until a frequency-derived weight
/// table is added.
pub fn gram_weight(gram: GramKey) -> u32 {
    let mut x = gram.0;
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    x as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dense_grams_emit_overlapping_triples() {
        let grams: Vec<_> = dense_grams(b"abcd").collect();
        assert_eq!(grams.len(), 2);
        assert_eq!(grams[0], pack(KIND_DENSE, b'a', b'b', b'c'));
        assert_eq!(grams[1], pack(KIND_DENSE, b'b', b'c', b'd'));
    }

    #[test]
    fn sparse_grams_emit_two_variants_per_window() {
        let grams: Vec<_> = sparse_grams(b"abcd").collect();
        assert_eq!(grams.len(), 2);
        assert_eq!(grams[0], pack(KIND_SKIP_1, b'a', b'c', b'd'));
        assert_eq!(grams[1], pack(KIND_SKIP_2, b'a', b'b', b'd'));
    }

    #[test]
    fn all_grams_chain_dense_and_sparse() {
        assert_eq!(all_grams(b"abcd").count(), 4);
    }

    #[test]
    fn gram_weight_is_stable() {
        let gram = pack(KIND_DENSE, b'a', b'b', b'c');
        assert_eq!(gram_weight(gram), gram_weight(gram));
    }
}
