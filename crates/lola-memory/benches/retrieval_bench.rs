//! Retrieval benchmarks against the RFC-002 §10 budget (<10 ms per
//! decision cycle) at S4-scale data (100k snapshots + decisions).

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
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
