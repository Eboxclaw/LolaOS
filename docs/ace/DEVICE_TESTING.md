# Device testing — LOLA ACE real-device protocol

How to install the rolling APK on a real ARM64 Android phone and run the
same validation protocol used for the emulator baseline
([BENCHMARK_RECORD_EMULATOR_V1](BENCHMARK_RECORD_EMULATOR_V1.md)).

## Get the APK

**Option A — download on the phone (no computer):** open the repo's
**Releases → "LOLA ACE — device testing"**, download the newest
`lola-ace-<N>.apk` (highest N = newest), and open it (allow
"install unknown apps" for your browser/file manager).

**Option B — adb:**

```bash
adb install -r lola-ace-<N>.apk     # -r upgrades while keeping data
```

Every merge to `main` publishes a new build with a **higher versionCode
and the same signing key**, so any newer APK installs directly over any
older one — app data (chats, installed model) is preserved. Do not mix in
locally-built debug APKs: their signature differs and Android will refuse
the update (uninstall first).

The signing key is the committed **dev-testing key** (`android/keystore/`,
password in `app/build.gradle.kts`) — for keeping test devices updatable.
Replace it with a secret-held production key before any public
distribution.

## First run (one-time, needs network)

1. Open **Library** → the pinned `LFM3-1.7B · Q4_K_M` entry.
2. **Download model** (~1.1 GB; resumable — if it fails, "Try again"
   resumes from retained bytes; SHA-256 is verified in-app).
3. **Chats** tab → **Load LFM3 on device** (loads into RAM, ~1.4 GB free
   RAM needed; keep other heavy apps closed).
4. First reply is the slowest (cold context warm-up).

## What to test (the A5 protocol)

Run in this order; record every number the **Status** screen shows
(per-generation: prompt→output tokens, TTFT, decode tok/s — measured
natively):

1. **Golden prompts** (`evals/ace/golden-prompts-pt-v1.json`): send the
   9 fixed cases, one per fresh chat; note the Status metrics per case
   and screenshot interesting answers. Case `apresentacao-curta` twice —
   with seed 42 the output must be byte-identical.
2. **Cancellation**: long-essay prompt → **Stop** mid-sentence → partial
   reply must persist in the chat.
3. **Offline**: enable airplane mode → chat must still stream (model is
   fully local) → then try a Library download → it must fail honestly and
   resume later.
4. **Persistence**: force-stop/kill the app → relaunch → chats and model
   registration intact; model reloads from the installed GGUF.
5. **Load/unload**: "Free memory" → reload → generation still works.
6. **Thermals**: after ~15 minutes of generation, note if the device
   throttles (Settings → Battery or a thermal app); phones sustain far
   less than laptops — record the device's behaviour, not a guess.

Record results in a note per device (model, Android version, free RAM)
and paste them into the next benchmark-record PR — those numbers become
the **A5 device truth** row (non-target devices recorded with caveat;
Pixel 8 Pro stays the named gate).

## Known limitations while testing

- Context cap 2,048 tokens: long chats may refuse new messages — start a
  new chat (compaction is A6, queued).
- The model's raw reasoning may appear collapsed under "Reasoning" in
  replies; expanding is read-only.
- Switching tabs while a reply is generating can desync the chat screen
  (known defect; "All chats" is disabled during generation as a guard).
- First generation after a fresh install/model load is the slowest.

## Device truth vs emulator

The emulator baseline proved the plumbing and the harness. A phone's
decode tok/s, TTFT, memory pressure and thermals are the missing A5
evidence — this APK exists precisely to collect them.
