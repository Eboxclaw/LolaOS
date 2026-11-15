//! SP2 — the RAM decision-cache hot map (ADR-041 R2, docs/PIPELINE.md).
//!
//! Hits never touch SQLite: a bounded LRU keyed by the full
//! 9-dependency cache key (ADR-036 — a hit requires *every* dependency
//! to match exactly, so version bumps invalidate by prefix). Hit path
//! gate: < 5 µs (criterion `cache_hit_hot`; expected well under 1 µs).
//! Hit/miss counters feed the retrieval metrics that already exist.

use std::collections::HashMap;

/// One dependency dimension of the cache key (names mirror
/// `retrieval::CACHE_DEPENDENCIES`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dependency {
    FeatureSchemaVersion,
    QuestionSchemaVersion,
    StrategyVersion,
    ModelCheckpoint,
    AdapterVersion,
    PolicyVersion,
    Instrument,
    Timeframe,
}

/// The full cache key: all 9 dependencies, exact match required.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub snapshot_hash: String,
    pub feature_schema_version: String,
    pub question_schema_version: String,
    pub strategy_version: String,
    pub model_checkpoint: String,
    pub adapter_version: String,
    pub policy_version: String,
    pub instrument: String,
    pub timeframe: String,
}

impl CacheKey {
    /// Does this key match on one dependency dimension (for prefix
    /// invalidation)?
    fn matches(&self, dep: Dependency, value: &str) -> bool {
        match dep {
            Dependency::FeatureSchemaVersion => self.feature_schema_version == value,
            Dependency::QuestionSchemaVersion => self.question_schema_version == value,
            Dependency::StrategyVersion => self.strategy_version == value,
            Dependency::ModelCheckpoint => self.model_checkpoint == value,
            Dependency::AdapterVersion => self.adapter_version == value,
            Dependency::PolicyVersion => self.policy_version == value,
            Dependency::Instrument => self.instrument == value,
            Dependency::Timeframe => self.timeframe == value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub invalidations: u64,
}

/// Bounded LRU. Default capacity 4096 entries — a scan-on-overflow
/// eviction is O(capacity) and only runs when inserting at capacity,
/// which keeps the hit path allocation-free and branch-cheap.
pub struct DecisionCache<V> {
    entries: HashMap<CacheKey, (V, u64)>,
    capacity: usize,
    stamp: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
    invalidations: u64,
}

impl<V: Clone> DecisionCache<V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::with_capacity(capacity),
            capacity,
            stamp: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
            invalidations: 0,
        }
    }

    /// Lookup: updates recency, counts a hit or miss.
    pub fn get(&mut self, key: &CacheKey) -> Option<V> {
        match self.entries.get_mut(key) {
            Some((value, stamp)) => {
                self.stamp += 1;
                *stamp = self.stamp;
                self.hits += 1;
                Some(value.clone())
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    /// Insert; at capacity, evicts the least-recently-stamped entry.
    pub fn insert(&mut self, key: CacheKey, value: V) {
        if self.entries.len() >= self.capacity && !self.entries.contains_key(&key) {
            if let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, (_, stamp))| *stamp)
                .map(|(k, _)| k.clone())
            {
                self.entries.remove(&oldest);
                self.evictions += 1;
            }
        }
        self.stamp += 1;
        self.entries.insert(key, (value, self.stamp));
    }

    /// Invalidate every entry matching one dependency dimension (a
    /// version bump). Bounded scan — rare and cheap.
    pub fn invalidate(&mut self, dep: Dependency, value: &str) -> usize {
        let before = self.entries.len();
        self.entries.retain(|k, _| !k.matches(dep, value));
        let removed = before - self.entries.len();
        self.invalidations += removed as u64;
        removed
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
            invalidations: self.invalidations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(snapshot: &str, policy: &str, instrument: &str) -> CacheKey {
        CacheKey {
            snapshot_hash: snapshot.into(),
            feature_schema_version: "market-state-v1".into(),
            question_schema_version: "tick-decision-v1".into(),
            strategy_version: "momentum-v1@1".into(),
            model_checkpoint: "d1-omni-2b@q8".into(),
            adapter_version: "0.1".into(),
            policy_version: policy.into(),
            instrument: instrument.into(),
            timeframe: "5m".into(),
        }
    }

    #[test]
    fn hits_and_misses_are_counted() {
        let mut cache: DecisionCache<u32> = DecisionCache::new(16);
        let k = key("h1", "p1", "BTC");
        assert_eq!(cache.get(&k), None);
        cache.insert(k.clone(), 7);
        assert_eq!(cache.get(&k), Some(7));
        let stats = cache.stats();
        assert_eq!((stats.hits, stats.misses), (1, 1));
    }

    #[test]
    fn every_dependency_must_match_exactly() {
        let mut cache: DecisionCache<u32> = DecisionCache::new(16);
        cache.insert(key("h1", "p1", "BTC"), 1);
        // One field differs ⇒ miss (this is the ADR-036 invariant).
        assert_eq!(cache.get(&key("h2", "p1", "BTC")), None);
        assert_eq!(cache.get(&key("h1", "p2", "BTC")), None);
        assert_eq!(cache.get(&key("h1", "p1", "ETH")), None);
        assert_eq!(cache.get(&key("h1", "p1", "BTC")), Some(1));
    }

    #[test]
    fn policy_bump_invalidates_the_prefix() {
        let mut cache: DecisionCache<u32> = DecisionCache::new(16);
        cache.insert(key("h1", "p1", "BTC"), 1);
        cache.insert(key("h2", "p1", "ETH"), 2);
        cache.insert(key("h3", "p2", "BTC"), 3);
        let removed = cache.invalidate(Dependency::PolicyVersion, "p1");
        assert_eq!(removed, 2);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get(&key("h3", "p2", "BTC")), Some(3));
    }

    #[test]
    fn capacity_evicts_least_recently_used() {
        let mut cache: DecisionCache<u32> = DecisionCache::new(2);
        cache.insert(key("a", "p", "BTC"), 1);
        cache.insert(key("b", "p", "BTC"), 2);
        // Touch "a" so "b" becomes the LRU.
        let _ = cache.get(&key("a", "p", "BTC"));
        cache.insert(key("c", "p", "BTC"), 3);
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get(&key("b", "p", "BTC")), None, "b evicted");
        assert_eq!(cache.get(&key("a", "p", "BTC")), Some(1));
        assert_eq!(cache.get(&key("c", "p", "BTC")), Some(3));
        assert_eq!(cache.stats().evictions, 1);
    }
}
