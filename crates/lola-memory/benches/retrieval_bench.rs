//! Retrieval benchmarks against the RFC-002 §10 budget (<10 ms per
//! decision cycle) at S4-scale data (100k snapshots + decisions).

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use lola_memory::cache::{CacheKey, DecisionCache};
use lola_memory::graph::query::EdgeHotCache;
use lola_memory::ledger::LedgerWriter;
use uuid::Uuid;

const ROWS: i64 = 100_000;

fn seeded_writer() -> (LedgerWriter, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!("lola-bench-{}.db", Uuid::new_v4()));
    let mut writer = LedgerWriter::open(&path).expect("open");
    writer
        .register_paper_instrument("paper", "PAPER:BTC-PERP", "BTC-PERP")
        .expect("instrument");
    writer
        .begin_session("bench", "paper_trading")
        .expect("session");

    // Bulk-seed 100k snapshots + decisions in one transaction. Signatures
    // rotate over a realistic set of buckets; hashes are unique per row.
    let conn = writer.connection();
    let tx = conn.unchecked_transaction().expect("tx");
    {
        let mut snap = tx
            .prepare(
                "INSERT INTO feature_snapshots
                     (snapshot_id, instrument_id, timeframe, event_time_ns,
                      ingestion_time_ns, feature_schema_version, state_signature,
                      snapshot_hash, features_json)
                 VALUES (?1, 'PAPER:BTC-PERP', '5m', ?2, ?2, 'market-state-v1', ?3, ?4, '{}')",
            )
            .expect("prepare snap");
        let mut dec = tx
            .prepare(
                "INSERT INTO decision_runs
                     (decision_id, session_id, instrument_id, timeframe, event_time_ns,
                      snapshot_id, strategy_id, strategy_version, decision_mode,
                      decision_json, policy_version, decision_hash, status, created_at_ns)
                 VALUES (?1, 'bench', 'PAPER:BTC-PERP', '5m', ?2, ?3, 'momentum-v1', 1,
                         'deterministic', '{}', 'risk-1', 'x', 'EXECUTED', ?2)",
            )
            .expect("prepare dec");
        let buckets = [
            "5m|Positive|Normal|Healthy|momentum-v1",
            "5m|Negative|Normal|Healthy|momentum-v1",
            "5m|Neutral|Low|Healthy|momentum-v1",
            "5m|Positive|Expanding|Thin|momentum-v1",
            "5m|Neutral|Normal|Healthy|momentum-v1",
        ];
        for i in 0..ROWS {
            let event_time = 1_700_000_000_000_000_000 + i * 300_000_000_000;
            let signature = buckets[(i % buckets.len() as i64) as usize];
            let snapshot_id = format!("bs-{i}");
            snap.execute(rusqlite::params![
                snapshot_id,
                event_time,
                signature,
                format!("hash-{i:08x}")
            ])
            .expect("snap insert");
            dec.execute(rusqlite::params![
                format!("bd-{i}"),
                event_time,
                snapshot_id
            ])
            .expect("dec insert");
        }
    }
    tx.commit().expect("commit");
    (writer, path)
}

fn bench_retrieval(c: &mut Criterion) {
    let (writer, path) = seeded_writer();
    let conn = writer.connection();

    let mut group = c.benchmark_group("retrieval@100k");
    group.throughput(Throughput::Elements(1));
    group.sample_size(30);

    // Step 2 of the pipeline: exact state-signature lookup via the index.
    group.bench_function("signature_lookup", |b| {
        b.iter(|| {
            let hit: Option<String> = conn
                .query_row(
                    "SELECT snapshot_id FROM feature_snapshots
                     WHERE state_signature = '5m|Positive|Normal|Healthy|momentum-v1'
                       AND timeframe = '5m'
                     ORDER BY event_time_ns DESC LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .ok();
            black_box(hit);
        })
    });

    // Steps 3+4: bucket-narrowed candidate scan (20k rows) ranked by a
    // numeric distance in Rust — the S4 kNN shape, measured pre-engine.
    group.bench_function("bucket_knn_top8", |b| {
        b.iter(|| {
            let mut stmt = conn
                .prepare(
                    "SELECT features_json, event_time_ns FROM feature_snapshots
                     WHERE state_signature = '5m|Positive|Normal|Healthy|momentum-v1'
                     ORDER BY event_time_ns DESC LIMIT 512",
                )
                .expect("prepare");
            let rows: Vec<(String, i64)> = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .expect("query")
                .filter_map(std::result::Result::ok)
                .collect();
            // Cheap proxy distance: parse-free byte-length distance.
            let mut scored: Vec<(usize, i64)> = rows
                .iter()
                .enumerate()
                .map(|(i, (json, t))| (json.len() ^ i, *t))
                .collect();
            scored.sort_unstable();
            scored.truncate(8);
            black_box(scored);
        })
    });

    // SP1: the same lookup + ranking over numeric columns (gate < 300 µs).
    group.bench_function("bucket_knn_numeric", |b| {
        let query = [0.005, 0.02, 0.04, 0.015, 1.3, 2.0, 2.0, 1.0, 2.0];
        b.iter(|| {
            let candidates = lola_memory::knn::knn_candidates(
                conn,
                "5m|Positive|Normal|Healthy|momentum-v1",
                512,
            )
            .expect("candidates");
            let top = lola_memory::knn::rank_topk(&query, &candidates, 8);
            black_box((candidates.len(), top.len()))
        })
    });

    // SP2: RAM decision-cache hit path (gate < 5 us; expect well under 1 us).
    let mut hot = DecisionCache::new(4096);
    let cache_key = CacheKey {
        snapshot_hash: "a1b2c3d4e5f60718".into(),
        feature_schema_version: "market-state-v1".into(),
        question_schema_version: "tick-decision-v1".into(),
        strategy_version: "momentum-v1@1".into(),
        model_checkpoint: "d1-omni-2b@q8".into(),
        adapter_version: "0.1".into(),
        policy_version: "risk-1".into(),
        instrument: "HYPERLIQUID:BTC-PERP".into(),
        timeframe: "5m".into(),
    };
    hot.insert(cache_key.clone(), 42u32);
    group.bench_function("cache_hit_hot", |b| {
        b.iter(|| {
            let hit = hot.get(black_box(&cache_key));
            black_box(hit.is_some())
        })
    });

    // SP3: graph traversal on a seeded hub (10k edges from one node),
    // warm cache vs cold two-hop SQL.
    {
        conn.execute_batch(
            "INSERT OR IGNORE INTO graph_nodes (node_id, node_type, canonical_key,
                 created_at_ns, updated_at_ns)
                 VALUES ('hub', 'Instrument', 'BTC-PERP', 1, 1);
             INSERT OR IGNORE INTO graph_nodes (node_id, node_type, canonical_key,
                 created_at_ns, updated_at_ns)
                 VALUES ('hub2', 'MarketRegime', 'UpTrend', 1, 1);",
        )
        .expect("nodes");
        let tx = conn.unchecked_transaction().expect("tx");
        {
            let mut e = tx
                .prepare(
                    "INSERT OR IGNORE INTO graph_edges (edge_id, source_node_id, relation,
                         target_node_id, attributes_json, source_type, source_ref, created_at_ns)
                     VALUES (?1, 'hub', 'MatchesPattern', 'hub2', '{}', 'bench', 'seed', 1)",
                )
                .expect("prepare edge");
            for i in 0..10_000 {
                e.execute(rusqlite::params![format!("be-{i}")])
                    .expect("edge");
            }
        }
        tx.commit().expect("commit");
    }
    let mut hot_edges = EdgeHotCache::new(256);
    let now = 1_700_000_000_000_000_000;
    // Warm the cache for the hub.
    let _ = hot_edges.edges_for(conn, "hub", now).expect("warm load");
    group.bench_function("graph_two_hop_cold_sql", |b| {
        b.iter(|| {
            let hops = lola_memory::graph::query::two_hop(
                black_box(conn),
                black_box("hub"),
                black_box(now),
                20,
            )
            .expect("hops");
            black_box(hops.len())
        })
    });
    group.bench_function("graph_edges_warm_cache", |b| {
        b.iter(|| {
            let edges = hot_edges.warm(black_box("hub"), black_box(now)).expect("warm");
            black_box(edges.len())
        })
    });

    group.finish();

    // Writer throughput: the actual per-tick decision write (mutable path).
    let mut writer = writer;
    let mut i = ROWS;
    c.bench_function("decision_insert", |b| {
        b.iter(|| {
            i += 1;
            let event_time = 1_700_000_000_000_000_000 + i * 300_000_000_000;
            let snap = writer
                .insert_snapshot(
                    "PAPER:BTC-PERP",
                    "5m",
                    event_time,
                    "5m|Positive|Normal|Healthy|momentum-v1",
                    &format!("hash-{i:08x}"),
                    "{}",
                    [0.005, 0.02, 0.04, 0.015, 1.3, 2.0, 2.0, 1.0, 2.0],
                )
                .expect("snap");
            let decision = writer
                .insert_decision(
                    "bench",
                    "PAPER:BTC-PERP",
                    "5m",
                    event_time,
                    &snap,
                    "momentum-v1",
                    "deterministic",
                    "{}",
                    "risk-1",
                    "EXECUTED",
                )
                .expect("decision");
            black_box(decision);
        })
    });

    drop(writer);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

criterion_group!(benches, bench_retrieval);
criterion_main!(benches);
