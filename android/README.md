# LOLA ACE Android application

This is the first native Android application shell for LOLA ACE.

## Requirements

- JDK 17
- Android SDK Platform 36
- Android SDK Build Tools 36.0.0
- Gradle 9.5.0
- Android Studio with support for Android Gradle Plugin 9.3.0

The repository currently does not include the Gradle Wrapper JAR. Install Gradle 9.5.0 locally and build from the repository root:

```bash
gradle -p android :app:testDebugUnitTest :app:assembleDebug
```

GitHub Actions provisions the pinned Gradle version and Android SDK components for the same build.

## Current screens

- **Conversas:** honest empty chat state. There is no inference backend yet, so no simulated model responses or enabled send action.
- **Modelos:** reads the same `models/catalog.v1.json` asset used by the Rust `ace-models` crate. It displays the pinned LFM3-1.7B GGUF Q4_K_M metadata and clearly marks it as catalogued, not installed.
- **Diagnóstico:** reports actual integration status; backend, resident model and runtime measurements are currently unavailable.

## Model manager

See [Android model manager implementation](../docs/ace/ANDROID_MODEL_MANAGER.md) for transfer, verification, storage and recovery behaviour.

## Architecture boundary

Kotlin/Compose owns the Android UI and platform lifecycle. The model catalogue JSON is bundled as an asset from the repository's canonical `models/` directory; it is not duplicated in the Android source tree.

Implemented in the model manager:

- resumable HTTPS downloads into app-private staging, cancellation and progress;
- SHA-256 verification and atomic installation;
- atomic installed-model registry and model/partial-file deletion.

Still not implemented:

- guaranteed background transfer after process death (partial files resume when the app is reopened);
- persistent conversations and chat history;
- Rust JNI/UniFFI bridge;
- reference or native inference backend;
- Pixel device tests, benchmarks or thermal measurements.

Those are separate implementation slices and must not be inferred from this UI scaffold.
