# LOLA ACE Android application

Native Android application for the LOLA ACE local-first runtime.

## Requirements

- JDK 17
- Android SDK Platform 36
- Android SDK Build Tools 36.0.0
- Android NDK 29.0.13113456
- CMake 3.31.6
- Gradle 9.5.0
- Android Studio with support for Android Gradle Plugin 9.3.0

The repository currently does not include the Gradle Wrapper JAR. Install Gradle 9.5.0 locally and build from the repository root:

```bash
gradle -p android :app:testDebugUnitTest :app:assembleDebug
```

The first native build fetches the pinned llama.cpp source commit through CMake FetchContent. GitHub Actions provisions the pinned SDK, NDK, CMake and Gradle versions.

## Current screens

- **Conversas:** loads the installed GGUF on demand, opens a local inference session and streams tokens into the chat UI. The app initially uses a 2K context and a 256-token output budget. Conversation history is currently in memory only.
- **Modelos:** reads the canonical `models/catalog.v1.json` asset, downloads to app-private storage, verifies SHA-256 and supports cancel/resume/delete. Deletion is blocked while the model is loading or resident.
- **Diagnóstico:** reports actual backend/model state. Device measurements remain unavailable until a physical-device run is performed.

## Model manager

See [Android model manager implementation](../docs/ace/ANDROID_MODEL_MANAGER.md) for transfer, verification, storage and recovery behaviour.

## Reference inference backend

See [Android reference backend](../docs/ace/ANDROID_REFERENCE_BACKEND.md) for the pinned llama.cpp revision, CMake options, session contract and limitations.

The app depends on the backend-neutral Kotlin `LocalInferenceBackend` interface. llama.cpp is a temporary reference implementation, not the LOLA ACE scheduler or native inference engine.

## Current limitations

- A model has to be installed, then explicitly loaded into memory.
- The first reference profile is CPU-only ARM64, 2K context and up to 256 generated tokens in the app.
- The full chat history is re-evaluated each turn; KV-cache reuse is not implemented.
- Downloads resume after the app is reopened but are not guaranteed to continue after Android kills the process.
- Conversations are not persisted.
- No physical Pixel 8 Pro generation or performance/thermal result is claimed.
