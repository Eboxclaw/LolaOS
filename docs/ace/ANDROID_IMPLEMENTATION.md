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
- Private SQLite conversation store with multiple conversations, automatic titles, ordered message history, partial assistant-response persistence on cancellation and confirmed conversation deletion.
- Android unit tests for model-download policy, HTTP range parsing and inference configuration.
- Android CI builds the ARM64 native library, runs JVM unit tests and assembles a debug APK; Rust CI remains separate.

## Not yet validated

- No physical Pixel 8 Pro execution has yet proven successful LFM3 generation.
- Conversation history is persisted locally but not encrypted by an app-specific encryption layer; there is no cloud sync.
- The reference chat context is capped at 2,048 tokens. Long conversations may need a new conversation; automatic history compaction is not implemented.
- No golden-output suite, load/unload stress run, cancellation-under-load device test or low-memory recovery test.
- No TTFT, prefill/decode speed, PSS, temperature, battery or thermal measurements.
- No physical Pixel 8 Pro execution, incremental KV-cache reuse, WorkManager/foreground download service or Kotlin↔Rust bridge.
- No LOLA Native Engine yet; llama.cpp remains a replaceable reference backend only.

A successful native build proves that the Android library compiles, not that a particular GGUF model can be loaded and answer prompts on the target device.

## Immediate validation sequence

1. Pass Android native build and JVM tests in GitHub Actions.
2. Install the debug APK on a Pixel 8 Pro and download the pinned GGUF through the app.
3. Load LFM3 using the 2K-token CPU reference profile and run fixed Portuguese prompts.
4. Verify streamed output, cancellation, repeat generation, unload/reload and deletion protection.
5. Measure TTFT, prefill, decode tokens/s, peak PSS and thermal state before enabling any acceleration.
6. Only then introduce the Kotlin↔Rust bridge and replace the reference backend with LOLA Native Engine.
