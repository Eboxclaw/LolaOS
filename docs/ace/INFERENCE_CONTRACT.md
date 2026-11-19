# LOLA ACE inference contract

## 1. Boundary rule

The inference engine knows models, tensors, tokens, cache state and generation. It does not know markets, portfolios, orders, financial risk or product-specific business objects.

The existing `lola-infer::Infer` API is not yet a suitable ACE contract because it references market decision structures. This document defines the target boundary; it does not claim that the current implementation conforms.

## 2. Core operations

The initial contract should expose a small set of explicit operations:

- `inspect_model(artifact) -> ModelDescriptor`
- `load_model(model_id, LoadOptions) -> ModelHandle`
- `unload_model(model_handle) -> UnloadReport`
- `create_session(model_handle, SessionOptions) -> InferenceSession`
- `prefill(session, input) -> PrefillReport`
- `generate(session, GenerationOptions, sink) -> GenerationResult`
- `cancel(run_id) -> CancelResult`
- `reset_session(session_id) -> ResetResult`
- `capabilities(backend, model) -> CapabilitySet`
- `diagnostics(run_id) -> RunDiagnostics`

Names are illustrative until the Rust types and FFI shape are implemented. Avoid exposing raw tensor pointers or backend-specific structures to Kotlin.

## 3. Typed data

The contract should include:
- stable model ID, artifact hash, model family/revision and tokenizer/template metadata;
- backend ID and backend version;
- input message/token representation;
- generation parameters (max output tokens, sampling configuration, stop conditions);
- streaming chunk with sequence number and terminal status;
- run/session IDs;
- explicit error categories (unsupported operator, invalid artifact, out of memory, cancelled, deadline, backend failure);
- diagnostics and timing fields.

Keep raw model output separate from parsed tool-call proposals. Tool execution is outside the inference engine.

## 4. Streaming semantics

- A run emits ordered chunks with monotonically increasing sequence numbers.
- The consumer can apply backpressure or cancel.
- Cancellation is idempotent and has a measurable completion latency.
- A terminal event is emitted exactly once for success, failure or cancellation.
- Partial text already emitted is not silently rewritten.
- UI rendering must not block native decode; use bounded buffering and coalescing where needed.

## 5. Mutable model state and session isolation

Each inference session owns its mutable state. For LFM3 this includes, as applicable to the selected architecture and backend:
- convolution history/state for gated short-convolution layers;
- attention KV state and sliding-window bounds;
- current position and cache metadata;
- sampler state and generation cursor.

No mutable convolution or KV state may be shared between independent conversations. Prefix sharing, if later implemented, must use immutable shared prefixes and copy-on-write semantics with proven isolation.

A model swap ends or checkpoints the active run first. A session reset clears all state owned by that session. App process death must not corrupt persistent conversation history; ephemeral inference state may be rebuilt from the stored conversation.

## 6. Backend capability negotiation

Capabilities must be explicit and queryable:
- model family/revision support;
- supported quantization/tensor types;
- supported operators and cache modes;
- context limits by device/backend profile;
- streaming support;
- cancellation support;
- modality support;
- accelerator placement and fallback behaviour.

A backend must reject unsupported graphs rather than silently produce partial or incorrect output. Report actual partitioning and fallback where the runtime supports mixed execution.

## 7. Reference and native backends

Keep at least one known-good reference path during native development. The reference path validates model loading, chat template, tokenization, output correctness and baseline performance. The LOLA Native Engine is promoted only after golden tests, cache/session tests and device benchmarks pass.

Do not treat llama.cpp, ONNX Runtime or LiteRT as the LOLA architecture. They are potential reference or backend implementations. GGUF and ONNX are model interchange/artifact paths, not inference engines.

## 8. Required conformance tests

- deterministic tokenizer and chat-template fixtures;
- prompt prefill and token-by-token generation;
- stop sequence and max-token enforcement;
- cancellation during prefill and decode;
- two concurrent sessions with distinct state;
- session reset and model unload;
- low-memory failure and safe recovery;
- malformed/corrupt model artefact;
- golden output/logit comparison with documented tolerances;
- repeatability across backend versions.
