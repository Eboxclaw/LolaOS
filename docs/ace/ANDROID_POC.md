# LOLA ACE Android proof of concept

## 1. User-visible objective

A real Android application, not a CLI demo. The initial experience is a chat screen with access to a local model library.

The POC is complete only when a user can install the app, download LFM3-1.7B, disconnect the network, start a conversation, receive streamed local output, close the app and continue later.

## 2. Screens

### Chat
- conversation list and new conversation;
- message bubbles with partial streaming output;
- stop-generation control;
- selected model indicator;
- loading/error states;
- persistence across app restart.

### Models
- catalogue of supported LFM3 artefacts;
- model description, quantization, size and compatibility;
- download progress, pause/resume and retry;
- checksum/size verification;
- install/ready/error states;
- select active model;
- delete model with storage impact confirmation.

### Diagnostics
- model/backend/version;
- model load time;
- TTFT, prefill and decode throughput;
- current/peak memory where available;
- context/cache configuration;
- cancellation and error information;
- thermal state when exposed by Android.

## 3. Suggested Android module layout

```text
android/
  settings.gradle.kts
  build.gradle.kts
  gradle/libs.versions.toml
  app/
    src/main/AndroidManifest.xml
    src/main/java/.../MainActivity.kt
    src/main/java/.../ui/chat/
    src/main/java/.../ui/models/
    src/main/java/.../ui/diagnostics/
  core/
    model-catalog/
    model-download/
    model-store/
    chat-data/
    native-bridge/
```

Use Kotlin and Jetpack Compose. Use Room for conversation metadata/messages if it fits the storage boundary; keep model files in app-private storage and never store large weights in SQLite.

## 4. Model manager requirements

- pinned catalogue metadata (model ID, revision, source, expected size, SHA-256 where published, format, quantization, supported capabilities);
- resumable download with progress and network failure recovery;
- temporary download file followed by verification and atomic install;
- disk-space check before download;
- safe cancellation and cleanup of incomplete files;
- model registry update only after successful verification;
- delete only app-managed artefacts, with active-model unload first;
- no silent overwrite of a different model revision.

The app must clearly distinguish a downloaded file from a validated, installed and runnable model.

## 5. First model and backend sequence

- First model: LFM3-1.7B text generation.
- First device: Google Pixel 8 Pro (Tensor G3).
- Initial proof: a reference backend to establish end-to-end model loading and streaming.
- In parallel: LOLA Native CPU backend conformance and performance work.
- Do not claim native inference if the app is still using a reference backend. The diagnostics screen must identify the backend truthfully.

## 6. Offline and lifecycle behaviour

- Chat, model selection and conversation history remain available offline after model installation.
- Download requires network; inference must not.
- App restart restores model metadata and conversation history.
- Interrupted download resumes or restarts safely.
- App backgrounding may pause/cancel inference according to lifecycle policy; never leave an unbounded background generation task.
- Android foreground-service use is not assumed for ordinary foreground chat.

## 7. Acceptance checklist

- [ ] App builds and installs on the target Pixel.
- [ ] Model catalogue displays LFM3-1.7B metadata.
- [ ] Download progress and cancellation work.
- [ ] Corrupt or mismatched artefact is rejected.
- [ ] Verified model can be installed and selected.
- [ ] Model can be deleted without deleting conversation history.
- [ ] Airplane-mode chat works after model installation.
- [ ] Tokens stream in order and Stop cancels generation.
- [ ] App restart preserves conversations and model registration.
- [ ] Two sessions do not share mutable LFM3 cache state.
- [ ] Diagnostics report the actual backend and measured metrics.

A UI mock or successful compile alone does not satisfy these acceptance criteria.
