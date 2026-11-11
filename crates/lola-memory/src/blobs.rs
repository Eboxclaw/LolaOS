//! Content-addressed blob store — path scheme and validation (ADR-009).
//!
//! Bulky payloads (model request/response bodies, session bulk, feature
//! snapshot bodies, omni media) live as zstd files addressed by their
//! sha256. Rows keep the hash forever; payloads may be TTL-compacted or
//! garbage-collected only when no live row references them.

use std::path::{Path, PathBuf};

/// `<root>/blobs/<hash[..2]>/<hash>.zst`
pub fn blob_path(root: &Path, hash: &str) -> PathBuf {
    let prefix = &hash[..2.min(hash.len())];
    root.join("blobs").join(prefix).join(format!("{hash}.zst"))
}

/// A valid blob hash is 64 lowercase hex chars (sha256).
pub fn is_valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_paths_are_sharded_by_hash_prefix() {
        let path = blob_path(Path::new("/data"), &"ab".to_string().repeat(32));
        assert_eq!(
            path,
            Path::new("/data/blobs/ab/").join(format!("{}.zst", "ab".repeat(32)))
        );
    }

    #[test]
    fn hash_validation_rejects_malformed_hashes() {
        assert!(is_valid_hash(&"ab".repeat(32)));
        assert!(!is_valid_hash(&"AB".repeat(32)), "uppercase rejected");
        assert!(!is_valid_hash("deadbeef"), "too short");
    }
}
