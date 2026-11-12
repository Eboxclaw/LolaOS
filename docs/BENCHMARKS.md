# LOLA Benchmarks — first recorded numbers (S1, 2026-11-12)

Hardware: dev laptop (Apple-silicon class). Criterion, default config
(`sample_size` 30–100). Budgets from `docs/RFC-002.md` §10 — these are
**budgets, not promises**; the point of recording is catching regressions.

## Decision-loop path (no model — the A0 baseline shape)

| Benchmark | Time | Budget | Headroom |
|---|---|---|---|
| `risk_all_15_checks` (full ring-0 evaluation) | **1.23 µs** | < 1 ms | ~800× |
| `features_60_bars` (full feature computation, 60-bar window) | **83 ns** | — | — |
| `full_tick_with_ledger` (features → signature → snapshot → strategy → risk → order+fill writes) | **169 µs** | loop p95 < 1 s | ~6,000× |

Takeaway: with no model in the path, the loop costs **microseconds** —
the entire latency budget is available for d1 prefill bursts (S2+) and
retrieval (below). The "decision-first is thermally trivial on a phone"
thesis (MOBILE_FIRST.md §1) now has numbers behind it.

## Retrieval @ 100k snapshots + 100k decisions (S4-scale seed)

| Benchmark | Time | Budget | Note |
|---|---|---|---|
| `signature_lookup` (exact bucket via index) | **2.48 µs** | — | index does all the work |
| `bucket_knn_top8` (narrow to 512 candidates, rank, top-8) | **7.99 ms** | < 10 ms total | ⚠️ only 20% margin — see below |
| `decision_insert` (snapshot + decision row write) | **133 µs** | — | WAL write cost per tick |

⚠️ **`bucket_knn_top8` is within budget but too close to it.** The proxy
fetches 512 `features_json` TEXT payloads and ranks on a byte-length
distance. The real S4 shape stores normalized feature vectors as numeric
columns (or a compact side table), which removes the JSON fetch entirely —
expected well under 2 ms. This measurement directly informs MEMORY.md
optimization consideration #6 (kNN placement). Until S4 lands the numeric
layout, treat 8 ms as the honest pre-engine ceiling.

## What is not measured yet (and why)

- **d1 decision latency** — no model loaded until S2 (oracle bring-up).
- **Agent streaming tok/s** — S2/S3.
- **Android device numbers** — everything above is laptop-class; S2m P1
  collects the phone baseline (anon RSS, thermal, tok/s during bursts).
- **Compaction job cost** — S4.

## Regression policy

`cargo bench --workspace` is the canonical run; significant regressions
against this table are CI-relevant once a bench runner is wired (Later).
