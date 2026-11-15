# LOLA Optimizations — the benchmark-gated fruit list

ADR-038. Rule: **no optimization lands without its gate.** Gates are not
always speed — quality, memory, energy, TTFT and correctness gates are
first-class. Research round 2026-11-15 (sources + measured micro-benchmarks
on this machine, Apple M4 single P-core, clang -O3).

## The measured floor (why fruit #1 is first)

- 64K×128 f32 w·V axpy (32 MiB stream): **428 µs auto-vectorized ≈ 77 GB/s**
  — the DRAM floor. Scalar-only: 459 µs (barely worse — layout beats
  vectorization here).
- LOLA's global attention @64K: 123 ms/token vs ~1.5 GiB streamed per token
  across 6 layers ⇒ **~1 GB/s effective** → **10–20× headroom** before any
  quantization. A dim-chunked register variant with strided reads measured
  **3× slower** (1130 µs) — do not chunk across rows.

## Ranked fruits (impact × certainty ÷ effort)

| # | Fruit | Gate(s) — metric, method, success | Est. magnitude | Effort |
|---|---|---|---|---|
| **F1** | Restructure global-attn w·V: register-promoted 128-dim accumulator across tokens, sequential V streaming, parallelize across the 4 KV heads | speed: µs/token @4K/32K/64K + derived GB/s vs 77 GB/s floor; **quality**: next-token KL vs f32 ≤ 1e-4, greedy golden token-identity ≥ 99.5% (fma reassociation!) | 8–20× @64K (→ ~6–15 ms), 1.5–2.5× @4K | M |
| **F2** | KV q8_0 for the 6 global layers | **memory**: RSS @64K −73% (1.5 GiB → ~400 MiB); **quality**: ppl Δ ≤ +0.5% on fixed corpus + **needle-in-haystack probe @32K/64K** (retrieval collapse is the failure mode ppl misses) + golden drift | ~2× extra speed post-F1; −73% memory | M |
| **F3** | Prefix-KV reuse + session persistence (`cache_prompt` semantics; persist global-layer KV **separately** from SWA ring state) | **TTFT**: scripted 20-turn session, warm resume < 20% of cold; **correctness**: resumed continuation token-identical to uninterrupted run (hash the KV) | ~7× TTFT on chat resume | M |
| **F4** | SQLite pack: `synchronous=NORMAL`, `prepare_cached`, batch 16-question decision writes in one txn, move `wal_autocheckpoint` off the decode thread | speed: decision write 133 µs → < 30 µs; **p99 gate**: 10K-write loop with concurrent decode, p99 < 100 µs, no >1 ms stall (checkpoints live in the tail, not the median) | ~4× write, tail stability | S |
| **F5** | NEON polynomial exp (round + deg-6 exp2 minimax + bit-trick scale; `vexpq_f32` is **not** in Apple's headers — verified) fused into the softmax block | **quality**: softmax rel drift ≤ 1e-5 vs `expf` (measured 9.4e-7); speed: block µs 4–8×. Honest: ~0.4% of 123 ms on Apple — **matters on Android** (bionic expf slower; est. 10–30% of attn @64K) | 4.3× on the exp block | S–M |
| **F6** | d1 prefill: cache the 5K-token **state KV as a prefix**, re-prefill only question suffixes (state unchanged across the ≤16-question set); q8 weights for prefill | **quality**: golden exact-match ≥ 99% vs full prefill (summarizing state changes the input distribution — expect the gate to veto it); **TTFT**: decision e2e p50/p95 over 100 calls | 753 ms → < 100 ms steady-state | M |
| **F7** | Prompt-lookup (n-gram) speculative decoding for slot B — no model changes, provably distribution-lossless with greedy verify | **accept-rate** ≥ 1.5 accepted tokens on our agent workload mix *before enabling*; e2e tok/s; greedy golden token-identity = 100%. Needs a multi-token verify path through conv+SWA state | 2–4× on grounded/tool-call text; ~1× novel | M |
| **F8** | Persistent worker pool, spin-µs-then-yield (no per-tick spawn) | tok/s + inter-token p50/p99; **energy**: µJ/token must not regress at idle | ~5% Android | S |
| **F9** | Core placement: Android `sched_setaffinity` big cores; Apple **QoS class, not affinity** (THREAD_AFFINITY_POLICY is a no-op) | **sustained** tok/s over 10 min within 10% of initial; µJ/token unchanged — thermal erosion is the failure mode | 10–30% Android | S |
| **F10** | Weight-mmap warm-up touch pass + `MADV_WILLNEED`; 16 KB-page alignment (Play requirement) regardless | **cold-start**: process→first-token before/after; page-fault count via perfetto/Instruments | −30–60% cold TTFT | S |
| **F11** | KV layout formalization: per-head SoA, head-dim contiguous for QK dot; V token-major for the axpy; 64 B alignment; false-sharing padding | same gate as F1 + a layout-only A/B; golden token-identity | 1.05–1.3×, *enables* F1/F2 | S |
| **F12** | **Anti-fruit, recorded:** layer-skip self-speculation (SLED/CLaSp/LayerSkip) — transformer-homogeneous literature, no evidence for conv+SWA hybrids, LayerSkip needs training-time dropout. Do not attempt without new evidence | — | ~0 | L |

## Harness gaps (criterion can't measure these today)

1. **Quality** — fixed agent-typical corpus: greedy token-identity %, next-token KL, ppl delta, **needle-in-haystack @32K/64K**. Required *before* F2/F5/F6. Cheapest: extend the golden-set harness (`lola golden`) with a quality mode.
2. **Energy** — µJ/token via `powermetrics` integration (macOS) / ODPM rails via perfetto (Android). Spin-waiting (F8) trades battery for latency — must verify.
3. **Thermal/sustained** — 10–15 min steady-state protocol; gate: ≥90% of initial tok/s.
4. **Tail latency** — 10K-write loop with concurrent decode; p99/p99.9 (SQLite checkpoints hide in tails).
5. **TTFT/resume** — scripted multi-turn session harness (cold vs warm) — the F3/F6 gates.
6. **Android parity** — an engine `bench` subcommand (tok/s, µJ/token, steady-state) instead of criterion-on-device.

Order of attack: **F1+F11 → F2 → F4 → F3/F6 → F5(Android) → F7**, with
F1a+F1b ✅ (3.7× @64K, 123→33.5 ms; 4K 1.84→0.96 ms) and F5's exp half ✅
(fused into F1b; the standalone-Android measurement still pending),
harness gap #1 built *first* (it gates F2/F5/F6).
