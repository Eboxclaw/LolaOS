# LOLA ACE model catalogue and artifact lifecycle

## 1. Scope

This document defines the lifecycle for model artefacts in the Android app. It is not a proposal for a new mandatory model-weight format.

LOLA ACE adapts to model artefacts supplied by model publishers. It should not require third-party model authors to generate a proprietary `.lola` file.

## 2. Catalogue record

Each catalogue entry should declare:
- stable model ID and display name;
- family and exact revision/commit;
- publisher and canonical source URL;
- supported modality/task;
- artifact format (GGUF, ONNX or another explicitly supported importer);
- quantization and tensor-type metadata;
- expected file size and cryptographic checksum when available;
- tokenizer and chat-template provenance;
- architecture/config metadata;
- context advertised by publisher and LOLA-tested context profile;
- minimum app/runtime version;
- license and attribution;
- capability tags and known limitations.

The catalogue is metadata, not executable code. Treat remote metadata as untrusted input and validate it.

## 3. Lifecycle states

```text
Available
  -> Downloading
  -> Downloaded
  -> Verifying
  -> Verified
  -> Installing
  -> Installed
  -> Loading
  -> Ready
  -> InUse
  -> Unloading
  -> Installed

Failure paths:
Downloading/Verifying/Installing -> Failed or Cancelled
Loading -> Failed
Installed -> Deleting -> Available
```

Persist state transitions and error reason. Do not label an unverified download as installed or runnable.

## 4. Download and verification

1. Check network policy and available disk space.
2. Download to a temporary app-private path.
3. Support HTTP range/resume only when the source supports it and validate the final file.
4. Check expected size.
5. Verify SHA-256 or stronger publisher-provided checksum where available.
6. Validate container header and required metadata.
7. Atomically move to the managed model store.
8. Register the installed revision in the local database.

If the publisher does not provide a checksum, record that fact and apply the strongest available validation; do not fabricate a trusted hash.

## 5. Install and storage layout

Suggested layout:

```text
files/models/
  <model-id>/
    <revision>/
      manifest.json
      weights.gguf
      tokenizer/
      metadata/
files/models/.staging/
```

The actual layout depends on the selected importer and Android storage constraints. Never expose internal paths to the model as trusted authority. The model store owns path resolution and deletion.

## 6. Selection, loading and residency

- Selecting a model changes the preferred model; it does not necessarily load it immediately.
- Loading validates backend capability and memory budget before allocating.
- The residency manager accounts for weights, context/KV, convolution state, scratch and runtime overhead.
- Only load concurrent models if measured headroom allows it.
- Do not swap a model in the middle of an active generation.
- If memory pressure requires eviction, cancel or checkpoint at a safe boundary and report the event.

## 7. Deletion

- A model in use must be unloaded before deletion.
- Delete only paths owned by the model store.
- Do not delete user conversations or diagnostics when deleting weights.
- Remove catalogue installation state only after filesystem deletion succeeds.
- Handle partial deletion and restart recovery.
- Ask for confirmation and show expected storage reclaimed.

## 8. Security and supply-chain requirements

- HTTPS sources only unless an explicit developer/test mode permits otherwise.
- Validate publisher and revision; avoid mutable `latest` references for reproducible builds.
- Verify checksums where provided.
- Treat GGUF/ONNX metadata, tokenizer files and templates as untrusted data.
- Enforce size limits and parser bounds before allocating memory.
- Do not execute code from model packages.
- Keep model artefacts in app-private storage by default.
- Record provenance and verification status in the local model registry.
