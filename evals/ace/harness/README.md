# Emulator/device truth-run harness

Tooling used to drive the LOLA ACE app end-to-end from adb for the
golden-suite runs (first used by `BENCHMARK_RECORD_EMULATOR_V1.md`).

## `adb-unicode-ime/` — broadcast IME for full-Unicode typing

Android's `input text` cannot type characters missing from the
KeyCharacterMap (all pt-BR diacritics NPE on API 35) and mangles long
strings. This 30-line InputMethodService receives
`am broadcast -a tools.lola.adbime.INPUT_TEXT --es text …` and commits the
text into the focused field verbatim.

Build & activate:

```bash
gradle -p adb-unicode-ime :app:assembleDebug
adb install adb-unicode-ime/app/build/outputs/apk/debug/app-debug.apk
adb shell ime enable tools.lola.adbime/.AdbImeService
adb shell ime set   tools.lola.adbime/.AdbImeService
```

Usage (host-side UTF-8 must be pushed to a file and read on-device —
accents die on the host→device argument path):

```bash
adb push prompt.txt /data/local/tmp/prompt.txt
adb shell 'am broadcast -a tools.lola.adbime.INPUT_TEXT --es text "$(cat /data/local/tmp/prompt.txt)"'
```

## Driving protocol (per suite case)

1. Tap `Nova conversa` (bounds resolved from a fresh `uiautomator dump`;
   do **not** hardcode — the layout shifts with/without a soft keyboard),
   verify the field shows the placeholder before typing.
2. Type the prompt via the IME broadcast above.
3. Resolve `Enviar` from the dump; tap.
4. Detect completion by **polling the app database**
   (`run-as ai.lola.ace cat databases/lola-chat.db` → new `assistant` row).
   UI heuristics (button text) are unreliable: both `Enviar` and
   `Parar geração` exist in the node tree regardless of state.
5. Only after completion, visit Diagnóstico (scroll — post-PR #17) to read
   TTFT/PREFILL/DECODE/TOTAL, and return to Conversas.
   Never switch tabs mid-generation (see known defect in the benchmark
   record).

`run_device_capture.sh` (this directory) collects ambient snapshots
(identity, thermal, CPU frequencies, app memory) for physical runs.
