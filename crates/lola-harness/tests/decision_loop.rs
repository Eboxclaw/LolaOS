//! S1 integration tests: the full decision loop on synthetic bars, the
//! kill switch, and the crash-recovery idempotency guarantee (ROADMAP S1
//! exit criteria).

use lola_harness::contracts::Side;
use lola_harness::decision_loop::{run_tick, LoopConfig, PaperPortfolio, TickAction, TickInput};
use lola_harness::features::{Bar, MIN_BARS};
use lola_harness::metrics::StageMetrics;
use lola_memory::ledger::LedgerWriter;

const SESSION: &str = "s1-integration";
const INSTRUMENT: &str = "PAPER:BTC-PERP";

/// Deterministic rising market with periodic volume spikes — momentum-v1
/// will find entries.
fn rising_bars(count: usize) -> Vec<Bar> {
    (0..count)
        .map(|i| {
            let close = 100.0 + (i as f64).sin() * 1.0 + i as f64 * 0.5;
            Bar {
                open: close,
                high: close + 0.2,
                low: close - 0.2,
                close,
                volume: if i % 10 == 9 { 1600.0 } else { 1000.0 },
                close_time_ns: 1_700_000_000_000_000_000 + (i as i64) * 300_000_000_000,
            }
        })
        .collect()
}

/// Falling market — triggers momentum-v1's exit (SELL reduce-only).
fn falling_bars(count: usize) -> Vec<Bar> {
    (0..count)
        .map(|i| {
            let close = 200.0 - (i as f64).sin() * 1.0 - i as f64 * 0.5;
            Bar {
                open: close,
                high: close + 0.2,
                low: close - 0.2,
                close,
                volume: 1000.0,
                close_time_ns: 1_700_000_000_000_000_000 + (i as i64) * 300_000_000_000,
            }
        })
        .collect()
}

fn writer() -> (LedgerWriter, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!("lola-s1-{}.db", uuid::Uuid::new_v4()));
    let mut w = LedgerWriter::open(&path).expect("open ledger");
    w.register_paper_instrument("paper", INSTRUMENT, "BTC-PERP")
        .expect("instrument");
    w.begin_session(SESSION, "paper_trading").expect("session");
    (w, path)
}

fn cleanup(path: &std::path::PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

#[test]
fn full_loop_trades_and_records_every_stage() {
    let (mut w, path) = writer();
    let config = LoopConfig::new(SESSION, INSTRUMENT);
    let mut portfolio = PaperPortfolio::funded(10_000.0);
    let mut metrics = StageMetrics::new();
    let bars = rising_bars(120);

    let mut fills = 0;
    for i in MIN_BARS..bars.len() {
        let input = TickInput {
            bars: &bars[..=i],
            spread_bps: 2.0,
            now_ns: bars[i].close_time_ns + 1_000_000_000,
        };
        let summary =
            run_tick(&config, &mut w, &mut portfolio, &input, &mut metrics).expect("tick survives");
        if let TickAction::Filled { .. } = summary.action {
            fills += 1;
        }
    }

    assert!(fills >= 1, "momentum entries must fire on a rising market");
    assert!(portfolio.position_qty > 0.0, "position opened");
    // Entry notional was capped at 25% of equity at fill time (the mark
    // may appreciate beyond it — that's the position working, not a breach).
    assert!(
        portfolio.position_qty * portfolio.avg_entry <= 10_000.0 * 0.26,
        "entry notional {} exceeds the concentration cap",
        portfolio.position_qty * portfolio.avg_entry
    );

    // Ledger recorded everything.
    let conn = w.connection();
    let decisions: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM decision_runs WHERE session_id = ?1",
            [SESSION],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(decisions as usize, 120 - MIN_BARS, "one decision per tick");
    let risk_checks: i64 = conn
        .query_row("SELECT COUNT(*) FROM risk_checks", [], |r| r.get(0))
        .unwrap();
    let orders: i64 = conn
        .query_row("SELECT COUNT(*) FROM paper_orders", [], |r| r.get(0))
        .unwrap();
    let fills_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM paper_fills", [], |r| r.get(0))
        .unwrap();
    assert_eq!(orders, fills_rows);
    assert!(risk_checks >= orders);

    // Every loop stage has latency metrics (S1 exit: metrics emitted).
    for stage in [
        "features",
        "signature",
        "ledger_snapshot",
        "strategy",
        "risk",
    ] {
        assert!(
            metrics.snapshot(stage).is_some(),
            "stage '{stage}' must be measured"
        );
    }

    cleanup(&path);
}

#[test]
fn kill_switch_blocks_every_order_and_is_audited() {
    let (mut w, path) = writer();
    let config = LoopConfig::new(SESSION, INSTRUMENT);
    let mut portfolio = PaperPortfolio::funded(10_000.0);
    portfolio.kill_switch = true;
    let bars = rising_bars(MIN_BARS + 5);

    let mut blocked = 0;
    for i in MIN_BARS..bars.len() {
        let input = TickInput {
            bars: &bars[..=i],
            spread_bps: 2.0,
            now_ns: bars[i].close_time_ns + 1_000_000_000,
        };
        if let TickAction::RiskRejected { failed_checks } = run_tick(
            &config,
            &mut w,
            &mut portfolio,
            &input,
            &mut StageMetrics::new(),
        )
        .expect("tick")
        .action
        {
            assert!(failed_checks.contains(&"instrument_eligibility".to_string()));
            blocked += 1;
        }
    }
    assert!(blocked >= 1, "kill switch must reject candidates");
    let orders: i64 = w
        .connection()
        .query_row("SELECT COUNT(*) FROM paper_orders", [], |r| r.get(0))
        .unwrap();
    assert_eq!(orders, 0, "kill switch ⇒ zero orders");
    cleanup(&path);
}

#[test]
fn replayed_tick_after_crash_never_double_fills() {
    let (mut w, path) = writer();
    let config = LoopConfig::new(SESSION, INSTRUMENT);
    let bars = rising_bars(80);

    // Drive until at least one fill.
    let mut portfolio = PaperPortfolio::funded(10_000.0);
    let mut metrics = StageMetrics::new();
    let mut last_input_index = MIN_BARS;
    for i in MIN_BARS..bars.len() {
        last_input_index = i;
        let input = TickInput {
            bars: &bars[..=i],
            spread_bps: 2.0,
            now_ns: bars[i].close_time_ns + 1_000_000_000,
        };
        if let TickAction::Filled { .. } =
            run_tick(&config, &mut w, &mut portfolio, &input, &mut metrics)
                .expect("tick")
                .action
        {
            break;
        }
    }

    // "Crash": a fresh process replays the same bar with a fresh
    // portfolio — the ledger's idempotency key must demote it to a no-op.
    let mut replayed_portfolio = PaperPortfolio::funded(10_000.0);
    let replay = TickInput {
        bars: &bars[..=last_input_index],
        spread_bps: 2.0,
        now_ns: bars[last_input_index].close_time_ns + 1_000_000_000,
    };
    let summary = run_tick(
        &config,
        &mut w,
        &mut replayed_portfolio,
        &replay,
        &mut StageMetrics::new(),
    )
    .expect("replay survives");
    assert!(
        matches!(summary.action, TickAction::DuplicateSuppressed { .. }),
        "replay must suppress, got {:?}",
        summary.action
    );

    let conn = w.connection();
    let orders: i64 = conn
        .query_row("SELECT COUNT(*) FROM paper_orders", [], |r| r.get(0))
        .unwrap();
    let fills: i64 = conn
        .query_row("SELECT COUNT(*) FROM paper_fills", [], |r| r.get(0))
        .unwrap();
    assert_eq!(orders, 1, "exactly one order despite the replay");
    assert_eq!(fills, 1, "exactly one fill despite the replay");

    // Recovery state knows where we are.
    let state = lola_harness::decision_loop::recover(&w, SESSION).expect("recover");
    assert!(state.last_decision_id.is_some());
    assert_eq!(state.known_idempotency_keys.len(), 1);
    cleanup(&path);
}

#[test]
fn falling_market_exits_the_position_reduce_only() {
    let (mut w, path) = writer();
    let config = LoopConfig::new(SESSION, INSTRUMENT);
    let bars = falling_bars(MIN_BARS + 10);
    // Pre-existing position (as if bought earlier) — the exit path only.
    let mut portfolio = PaperPortfolio::funded(9_000.0);
    portfolio.position_qty = 5.0;
    portfolio.avg_entry = 190.0;

    let input = TickInput {
        bars: &bars,
        spread_bps: 2.0,
        now_ns: bars.last().unwrap().close_time_ns + 1_000_000_000,
    };
    let summary = run_tick(
        &config,
        &mut w,
        &mut portfolio,
        &input,
        &mut StageMetrics::new(),
    )
    .expect("tick");
    match summary.action {
        TickAction::Filled { side, qty, .. } => {
            assert_eq!(side, "SELL");
            assert!((qty - 5.0).abs() < 1e-6, "sells the whole position");
            assert!(portfolio.position_qty <= 1e-12, "flat after exit");
        }
        other => panic!("expected reduce-only SELL fill, got {other:?}"),
    }
    let _ = Side::Buy; // keep the import honest
    cleanup(&path);
}
