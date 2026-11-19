# Android model manager — initial implementation

## Implemented

- Uses the canonical bundled model catalogue; a transfer is accepted only for a GGUF artefact with an immutable 40-character revision, a 64-character SHA-256 and the exact canonical Hugging Face `resolve/<commit>/<filename>` URL.
- Downloads into app-private `filesDir/models/.staging/` using a bounded 128 KiB buffer.
- Supports byte-range resume with HTTP `Range` and validates `Content-Range`. If the server ignores Range and returns HTTP 200, the partial file is overwritten instead of appended. HTTP 416 is accepted only when the server's total equals the existing partial length; the partial is then hash-verified.
- Follows at most five HTTPS redirects. Cleartext HTTP is not accepted.
- Enforces an 8 GiB artifact ceiling, an initial free-space threshold, and remaining-byte + disk-headroom checks when a server reports content length.
- Supports cancellation. Partial bytes are retained on cancellation or transport failure so that a later app launch can resume from the current file length.
- Hashes the completed file incrementally with SHA-256 before it can be installed.
- Promotes the verified file to its app-private revision directory with an atomic filesystem move. It does not fall back to a non-atomic copy.
- Persists the installed model record using Android `AtomicFile`.
- Deletes managed model revisions and partial files on a background executor, not on the Compose UI thread.
- The Models screen shows transfer state, byte progress, and download/cancel/resume/delete actions.

## Recovery and failure behaviour

A process death leaves the staging file in app-private storage. On the next launch, the Models screen presents it as interrupted and the user can resume it. If a process stopped after the atomic file move but before the installed registry was committed, the next download attempt checks the promoted file's SHA-256 and reconstructs the registry entry if it is valid.

A wrong SHA-256 causes the partial artifact to be discarded. HTTP, range, disk-space and filesystem failures are surfaced in the model card; they do not mark a model as installed.

## Explicit limitations

- The transfer executor is process-local. It does not guarantee a download continues when Android kills the process or the user force-stops the app. The partial file is resumable after the app is reopened. A foreground service / WorkManager policy is a separate follow-up.
- No retry scheduler, network constraint policy, notification UI or persisted byte-progress events yet.
- Only GGUF is accepted by this first Android download path; ONNX remains a catalogue format only.
- No end-to-end network server test or Pixel installation test is claimed by the unit tests. The unit tests cover catalogue pinning policy and HTTP range parsing.
- The Android downloader and the Rust `ace-models::verify_reader` currently implement the same SHA-256 contract independently; the Android layer is not yet calling the Rust crate through FFI.
- A downloaded and verified GGUF file is not proof that the current inference backend can load LFM3. The app still has no inference backend or chat generation.

## Next steps

1. Make the Android transfer restart-safe and background-aware with an explicit foreground download service or WorkManager design.
2. Add a device test that downloads the pinned public artifact, cancels, resumes, checks SHA-256 and deletes it on a Pixel 8 Pro.
3. Persist conversations and model metadata with a small local database.
4. Connect a reference LFM3 backend, then introduce the Kotlin↔Rust bridge.
