# LOLA ACE Android implementation status

## Implemented

- Standalone Android Gradle project under `android/`.
- Kotlin launcher activity and Jetpack Compose screens for Conversas, Modelos and Diagnóstico.
- Bundled model catalogue loaded from the canonical repository manifest at `models/catalog.v1.json`.
- LFM3-1.7B Q4_K_M catalogue metadata shown with pinned revision and SHA-256.
- App-private model staging and revision directories.
- Resumable HTTP Range downloads with cancellation and progress, strict HTTPS redirect handling, and HTTP range validation.
- Incremental SHA-256 verification before installation and atomic file promotion.
- Atomic installed-model registry and deletion of managed revisions/partials away from the UI thread.
- Android unit tests for download metadata and HTTP range parsing.
- GitHub Actions runs Android unit tests and assembles a debug APK.

## Deliberately not represented as implemented

- No LFM3 inference backend, model loading, token streaming or chat generation.
- No persisted conversations or conversation UI.
- No foreground-service / WorkManager guarantee that a download continues after Android kills the app. Partial bytes are retained and can be resumed when the app is reopened.
- No Kotlin↔Rust bridge, no runtime model-residency integration and no LOLA Native Engine.
- No Pixel 8 Pro installation, end-to-end network transfer, performance or thermal result.

A verified GGUF file is not automatically a model that the current application can execute. The inference path is still a separate milestone.

## Next vertical slices

1. Add a foreground download policy or WorkManager integration for reliable long-running transfer, and validate pause/resume on the Pixel 8 Pro.
2. Persist conversations and implement the conversation list/message history.
3. Integrate a reference LFM3 inference backend and prove offline token streaming.
4. Add the Kotlin↔Rust bridge and move lifecycle, residency and execution control into ACE.
5. Implement LOLA Native Engine for LFM3 after golden outputs and conformance tests exist.
