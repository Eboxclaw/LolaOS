# Benchmark record — reference backend on the Android emulator (v1)

> **This is not device truth.** All numbers below were measured inside an
> Android emulator on an Apple-Silicon macOS host. They validate the
> end-to-end path (download → verify → load → stream → persist) and the
> measurement harness itself. They say nothing about phone-class CPU, memory
> or thermal behaviour. The physical-device run (ANDROID_POC §7, Pixel 8 Pro
> or any ARM64 phone) remains the open gate.

Run date: 2026-11-22. Suite: `evals/ace/golden-prompts-pt-v1.json`
(9 cases). Outputs: `evals/ace/golden-outputs-emulator-v1/` (11 recorded
runs incl. the stability rerun and the cancelled generation).

## Environment

| Item | Value |
|---|---|
| Guest | AVD `LolaAce_API35` — Pixel 7, Android 15 (API 35), `google_apis/arm64-v8a`, 4 vCPU, 4 GiB RAM, 6 GiB data partition |
| Host | Apple-Silicon macOS, Android emulator 37.2.7 (build 16195039, native darwin-aarch64) |
| App build | commit `bb8990b` (= main `2328aa5` with the transfer fix PR #16, plus the Diagnostics-scroll fix PR #17) |
| Backend | llama.cpp reference `b11960` (`8642541b…`), CPU-only arm64-v8a, threads=4 |
| Model | LFM3-1.7B Q4_K_M @ HF rev `b3f2ae5d…`, SHA-256 verified in-app before install |
| Sampler | temp 0.1 · top-k 50 · repeat-penalty 1.1 · **seed 42** · context 2,048 · output cap 256 |

Method: prompts delivered through the app UI (a 30-line broadcast IME was
built for unicode fidelity — `input text` cannot type pt-BR diacritics);
completion detected by polling the app's SQLite (`assistant` row persisted);
timings read from the Diagnostics screen (native `steady_clock` metrics
merged in PR #14). No app code was modified for measurement except the two
bug fixes below, both merged before the suite ran.

## Generation metrics (native, per run)

| Run | prompt/output tokens | TTFT | prefill | decode | total |
|---|---|---|---|---|---|
| warmup (cold first generation) | 51 / 118 | — (scroll bug) | — | — | — |
| apresentacao-curta A | 56 / 100 | 34,507 ms | 34,494 ms | 1.17 tok/s | 119,718 ms |
| factual-uma-palavra | 61 / 73 | 35,881 ms | 35,866 ms | 1.23 tok/s | 95,015 ms |
| instrucao-tres-itens | 72 / 256 | 42,450 ms | 42,439 ms | 1.21 tok/s | 253,954 ms |
| aritmetica-simples | 63 / 106 | 36,996 ms | 36,987 ms | 1.24 tok/s | 122,431 ms |
| multi-turn t1 | 67 / 141 | 39,364 ms | 39,357 ms | 1.21 tok/s | 155,505 ms |
| multi-turn t2 (full history re-eval) | 121 / 146 | 71,798 ms | 71,786 ms | 1.21 tok/s | 192,560 ms |
| diacriticos-unicode | 79 / 256 | 46,162 ms | 46,154 ms | 1.20 tok/s | 259,278 ms |
| sem-ferramentas-hora | 58 / 256 | 34,782 ms | 34,775 ms | 0.88 tok/s | 325,453 ms |
| contexto-longo | **651** / 256 | 633,005 ms | 632,987 ms | 0.97 tok/s | 896,139 ms |
| apresentacao-curta B (stability) | 56 / 100 | 50,264 ms | 50,255 ms | 0.80 tok/s | 175,077 ms |
| cancelamento (partial) | 68 / 21 | 71,464 ms | — | 0.67 tok/s | 103,012 ms |

Readings:

- **Decode ≈ 0.8–1.24 tok/s** and **prefill ≈ 0.55–0.97 s/token** in this
  environment. Both are emulator-dominated (virtualised 4-vCPU guest); do
  not extrapolate to phones.
- **Seed-42 reproducibility holds**: runs A and B of `apresentacao-curta`
  produced **byte-identical output** (393 = 393 chars, sha256-equal in the
  manifest) despite different timings. The golden-corpus premise — pinned
  seed ⇒ comparable outputs across builds/backends — is validated.
- The long-context case consumed **651 prompt tokens** (template included)
  inside the 2,048 budget; multi-turn t2 re-evaluated 121 tokens of history,
  confirming the documented no-incremental-KV behaviour (prefill grows with
  history).
- Cancellation preserved a 99-char partial reply and reported 21 generated
  tokens at stop — the cancellation-safe persistence path works.

## Model acquisition — and a falsified claim, fixed

The run falsified PR #10's "resumable verified downloads" claim: the shipped
`ModelTransferManager` used `check(task.isCancelled())` (throws when **not**
cancelled), so **every download died instantly** with `Falha /
"Transferência cancelada"` and zero bytes — on any device or emulator; it
had never transferred a byte anywhere. Fixed in PR #16 (six sites inverted).

After the fix, the documented path was proven end-to-end on the emulator:

- fresh download: `320.6 MiB / 1.02 GiB` after 20 s (~16 MiB/s guest network);
- **real network-loss interruption** (airplane mode mid-transfer): honest
  error `Software caused connection abort`, partial bytes retained
  (`Apagar transferência parcial` offered);
- retry resumed **from the retained partial bytes** (counter continued at
  551.1 MiB, not zero) — HTTP Range resume validated;
- SHA-256 verified in-app → `Modelo verificado e instalado.`

## Bugs found by the run (beyond #16)

1. **Diagnostics timing cards were unreachable** — the metrics Column had no
   `verticalScroll`, so TTFT/PREFILL/DECODE/TOTAL rendered off-screen. Fixed
   in PR #17. (The warmup row above has no timings for exactly this reason.)
2. **Navigating away mid-generation desyncs the chat** — switching tabs
   during a generation left the screen showing `Parar geração` forever at
   ~0% CPU with **no assistant message persisted** (user message only in
   SQLite). Observed once (repro: send → switch tab within ~15 s → return);
   not isolated further — recorded as an open defect for the device run.
3. **`<think>` leakage** — LFM3's reasoning block is rendered raw, ending
   with a literal `</think>` tag in the user-visible bubble (seen on every
   run; e.g. warmup output). Either template or post-processing gap; a
   quality item for the app, independent of the backend.

## Behavioural evidence (emulator-scoped)

| ANDROID_POC §7 item | Emulator evidence |
|---|---|
| App builds and installs on the target Pixel | installs/launches on ARM64 emulator only — **not** a Pixel |
| Model catalogue displays LFM3-1.7B metadata | ✓ (Biblioteca shows pinned revision/SHA/size) |
| Download progress and cancellation work | ✓ progress bytes, real network-loss interrupt + resume |
| Corrupt or mismatched artefact is rejected | not exercised (would require a tampered download) |
| Verified model can be installed and selected | ✓ installed + loaded + generating |
| Model can be deleted without deleting conversations | delete-protection while resident ✓; actual deletion not exercised (avoids 1.1 GB re-download) |
| Airplane-mode chat works after installation | ✓ full reply persisted with radio off |
| Tokens stream in order and Stop cancels generation | ✓ streaming observed; Stop → partial persisted + partial metrics |
| App restart preserves conversations and model registration | ✓ force-stop/relaunch: model `Instalado e verificado`, `Histórico (15)` intact |
| Two sessions do not share mutable LFM3 cache state | not exercised (single-session app UI) |
| Diagnostics report the actual backend and measured metrics | ✓ post-#17: backend + native timings per generation |

## What this record does not establish

Phone-class TTFT/decode/memory/thermal (guest numbers are host-CPU-bound);
PSS (virtual-guest memory meaningless here); physical-radio download
behaviour; the Pixel 8 Pro acceptance gate. Those stay with A5 on hardware,
for which this run is the harness rehearsal.
