# LOLA Tools — Categorized Native Tool Layer

ADR-033/034. Code: `lola-harness/src/tools.rs` · config: `config/tools.yaml`.
All Android facts below verified against primary docs Nov 2026 (sources
inline; ⚠️ = verify at implementation).

## 1. Posture

**Read-only by default; every write is confirm-on-write** (agent proposes →
one deliberate FAB tap → execute). Registry grows per category; the
*prompt* sees ≤10 tools + `search_tools(query)` beyond — and note LFM3
natively supports this: `defer_loading=True` keeps tool definitions out of
the prompt until a search returns them as `tool_reference`, injected into
`<additional_tools>` ([LFM3-1.7B card](https://huggingface.co/LiquidAI/LFM3-1.7B)).
Our registry design and the model's own mechanism align.

## 2. Category taxonomy (v0.1 registry: 23 tools)

| Category | Side | Initial tools | Native API (verified) | Permission | Gotchas |
|---|---|---|---|---|---|
| calendar | Bridge | list_events · create_event | `CalendarContract` queries; writes via `ACTION_INSERT` intent or provider insert | READ/WRITE_CALENDAR (dangerous) | intents are permissionless and Google-recommended — the Calendar app itself is the confirmation UI; direct insert needs `CALENDAR_ID`, `DTSTART`, `EVENT_TIMEZONE` ([docs](https://developer.android.com/identity/providers/calendar-provider)) |
| time_alarm | Bridge | clock_now · lola_next_tick · alarm_set · timer_set | `AlarmClock.ACTION_SET_ALARM/SET_TIMER/SHOW_ALARMS` (+dismiss/snooze); `AlarmManager` for OUR ticks only | `com.android.alarm.permission.SET_ALARM` (**normal**) | **no read-back API exists** — `lola_next_tick` reads only LOLA's own scheduled work ([AlarmClock ref](https://developer.android.com/reference/android/provider/AlarmClock)); exact alarms need `SCHEDULE_EXACT_ALARM` (special, denied by default since A14) — we don't use them ([A14 changes](https://developer.android.com/about/versions/14/changes/schedule-exact-alarms)) |
| notifications | Bridge | recent · reply | `NotificationListenerService` (+ `RemoteInput` for reply) | special-access toggle only ([docs](https://developer.android.com/reference/android/service/notification/NotificationListenerService)) | Android 15 redacts OTPs from listeners; rebinding after crash-loops via `requestRebind()`; ⚠️ exact POST_NOTIFICATIONS interplay for targeting-13+ |
| device | Bridge | battery · screen · network · torch | `BatteryManager`, `PowerManager.isInteractive`, `ConnectivityManager`, `CameraManager.setTorchMode` | none / `ACCESS_NETWORK_STATE` (normal) | torch has no exclusive ownership (camera use turns it off) ([CameraManager](https://developer.android.com/reference/android/hardware/camera2/CameraManager)) |
| files | Bridge | files_pick | SAF `ACTION_OPEN_DOCUMENT` (+tree) | **none** — user-driven picker | persist via `takePersistableUriPermission` ([SAF](https://developer.android.com/guide/topics/providers/document-provider)) |
| media | Bridge | mic_capture | `AudioRecord` → omni voice path | RECORD_AUDIO (dangerous) | foreground-only; ⚠️ A12 quick-settings toggles / A14 while-in-use FGS rules (source 404'd) |
| apps | Bridge | app_launch | `getLaunchIntentForPackage` + `startActivity` | none; `<queries>` visibility | `QUERY_ALL_PACKAGES` is Play-restricted — declare `<queries>` for a whitelist instead ([visibility](https://developer.android.com/training/package-visibility/declaring)) |
| accessibility | Bridge | **none** — disabled | `AccessibilityNodeInfo` tree + `dispatchGesture` | BIND_ACCESSIBILITY_SERVICE + user grant | **Play policy bans assistants/autonomous planning**; only deterministic rules + disability-primary tools are sanctioned ([policy](https://support.google.com/googleplay/android-developer/answer/10964491)) → ADR-034: if ever enabled, deterministic actions only, per-action confirm |
| memory | Core | graph_query · fts_search · ledger_lookup · casebank_lookup | our SQLite/FTS5 | — | results still pass the support gate (ADR-032) |
| market | Core | market_snapshot · backtest_run | venue adapters + lola-replay | — | backtest_run gated (cost) |
| system | Core | support_pass · kill_switch | d1 support pass; risk engine | — | gated by policy version |

**Deliberately absent:** SMS (Play-restricted to default-handler apps),
broad contacts (Play-restricted; the Contact Picker is the compliant path
when needed), background location (separate grant + Play policy),
background clipboard (impossible without focus since A10).

## 3. The bridge (ADR-027/032/033)

```
agent emits ToolCall {name, args_json, idempotency_key}
 └► Rust core: registry check → category enabled? safety tier?
     ├ read_only ──────────► UniFFI callback ─► Kotlin executor ─► result
     └ confirm_on_write ──► FAB confirmation (proposal shown, one tap)
                              └ approved ► Kotlin executor
                                  (runtime permission on first use) ► result
 Kotlin result ─► ref + hash to ledger ─► support-pass gate ─► context
```

Kotlin-side contract: one executor per category (each category = one
future Kotlin file); lazy runtime-permission requests per tool;
results are references + hashes, never raw dumps (ADR-032); idempotency
keys dedupe retries across process death.

## 4. Skills linkage

Skills declare `allowed-tools` (standard field); names must resolve to
enabled `tools.yaml` entries. Skills compose categories; they never ship
their own tools.

## 5. Open-source landscape (borrow, don't depend)

| Project | Mechanism | License | Verdict |
|---|---|---|---|
| [mobilerun-portal](https://github.com/droidrun/mobilerun-portal) | AccessibilityService app + HTTP/WS JSON-RPC | custom (inspect!) | best reference for an accessibility tool surface; license must be checked before borrowing code |
| [MobileAgent-Android](https://github.com/GiggleWang/MobileAgent-Android) | fully on-device Kotlin agent (accessibility + VLM) | MIT | architecture reference for on-device loops |
| [Open-AutoGLM](https://github.com/zai-org/Open-AutoGLM) | host-side adb control | Apache-2.0 | borrow prompt/action vocab only |
| [DroidMCP](https://github.com/kahz12/DroidMCP) | 14 MCP servers in Termux | MIT | per-tool permission-surface design ideas |
| [AppAgent](https://github.com/TencentQQGYLab/AppAgent) / [MobileAgent](https://github.com/X-PLUG/MobileAgent) | host-side adb | MIT | action-space reference; mechanism unusable on-device |
| termux-mcp (termuxgpt) | Termux HTTP bridge | **AGPL-3.0** | design only — AGPL contamination, never copy |

## 6. Extending a category

1. Add the tool to `config/tools.yaml` under its category (safety ≥
   category default; permissions listed).
2. Implement the Kotlin executor method (Bridge tools) or Rust fn (Core
   tools) behind the same ToolCall contract.
3. Prompt snippet ≤1 line; if the category crosses
   [`MAX_PROMPT_TOOLS`], rely on `search_tools`.
4. Confirm-on-write UI copy reviewed like a risk-policy change.
