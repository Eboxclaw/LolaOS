# LOLA ACE model catalogue implementation

## What this slice implements

The `ace-models` crate is a portable, Android-independent domain layer. It currently provides:

- schema-versioned JSON catalogue types;
- strict validation of model IDs, Hugging Face repository IDs, immutable 40-character revisions, artifact filenames, canonical pinned download URLs and SHA-256 strings;
- a bounded-memory SHA-256 verifier that reads an artifact incrementally and can also enforce an exact byte count when one is known;
- an explicit artifact lifecycle state machine covering download, verification, installation, loading, use, unloading and deletion;
- unit tests for the bundled catalogue, mutable revision rejection, URL pinning, integrity checks and invalid lifecycle transitions.

The initial catalogue entry is the official Liquid AI LFM3-1.7B GGUF Q4_K_M artefact. It is pinned to the repository revision recorded in the manifest. Its SHA-256 is taken from the publisher's Hugging Face file metadata. The displayed size is publisher-rounded; no exact byte count is asserted.

## Important boundary

This crate does **not** implement HTTP download, resume, filesystem installation, SQLite persistence, Android UI, model loading or inference. It supplies typed metadata, validation, integrity and lifecycle primitives for those implementations. A state transition does not itself perform the corresponding I/O operation.

The 256K context is a publisher-advertised specification, not a context size validated on the Pixel 8 Pro. The catalogue leaves `tested_context_tokens` unset until device measurements establish a safe profile.

## Next implementation steps

1. Android model-download/store layer: resumable download to app-private staging, progress and cancellation.
2. Verify size/hash and atomically promote the file into the managed store.
3. Persist installed artefact revisions and lifecycle/error state.
4. Add a backend compatibility check before enabling Load/Ready.
5. Add the first reference inference backend and verify local streaming in airplane mode.
6. Build the Kotlin/Compose chat and model-library application around those stable contracts.
