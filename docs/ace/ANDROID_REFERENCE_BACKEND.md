# Android reference inference backend

## Purpose

This is a temporary conformance backend for proving an offline LFM3 chat path on Android. It is not the LOLA Native Engine and is not a dependency of ACE core. The application depends on `LocalInferenceBackend`, not on llama.cpp types.

## Pinned reference source

- Upstream: `ggml-org/llama.cpp`
- Release tag: `b11960`
- Immutable commit: `8642541b255bfb5dc39cd351a178be09013a55d6`
- CMake fetches this exact commit, not a floating branch or tag.
- The tag is selected as the minimum reference build for Liquid AI's LFM3 GGUF path. A later upgrade must be explicit and revalidated.

## Android build profile

- ABI: `arm64-v8a` only in the first build.
- Backend: CPU baseline.
- GPU layers: zero.
- KleidiAI, OpenMP, dynamic backend loading, LLAMAFILE and native-host tuning are disabled in this first reference configuration.
- No global `-march` flag is set. Runtime capability-specific variants are not yet built.
- Native code is compiled with Android NDK 29 and CMake 3.31.6.

This is intentionally conservative. The build must remain runnable on the selected ARM64 Android baseline before any device-specific kernel is enabled.

## Session and streaming model

- Sampling follows Liquid AI's published LFM3 recommendation: temperature 0.1, top-k 50 and repeat penalty 1.1. The reference sampler is explicitly limited to those samplers (plus the final distribution draw), instead of inheriting unrelated generic top-p/min-p/DRY defaults.
- One model handle is loaded and can own multiple inference sessions.
- Each session owns a separate llama context, batch, sampler and cancellation flag.
- The first implementation re-evaluates the complete supplied chat history for every turn and clears that session's KV memory before prefill. It does not yet implement incremental conversation KV reuse.
- The chat template is read from GGUF metadata and applied by the pinned runtime.
- The host caps context at 8,192 tokens; the app initially uses 2,048.
- The host caps generated output at 256 tokens in the app; the native entry point rejects budgets above 1,024.
- Cancellation is checked during prefill batches and between generated tokens.
- Generated text is streamed through a Kotlin callback flow. A bounded-by-output-budget queue prevents token chunks from being silently dropped when UI rendering is slower than native decode.
- JNI message strings are converted from UTF-16 to standard UTF-8; JNI Modified UTF-8 must not be passed to the tokenizer.

## Native timing metrics

Each completed or cancelled generation publishes a native timing sample to the backend-neutral diagnostics state:

- **Prompt tokens:** token count after GGUF chat-template rendering and tokenization.
- **Prefill:** time spent processing prompt batches in llama.cpp. Template rendering and tokenization are excluded from this field.
- **TTFT:** elapsed time from entry into native generation to the first streamed token callback. It includes JNI message conversion, template rendering, tokenization, prefill and selection/streaming of the first token.
- **Decode:** elapsed time from the end of prefill to generation end. It includes sampling, token-piece conversion and the JNI streaming callback.
- **Decode tok/s:** generated tokens divided by the decode window. It is not a GPU/NPU throughput figure.
- **Total:** time for the native generation call, including prompt preparation, prefill and decode.

All timings use `std::chrono::steady_clock` (monotonic). Cancellation samples report tokens produced up to cancellation. An empty/EOG response has no throughput value. Model-load time is excluded.

These timings support same-device comparisons. They have not yet been collected on a physical Pixel 8 Pro. PSS, CPU frequency, battery and thermal state are not measured by this slice.

## Safety and limitations

- No tools, file access, network tools, ReAct loop or agent permissions are exposed to the model.
- No GPU, NPU, NNAPI or Tensor/GPU offload is claimed.
- The model and context remain in memory until the user unloads them or the process exits.
- Conversations and messages are persisted in app-private SQLite storage. There is no cloud sync or app-level database encryption layer; data protection relies on the Android app sandbox and device security. The app manifest disables backup.
- Conversation history is persisted, but the inference backend re-evaluates the supplied history on every turn; incremental KV-cache reuse and automatic history compaction are not implemented. The app's initial context budget is 2,048 tokens, so start a new conversation when the budget is reached.
- Android model deletion is blocked while the model is loading or resident.
- The reference runtime is not yet tested on a physical Pixel 8 Pro.
- The CI build and JVM tests can establish compilation and policy contracts, but not successful LFM3 generation on device.
- Performance, memory, thermal behaviour and time-to-first-token remain unmeasured.

## Next validation steps

1. Build the native ARM64 library in CI.
2. Install the debug APK on a Pixel 8 Pro.
3. Download the pinned model, confirm the SHA-256 and cold-load it.
4. Run a fixed Portuguese prompt suite and compare the generated output with golden behaviour.
5. Verify streaming, cancellation, repeat generation, unload/reload and deletion protection.
6. Record cold-load time, prefill time, TTFT, decode tokens/s, peak PSS, context size, CPU frequency and thermal state.
7. Add cancellation during prefill/decode, repeated load/unload and low-memory recovery device tests.
8. Only then compare the reference backend against the LOLA Native Engine.
