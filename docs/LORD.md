# LORD — the face of LOLA

`assets/lord/lord-base.png` (source sprite: `darktwin.png`, owner-supplied).
This doc is the interface concept: **a minimalist, Obsidian-like app with
plugins and chat**, hosting the LOLA SDK as a background service, LORD as
its main character. Visual craft (layout, motion, theming) remains a
design pass; this fixes the *concept and semantics*.

## 1. Who LORD is

A dark-paladin guardian: charcoal armor, gold trim, crimson accents;
greatsword upright, shield forward; glowing red slit eyes. Stern,
protective, noble — the visual embodiment of LOLA's ring-0 discipline:
**the guardian of the ledger**. LORD is not decoration; LORD *is* the
state machine, made visible.

## 2. LORD states = LOLA machine states

Every animation state maps to a real kernel/loop event — the sprite never
lies about what the system is doing:

| LORD state | LOLA semantics | Trigger |
|---|---|---|
| **idle** (breathing bob, cape flutter, eye pulse) | loop sleeping between ticks; both slots resident | WorkManager cadence |
| **thinking** (sword lowered, helm tilt, `?` glint) | slot A prefill burst — a d1 support pass running | decision tick / router pass |
| **alert** (eyes flare, cape whips, shield raises) | escalation: slot B agent invoked | router `escalate_agent` granted |
| **speaking** (visor flickers like an equalizer) | slot B streaming (text or voice via LiquidCodec K9) | token stream active |
| **error** (eyes flash white/orange, shake, vignette) | risk rejection · kill switch · fail-closed | ring-0 verdict |
| **success** (sword raised, gold glint sweep) | paper/testnet fill recorded; decision settled | ledger fill row |

Readability at 32–48 px is preserved by design (eyes, shield cross, helm
finial carry the signal) — LORD works in the status bar, the FAB, and
notifications.

## 3. The app: minimal Obsidian, but alive

```
┌────────────┬──────────────────────────────┬───────────────┐
│ left rail  │         chat (center)        │ plugin panel  │
│            │                              │  (swipe/split)│
│ sessions ▾ │  LORD ─ hi. ledger: 42       │ ┌───────────┐ │
│  ├ today   │  you ─ why did d-41 reject? │ │ decisions │ │
│  ├ vault   │  LORD ─ [decision card]     │ │ skills    │ │
│  └ graph   │        evidence ▸ 3 refs    │ │ devices   │ │
│            │                              │ │ market    │ │
│ quick:     │  [input: ask, speak, attach] │ └───────────┘ │
│ ◉ LORD     │                              │  contextual   │
└────────────┴──────────────────────────────┴───────────────┘
```

- **Obsidian virtues**: local-first (the SQLite ledger *is* the vault);
  linking (sessions, decisions, graph nodes are all linkable ids —
  `[[d-41]]`); graph view = the actual knowledge graph, not a metaphor;
  plugin panel = Agent Skills + tool categories, progressive disclosure.
- **Chat is the primary surface**; the hard rule stands (ADR-032): the UI
  never dumps content into context — messages reference ids, LORD pulls
  via tools.
- **Device connections** = the categorized tool layer (TOOLS.md):
  calendar, alarms, notifications, files, apps… each appears in the
  plugin panel when enabled; read-only first, confirm-on-write via the
  FAB.

## 4. Architecture (already decided, restated)

Compose Multiplatform UI (ADR-027) ⇄ UniFFI ⇄ **LOLA SDK running as a
background service** (foreground service when active; WorkManager ticks
between; checkpoint/resume on process death — ADR-025). The SDK owns:
engine (K-kernels), memory (ledger/graph/retrieval), router, tools, LORD
state events. The UI owns presentation only — it renders LORD states from
ledger events, so a future wear/watch face or widget renders the same
truth.

## 5. Product principles

1. **LORD never fakes**: no "thinking" spinner without a real pass.
2. Minimal chrome, dense meaning: every pixel of chrome must link to a
   ledger object.
3. Plugins are skills; skills are auditable (manifests, allowed-tools).
4. The vault is portable: the SQLite ledger moves device-to-device.
5. Voice is first-class (omni pair; LORD speaks through K9).
