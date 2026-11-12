//! Full decision-tick benchmarks against the RFC-002 §10 budgets
//! (risk <1 ms; decision loop p95 <1 s — we expect orders of magnitude
//! better with no model in the path).

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lola_harness::contracts::{ExecutionMode, OrderIntentV1, OrderType, Side, TimeInForce};
use lola_harness::decision_loop::{run_tick, LoopConfig, PaperPortfolio, TickInput};
use lola_harness::features::{Bar, MIN_BARS};
use lola_harness::metrics::StageMetrics;
use lola_harness::risk::{evaluate, PortfolioView, RiskLimits};
use lola_memory::ledger::LedgerWriter;
use uuid::Uuid;

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

fn intent(decision_id: &str, qty: f64, price: f64) -> OrderIntentV1 {
    OrderIntentV1 {
        contract_version: "1.0".into(),
        decision_id: decision_id.into(),
        instrument_id: "PAPER:BTC-PERP".into(),
        action: Side::Buy,
        order_type: OrderType::Market,
        quantity: format!("{qty:.4}"),
        limit_price: Some(format!("{price:.2}")),
        time_in_force: TimeInForce::Gtc,
        reduce_only: false,
        requested_leverage: Some(1),
        execution_mode: ExecutionMode::Paper,
    }
}

fn view(price: f64) -> PortfolioView {
    PortfolioView {
        cash: 10_000.0,
        position_qty: 0.0,
        avg_entry_price: 0.0,
        session_start_equity: 10_000.0,
        realized_pnl_today: 0.0,
        peak_equity: 10_000.0,
        price,
        spread_bps: 2.0,
        data_age_ns: 1_000_000_000,
        venue_ok: true,
        kill_switch: false,
        seen_idempotency_keys: 0,
        total_idempotency_keys: 0,
    }
}

fn bench_tick(c: &mut Criterion) {
    // Ring-0 alone: budget <1 ms.
    c.bench_function("risk_all_15_checks", |b| {
        let order = intent("bench", 5.0, 100.0);
        let v = view(100.0);
        let limits = RiskLimits::default();
        b.iter(|| {
            let report = evaluate(
                black_box(&order),
                black_box(&v),
                black_box(&limits),
                "risk-1",
            );
            black_box(report.accepted);
        })
    });

    // Feature computation over a 60-bar window.
    let bars = rising_bars(60);
    c.bench_function("features_60_bars", |b| {
        b.iter(|| {
            let features = lola_harness::features::compute(black_box(&bars), 2.0);
            black_box(features)
        })
    });

    // The full tick with ledger writes (real SQLite, WAL).
    let path = std::env::temp_dir().join(format!("lola-tickbench-{}.db", Uuid::new_v4()));
    let mut writer = LedgerWriter::open(&path).expect("open");
    writer
        .register_paper_instrument("paper", "PAPER:BTC-PERP", "BTC-PERP")
        .expect("instrument");
    writer
        .begin_session("bench", "paper_trading")
        .expect("session");
    let config = LoopConfig::new("bench", "PAPER:BTC-PERP");
    let mut portfolio = PaperPortfolio::funded(10_000.0);
    let mut metrics = StageMetrics::new();

    let mut tick_bars = rising_bars(400);
    let mut next = MIN_BARS;
    c.bench_function("full_tick_with_ledger", |b| {
        b.iter(|| {
            if next + 1 >= tick_bars.len() {
                // Extend the synthetic history so every iteration is fresh.
                let start = tick_bars.last().unwrap().close_time_ns;
                let base = tick_bars.len();
                for i in 0..200 {
                    let close = 100.0 + ((base + i) as f64).sin() * 1.0 + (base + i) as f64 * 0.5;
                    tick_bars.push(Bar {
                        open: close,
                        high: close + 0.2,
                        low: close - 0.2,
                        close,
                        volume: if (base + i) % 10 == 9 { 1600.0 } else { 1000.0 },
                        close_time_ns: start + (i as i64 + 1) * 300_000_000_000,
                    });
                }
                next = 200;
            }
            next += 1;
            let input = TickInput {
                bars: &tick_bars[..next],
                spread_bps: 2.0,
                now_ns: tick_bars[next - 1].close_time_ns + 1_000_000_000,
            };
            let summary = run_tick(
                black_box(&config),
                black_box(&mut writer),
                black_box(&mut portfolio),
                black_box(&input),
                black_box(&mut metrics),
            )
            .expect("tick");
            black_box(summary.decision_id);
        })
    });

    drop(writer);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

criterion_group!(benches, bench_tick);
criterion_main!(benches);
