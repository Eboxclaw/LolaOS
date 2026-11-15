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


## S3 kernels — first engine numbers (K1, 2026-11-14)

| Benchmark | Time | Iteration note |
|---|---|---|
| `conv7_step_2048x2048` (dense interpretation) | **6.77 ms** | 19.5 ms naive → 17.3 ms contiguous/alloc-free → 6.77 ms with the NEON dot kernel |
| `conv7_depthwise_step_2048` (architecture-correct) | **8.48 µs** | O(in_ch × 7) per token — the LFM3 shape |
| `swa_step_full_window_16x4x128` (K2: SWA-256 attention core, GQA 16/4×128, window full) | **71.3 µs** | ring buffer never grows; NEON dots |
| `global_attn_step_4k_16x4x128` (K3: global attention, partial RoPE θ=1e6, GQA 16/4×128, 4K context) | **1.45 ms** | scores materialized (not yet flash-blocked); linear extrapolation to 128K ≈ 46 ms/layer ⇒ K4 fusion + blocked softmax is *required* before long contexts — the next milestone by measurement, not by preference |

**The finding that matters:** the dense interpretation is ~800× more
expensive and physically impossible — 13 conv layers at 2048×2048×7 would
be ~1.5 GB of weights, dwarfing the whole 2B budget. LFM3's gated short
conv is **depthwise** (per-channel k=7, value+gate), which is also why
only 6/24 layers carry long KV. The dense kernel stays in the codebase as
a parity/pointwise utility; the depthwise one is the shipped shape
(wiring confirmed against the checkpoint at K1 parity time). Invariants
tested for both: step == scan, strict causality, constant-size state,
SIMD-vs-scalar dot agreement. The SWA eviction test caught and fixed a
real kernel bug (stale `out` buffer accumulation) before it could ever
ship — the invariant tests are doing their job.

## What is not measured yet (and why)

- **d1 decision latency** — no model loaded until S2 (oracle bring-up).
- **Agent streaming tok/s** — S2/S3.
- **Android device numbers** — everything above is laptop-class; S2m P1
  collects the phone baseline (anon RSS, thermal, tok/s during bursts).
- **Compaction job cost** — S4.

## Agentic metric frame (measurement policy for S2m/S9 — ADR-037)

Per MLCommons Agentic Inference (2026-07) and the Nov-2026 edge study:
measure **trajectories, not tokens** — time-to-first-action, time-to-first
response token, task completion rate, tool-selection accuracy,
**argument accuracy** (the known bottleneck: 63–84% vs 93–100% selection),
end-to-end latency, peak RAM (both slots resident), energy, thermal decay
over 30–50 repeated trials. Raw tokens/s of slot B alone is explicitly
*not* a LOLA target metric.

## Regression policy

`cargo bench --workspace` is the canonical run; significant regressions
against this table are CI-relevant once a bench runner is wired (Later).
