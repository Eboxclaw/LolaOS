# LOLA Mobile-First — Deployment, Budgets, De-Risking

Companion to `docs/ENGINE.md` + `docs/NANO_RIPL.md`. ADR-025/026/027.
Research round: Nov 2026 (sources inline).

## 1. First principles

**The phone runs the trading loop.** This is feasible *precisely because*
LOLA is decision-first: the loop is a d1 prefill burst per closed candle
(milliseconds of compute — thermally trivial), not a streaming chat. The
published phone-inference data kills "always-streaming agents" (iPhone
16 Pro: −44% decode in ~2 min; Galaxy S24 Ultra: hard GPU thermal failure
at 78.3°C; phones sustain ~5–10 heavy queries/hour —
[arXiv:2603.23640](https://arxiv.org/html/2603.23640v1)) — and LOLA's duty
cycle sits far below that line. Agent chat/voice = user-initiated bursts
under a foreground session; the loop = WorkManager ticks + a checkpointed
foreground service. Laptop/PC later runs the same core with headroom for
backtests and discovery.

## 2. Lifecycle rules (Android 14–17)

- **In-process everything.** The phantom-process killer (Android 12+,
  32-child cap) kills forked children — Termux's "signal 9" story
  ([termux#2366](https://github.com/termux/termux-app/issues/2366)). One
  process, native threads only.
- **FGS caps**: `dataSync`/`mediaProcessing` foreground services are
  6h/24h-capped on Android 15+; Android 16 extends quotas to jobs started
  from an FGS ([FGS changes](https://developer.android.com/develop/background-work/services/fgs/changes)).
  Consequence: the loop checkpoints and resumes by design
  (`lifecycle.rs`); the ledger already holds resume state — the S0 design
  pays off here.
- **Reference pattern**: Google AI Edge Gallery runs inference in a
  foreground service, in-process ([repo](https://github.com/google-ai-edge/gallery)).

## 3. Memory budget (the Android 17 law)

Android 17's **Memory Limiter** (cgroup-based, rolling out from Pixel)
meters **anonymous RSS + swap** and kills apps over budget even with free
RAM; **file-backed pages don't count**
([announcement](https://android-developers.googleblog.com/2026/08/app-broader-memory-limits.html),
[AOSP](https://source.android.com/docs/core/perf/memory-limiter)). Exact
thresholds unpublished — design to ≤ ~2.5 GB anon on 12 GB+ devices.

| Component | ~Size | Anon? |
|---|---|---|
| d1(-Omni) weights, repacked, mmap | 1.0–1.9 GB | **No** (file-backed) |
| LFM3-Omni Q4 weights + mmproj, mmap | ~1.4 GB | **No** |
| Slot B KV @32K (q8; only 6/24 layers long) | ~0.1–0.3 GB | Yes |
| Slot A prefill peak (transient) | ~0.02–0.06 GB | Yes (peak) |
| Arenas/sampler/codec per model | ~0.15–0.3 GB | Yes |
| **Total** | **~3.5–4 GB** | **anon ≤ ~1 GB** ✅ |

`mlock` is effectively unavailable (64 KB RLIMIT) — design for
reclaim-ability: weight pages may be evicted and re-faulted; K6 measures
re-fault stalls (prototype P4).

## 4. Compute placement (per chip)

| Workload | Snapdragon (Adreno+Hexagon) | Tensor (Immortalis+TPU) | Mid-range |
|---|---|---|---|
| d1 prefill burst | **our CPU kernels**, 2–3 pinned big cores | CPU | CPU |
| Agent decode | CPU floor 8–15 tok/s; Hexagon NPU 47–52 tok/s (2B-class, [snapdragon README](https://github.com/ggml-org/llama.cpp/blob/master/docs/backend/snapdragon/README.md)) **if** LFM3-omni passes there — unverified | **CPU only** (Pixel Vulkan historically loses to CPU [#829](https://github.com/ggml-org/llama.cpp/issues/829); TPU unreachable from our stack [#23397](https://github.com/ggml-org/llama.cpp/issues/23397)) | CPU |
| Codec + towers | CPU (always) | CPU | CPU |
| Concurrency | slot A bursts preempt slot B's cores; K6 scheduler owns ordering | serialized bursts | serialized |

Corrections vs earlier docs: Vulkan is *not* a Tensor phone story; the
Vulkan tier (K7) is for **AMD laptops** (and Tensor Mali at best).

## 5. De-risking prototypes (S2m — before any NPU promises)

- **P1 — CPU dual-resident harness** (any device): both slots resident via
  the oracle; measure steady tok/s, tok/s *during* a slot-A burst, anon
  RSS over 30 min, cold/warm load. Kills or confirms the concurrency math.
- **P2 — Hexagon-in-APK smoke**: package per-chip libs in jniLibs,
  `ADSP_LIBRARY_PATH` → `nativeLibraryDir`, retail (non-root) device.
  No public ggml-hexagon-in-APK success exists yet
  ([QAIRT packaging docs](https://docs.qualcomm.com/doc/80-63442-10/topic/dsp_runtime.html)).
- **P3 — LFM on Hexagon**: LFM2 → LFM3-Omni op coverage + sustained decode
  with non-burst DCVS profile (`default` currently aliases `burst` —
  silent throttling trap).
- **P4 — mmap-eviction stress**: system pressure vs decode stall spikes.
- **P5 — thermal envelope**: 30–60 min realistic duty across CPU /
  NPU+CPU configs; log thermal headroom + tok/s decay.

## 6. Stack (locked, ADR-026/027)

Compose Multiplatform UI (Android target *is* Jetpack Compose; desktop
door free) → **UniFFI** Kotlin bindings (one set serves ART + desktop JVM;
batch calls; explicit `cancel()` across FFI) → **Rust core in-process**
(everything: engine, memory, harness). **WASM: never for inference**;
reserved as a `wasmi` sandbox for untrusted third-party skills later.
iOS: not designed-for.
