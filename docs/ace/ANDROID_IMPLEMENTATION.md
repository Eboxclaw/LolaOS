# LOLA ACE Android implementation status

## Implemented

- Standalone Android Gradle project under `android/`.
- Kotlin launcher activity and Jetpack Compose screens for Conversas, Modelos and Diagnóstico.
- Bundled model catalogue loaded from the canonical repository manifest at `models/catalog.v1.json`.
- App-private model staging and revision directories.
- Resumable HTTPS Range downloads with cancellation, progress and byte-range validation.
- Incremental SHA-256 verification before installation and atomic file promotion.
- Atomic installed-model registry and model/partial-file deletion off the UI thread.
- Backend-neutral Kotlin `LocalInferenceBackend` contract.
- CPU-only ARM64 reference backend pinned to llama.cpp `b11960` / commit `8642541b255bfb5dc39cd351a178be09013a55d6`.
- Uses `llama-common` Jinja chat-template rendering and common sampling APIs, rather than the simpler C chat-template formatter.
- One llama context and sampler per inference session; full supplied conversation is evaluated per turn in this initial baseline.
- Kotlin streaming adapter, host-provided output-token budget, cancellation and Unicode-safe JNI text conversion.
- Chat UI connected to real local backend state and streamed token output; no mock responses.
- Native monotonic-clock metrics for prompt/output tokens, prefill, TTFT, decode duration, decode tokens/s and total generation time, shown in Diagnostics.
- Private SQLite conversation store with multiple conversations, automatic titles, ordered message history, partial assistant-response persistence on cancellation and confirmed conversation deletion.
- Android unit tests for model-download policy, HTTP range parsing and inference configuration.
- Android CI builds the ARM64 native library, runs JVM unit tests and assembles a debug APK; Rust CI remains separate.

## Not yet validated

- No physical Pixel 8 Pro (or any phone) execution has yet proven successful LFM3 generation. **Emulator-validated 2026-11-22** on an ARM64 AVD ([BENCHMARK_RECORD_EMULATOR_V1](BENCHMARK_RECORD_EMULATOR_V1.md)): full download→interrupt→resume→SHA-256→install path, golden pt-BR suite outputs (seed-42 byte-reproducible), native TTFT/prefill/decode timings, cancellation with partial persistence, airplane-mode chat, restart persistence, delete-protection while resident. Emulator numbers are not device truth.
- Conversation history is persisted locally but not encrypted by an app-specific encryption layer; there is no cloud sync.
- The reference chat context is capped at 2,048 tokens. Long conversations may need a new conversation; automatic history compaction is not implemented.
- Load/unload stress, cancellation-under-load device test and low-memory recovery test remain unexercised; model deletion was not exercised (only its protection while resident).
- Native timing has been sampled on the emulator only; PSS, CPU frequency, temperature, battery and thermal measurements remain unavailable (not applicable in a guest).
- Known defects carried from the emulator run: navigating away mid-generation desyncs the chat screen (no assistant message persisted); the model's `<think>` block leaks into the visible reply ending with a literal `</think>`.
- No incremental KV-cache reuse, WorkManager/foreground download service or Kotlin↔Rust bridge.
- No LOLA Native Engine yet; llama.cpp remains a replaceable reference backend only.

A successful native build proves that the Android library compiles, not that a particular GGUF model can be loaded and answer prompts on the target device. The transfer-cancel inversion found live during the emulator run (PR #16) — which had silently broken every download — is the case in point.

## Immediate validation sequence

1. Pass Android native build and JVM tests in GitHub Actions.
2. Install the debug APK on a Pixel 8 Pro and download the pinned GGUF through the app.
3. Load LFM3 using the 2K-token CPU reference profile and run fixed Portuguese prompts.
4. Verify streamed output, cancellation, repeat generation, unload/reload and deletion protection.
5. Measure TTFT, prefill, decode tokens/s, peak PSS and thermal state before enabling any acceleration.
6. Only then introduce the Kotlin↔Rust bridge and replace the reference backend with LOLA Native Engine.
