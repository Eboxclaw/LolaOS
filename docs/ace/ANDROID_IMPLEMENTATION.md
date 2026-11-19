# LOLA ACE Android implementation status

## Implemented in the initial application shell

- Standalone Android Gradle project under `android/`.
- Kotlin launcher activity and Jetpack Compose dark theme.
- Three functional navigation destinations: Conversas, Modelos and Diagnóstico.
- Bundled model catalogue loaded from the canonical repository manifest at `models/catalog.v1.json`.
- LFM3-1.7B Q4_K_M catalogue metadata displayed with revision and checksum prefixes.
- Explicit UI states showing that the artefact is not installed and no inference backend is connected.
- A GitHub Actions debug-APK build job.

## Deliberately not represented as implemented

- No model download, resume, verification of downloaded bytes or installation exists on Android yet.
- No chat generation or token streaming exists yet.
- No persistent conversations, local model registry, native bridge or device diagnostics exist yet.
- No Pixel 8 Pro runtime or performance claim is made.

The screens are an application shell, not an end-to-end local AI application.

## Next vertical slices

1. Implement Android model transfer and storage: cancellable resumable HTTP download to app-private staging, disk-space checks, byte count and SHA-256 verification, atomic promotion and restart-safe state.
2. Persist installed model metadata and conversation records separately from model files.
3. Integrate a reference inference backend and prove offline chat streaming on the Pixel 8 Pro.
4. Add the Kotlin↔Rust bridge and move model lifecycle, residency and execution control into ACE.
5. Implement the LOLA Native Engine for LFM3 after reference output fixtures and conformance tests exist.
