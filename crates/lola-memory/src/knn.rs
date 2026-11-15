//! SP1 retrieval: numeric bucket-kNN (ADR-041, docs/PIPELINE.md §2 R1).
//! Candidates load as compact numeric rows (no JSON parsing); ranking is
//! a plain 9-float distance over ≤[`KNN_CANDIDATE_LIMIT`] rows. Gate:
//! < 300 µs at 100k rows (criterion `bucket_knn_numeric`), replacing the
//! 7.99 ms JSON-scan shape measured pre-SP1.

use rusqlite::Connection;

use crate::error::Result;

/// The compact feature vector shape (matches `features::to_vector9` and
/// the `feature_vectors` columns; order fixed by ADR-041).
pub type Vector9 = [f32; 9];

/// One retrieval candidate: its snapshot id and numeric vector.
#[derive(Debug, Clone, PartialEq)]
pub struct KnnCandidate {
    pub snapshot_id: String,
    pub vector: Vector9,
}

/// Load candidates for a bucket signature: newest `limit` rows, numeric
/// columns only — the hot path never touches `features_json`.
pub fn knn_candidates(
    conn: &Connection,
    state_signature: &str,
    limit: usize,
) -> Result<Vec<KnnCandidate>> {
    let mut stmt = conn.prepare_cached(
        "SELECT snapshot_id, r1, r5, r20, atr, vr, spread, trend_s, vol_s, liq_s
         FROM feature_vectors
         WHERE state_signature = ?1
         ORDER BY event_time_ns DESC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![state_signature, limit as i64], |row| {
        Ok(KnnCandidate {
            snapshot_id: row.get(0)?,
            vector: [
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
                row.get(9)?,
            ],
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Squared Euclidean distance over the 9 dims — deterministic, no sqrt
/// needed for ranking.
fn dist2(a: &Vector9, b: &Vector9) -> f32 {
    let mut sum = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        let d = x - y;
        sum += d * d;
    }
    sum
}

/// Rank candidates against the query vector; returns the top-k, nearest
/// first. Ties break by candidate order (stable sort — reproducible).
pub fn rank_topk<'a>(
    query: &Vector9,
    candidates: &'a [KnnCandidate],
    k: usize,
) -> Vec<&'a KnnCandidate> {
    let mut ranked: Vec<(f32, usize)> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| (dist2(query, &c.vector), i))
        .collect();
    ranked.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    ranked
        .into_iter()
        .take(k)
        .map(|(_, i)| &candidates[i])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranking_is_nearest_first_and_reproducible() {
        let candidates: Vec<KnnCandidate> = (0..6)
            .map(|i| KnnCandidate {
                snapshot_id: format!("s{i}"),
                vector: [i as f32 * 0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            })
            .collect();
        let query = [1.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let top = rank_topk(&query, &candidates, 3);
        assert_eq!(top.len(), 3);
        // nearest: 1.0 (s2), 1.5 (s3), 0.5 (s1)
        assert_eq!(top[0].snapshot_id, "s2");
        assert_eq!(top[1].snapshot_id, "s3");
        assert_eq!(top[2].snapshot_id, "s1");
        // Reproducible: same query, same order.
        assert_eq!(
            top.iter()
                .map(|c| c.snapshot_id.clone())
                .collect::<Vec<_>>(),
            rank_topk(&query, &candidates, 3)
                .iter()
                .map(|c| c.snapshot_id.clone())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn empty_candidates_rank_to_empty() {
        let query = [0.0f32; 9];
        assert!(rank_topk(&query, &[], 5).is_empty());
    }
}
