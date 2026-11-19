# Repository audit and LOLA ACE migration map

**Audit date:** 2026-11-19  
**Audited ref:** `main`  
**Target:** LOLA ACE — Android-first local AI application and native LFM3 inference engine  
**Status:** Source inventory and architectural classification; no runtime behaviour changed

## 1. Findings at a glance

The repository is a substantial Rust prototype for a trading-focused edge agent. It already contains non-trivial work in inference kernels, GGUF parsing, model residency, event/session handling, SQLite persistence and retrieval. However, important interfaces are still coupled to trading concepts, and the repository currently has no Android application project.

This is not a blank slate, and it should not be thrown away. It is also not yet a clean generic agent runtime. The migration must preserve useful work while removing trading assumptions from the product's generic core.

### Confirmed repository shape

- Rust workspace with six crates: `lola-memory`, `lola-harness`, `lola-infer`, `lola-venue`, `lola-replay`, `lola-cli`.
- Rust 2021; workspace declares Rust 1.85.
- CI workflow exists under `.github/workflows/ci.yml`.
- Existing config, SQL migrations, evaluation fixtures, benchmarks and extensive technical documentation.
- No `settings.gradle(.kts)`, Gradle wrapper, Android manifest, Kotlin source tree or Compose application appears in the audited repository tree.
- The root README and current RFC describe LOLA as a trading-oriented system.

## 2. File and crate classification

Legend:

- **Keep / candidate:** potentially useful in the generic LOLA ACE core; must pass contract and test review.
- **Extract / rewrite:** useful concept or implementation, but currently coupled to trading.
- **Trading vertical:** keep out of the generic LOLA ACE runtime; preserve in a separate product/vertical if still needed.
- **Replace / defer:** current scaffold or design needs a new implementation for the Android POC.

| Area | Observed contents | Classification | Migration action |
|---|---|---|---|
| `lola-harness::session` | Append-only session event tree, parent links, message/model-change/compaction concepts | Keep / candidate | Extract a domain-neutral session/event model; retain append-only and branch semantics |
| `lola-harness::lifecycle` | Checkpoint/resume and Android process/thermal/memory lifecycle concepts, currently framed around trading ticks | Extract / rewrite | Replace tick-specific lifecycle with generic task/session lifecycle and cancellation |
| `lola-harness::events` | Synchronous event bus with risk verdict, order intent and paper-fill events | Extract / rewrite | Define typed generic events; move market/order event variants to trading adapter |
| `lola-harness::contracts` | MarketStateV1, DecisionV1, OrderIntentV1, portfolio/risk types | Extract / rewrite | Keep only generic execution/session/model contracts in ACE core; trading schemas belong in vertical |
| `lola-harness::decision_loop` | Completed bars → features → strategy → risk → paper order/fill | Trading vertical | Do not port this loop into ACE core |
| `lola-harness::features`, `strategy` | Market indicators and momentum strategy | Trading vertical | Preserve only in trading product |
| `lola-harness::risk` | Portfolio/order risk guard | Trading vertical | ACE needs tool/policy safety, not financial risk checks |
| `lola-harness::router` | d1 scoring and pass-forward rules with market-decision types | Extract / rewrite | Preserve deterministic policy ownership and hard budgets; replace domain schema |
| `lola-harness::tools` | Tool registry, safety tiers, AndroidBridge/Core split, plus Market category | Keep / candidate | Keep capability/safety model; move market tools to trading extension |
| `lola-harness::persona` | Versioned persona and session prompt placement | Keep / candidate | Reuse after testing its storage and context contracts |
| `lola-infer::gguf` | Pure Rust GGUF header/metadata/tensor-info parser | Keep / candidate | Retain as importer; harden malformed-file tests and quantization coverage |
| `lola-infer::engine` | LOLA custom LFM3-oriented engine and kernel milestones | Keep / rewrite | Treat as experimental; verify operator completeness and numerical parity before app integration |
| `lola-infer::kernels/*` | Conv7, global attention, sliding-window attention, fused and quality helpers | Keep / candidate | Preserve; add LFM3-specific conformance tests and session-state isolation |
| `lola-infer::infer` | Infer trait takes market-state decision inputs and model outputs | Extract / rewrite | Replace with model-agnostic generate/stream/embed/score interfaces, typed capabilities and cancellation |
| `lola-infer::oracle` | Golden conformance harness around trading decision contracts | Extract / rewrite | Retain golden-test machinery; replace market fixtures with token/logit/operator fixtures |
| `lola-infer::llamacpp` | Explicitly marked scaffold; dependency not declared | Replace / defer | Do not advertise as working backend until linked, built and tested |
| `lola-infer::residency` | Pair-based model residency and memory budgeting | Keep / rewrite | Generalize to model slots, memory classes, safe swap boundaries and Android memory signals |
| `lola-memory::ledger` | Single-writer SQLite ledger with venues, instruments, orders and sessions | Extract / rewrite | Separate generic event/session storage from trading tables and migrations |
| `lola-memory::graph` | Relational graph with venue, market regime, order and trade node types | Extract / rewrite | Keep traversal approach; make generic graph types domain-neutral |
| `lola-memory::cache`, `casebank` | Market-state decision cache and outcome-weighted trading cases | Trading vertical / extract ideas | Keep cache-key discipline; redesign for prompt/prefix/model/tool result caches |
| `lola-memory::compaction`, `blobs`, `persistence` | Archive/compaction, content-addressed blobs, SQLite persistence | Keep / candidate | Verify schema coupling, crash recovery and lifecycle tests |
| `lola-venue` | Venue trait and Hyperliquid-oriented configuration | Trading vertical | Keep isolated; no dependency from ACE core |
| `lola-replay` | Conservative market-bar/order replay assumptions | Trading vertical | Keep isolated as a trading experiment tool |
| `lola-cli` | CLI loads risk and venue config and runs trading-oriented commands | Replace / split | Keep optional developer CLI later; do not make it the Android product interface |
| `config/risk.yaml`, `venue.yaml`, `router.yaml` | Financial risk, venue and market routing | Trading vertical | Move with trading app, not generic ACE |
| `config/models.yaml`, `memory.yaml` | Model catalogue/residency and memory policy | Keep / rewrite | Rework around downloadable LFM3 model artefacts and Android device profiles |
| `config/tools.yaml` | Tool registry and safety configuration | Keep / candidate | Remove market category from generic default; define app permissions and confirmation policy |
| `migrations/*` | Ledger, omni sessions and feature vectors | Extract / rewrite | Split generic session/model/download tables from trading schema; use forward-only migrations |
| `evals/*` | Agent task and decision-contract fixtures | Extract / rewrite | Add chat, streaming, cancellation, model-load and session-isolation evals |
| `docs/*` | Extensive architecture docs, many explicitly trading-specific | Reorganize | Preserve historical trading docs under a trading namespace; establish ACE docs as canonical |
| `assets/lord/*` | LORD image asset | Review ownership | Retain only if product/brand owner confirms it belongs in ACE |

This table is a first-pass classification based on source inspection. It is not a deletion list. Mixed modules require a dependency-level audit before extraction.

## 3. Critical architectural risks

### 3.1 Inference API is not yet domain-neutral

The current `Infer` interface and oracle use market decision structures such as `MarketStateV1`, `DecisionV1`, `MarketRegime` and portfolio/risk types. A generic model engine must not depend on these concepts.

Target separation:

- **Model runtime:** tokens, logits, structured outputs, streaming chunks, cache state, model metadata and cancellation.
- **ACE decision layer:** policy-owned routing, execution strategy selection, tool proposals, budgets and approvals.
- **Trading vertical:** market state, portfolio, risk and order intent.

### 3.2 Session storage and model state need explicit isolation

LFM3 has both convolution state and attention KV state. Each conversation/session must own independent mutable model state. The repository's event-tree/session concepts are promising, but their correctness must be tested against concurrent sessions, cancellation, app restart and model swap.

### 3.3 The existing Android lifecycle module is not an Android app

The lifecycle source documents Android constraints but the repository contains no Android Gradle project, manifest, Compose screens, download manager, model installer or JNI/UniFFI bridge. Android readiness must not be inferred from comments or constants.

### 3.4 GGUF parser does not prove end-to-end inference

A GGUF reader and kernel modules are useful foundations, but do not alone establish support for all required tensor types, quantization variants, tokenizer/chat templates, sampling, cache update semantics, or a runnable LFM3 checkpoint. The app must first reach an end-to-end reference-backend milestone, then the native engine can replace it behind the same contract.

## 4. Target repository organization

Proposed incremental target; not a request for an immediate repository-wide rename:

```text
.
├── android/
│   ├── app/                         # Compose chat and model library
│   ├── core/
│   │   ├── model-catalog/           # metadata, checksums, compatibility
│   │   ├── model-download/           # resumable download and verification
│   │   ├── model-store/              # install, select, delete, storage
│   │   └── chat-data/                # conversations and message persistence
│   └── native/
│       └── lola-bridge/              # Kotlin ↔ Rust FFI boundary
├── crates/
│   ├── ace-core/                     # budgets, scheduler, policy, cancellation
│   ├── ace-session/                  # session/event ledger abstractions
│   ├── ace-infer/                    # backend-neutral inference API
│   ├── ace-lfm3/                     # LFM3 model frontend and state
│   ├── ace-kernels/                  # native operators and kernels
│   ├── ace-model-format/              # GGUF importer and future adapters
│   └── ace-diagnostics/               # metrics and benchmark events
├── verticals/
│   └── trading/                      # isolated legacy trading application
├── config/
│   └── lfm3-models.yaml              # curated model metadata, not secrets
├── docs/
│   ├── ace/                           # canonical LOLA ACE architecture
│   ├── adr/                           # accepted architecture decisions
│   └── trading-archive/               # preserved historical trading docs
├── evals/
│   ├── inference/
│   ├── streaming/
│   ├── sessions/
│   └── android/
└── scripts/
```

The proposed names are target names, not an instruction to rename every crate in one PR. Keep the existing crate names until the extraction boundaries compile independently.

## 5. Progressive PR plan

| PR | Scope | Expected outcome | Destructive? |
|---|---|---|---|
| 1 | Initial migration boundary | Agreed direction and safety rules | No |
| 2 | Repository inventory and migration map | File-level classification and target layout | No |
| 3 | ACE architecture docs and ADR baseline | Canonical contracts, state ownership and budgets | No |
| 4 | Generic session/event contracts | First code extraction with compatibility tests | No |
| 5 | Inference API separation | Remove market types from generic inference boundary | Controlled |
| 6 | Model artefact manager contracts | Manifest, checksum, install lifecycle and storage policy | No |
| 7 | Android Compose app shell | Installable chat/model-library skeleton | No |
| 8 | Reference LFM3 backend | Offline chat and streaming vertical slice | No |
| 9 | Native engine integration | LFM3 native CPU path behind stable interface | Controlled |
| 10 | Trading isolation and cleanup | Move or remove trading-only paths after references are clear | Yes, only with approval and tests |

PRs 4 onward must be built on the result of earlier merged PRs or explicitly stacked. Avoid parallel branches that edit the same root workspace or shared contracts.

## 6. Acceptance gates

Before a module is called reusable:

- It has no trading-domain dependency in its public API.
- It builds independently in the workspace.
- Unit and integration tests cover its stated invariants.
- It has a clear owner and stable contract.
- Android constraints are measured rather than asserted.

Before the first Android POC is called complete:

1. Install the app on the target Pixel.
2. Download and verify LFM3-1.7B.
3. Disable network connectivity.
4. Start a new conversation and receive streamed tokens.
5. Cancel generation and confirm prompt cancellation.
6. Close and reopen the app; model and conversation remain available.
7. Run two sessions without convolution/KV state leakage.
8. Report TTFT, prefill/decode speed, memory and backend honestly.

## 7. Explicit non-goals for this migration

- Do not delete the trading product or its research history in the first cleanup.
- Do not invent a proprietary `.lola` weight format.
- Do not claim LOLA outperforms GGUF/llama.cpp, ONNX Runtime or LiteRT without same-device measurements.
- Do not assume Tensor G3 has a usable Qualcomm QNN path.
- Do not load all model slots simultaneously on constrained devices.
- Do not move to Rust-only UI or make CLI the primary user experience.
