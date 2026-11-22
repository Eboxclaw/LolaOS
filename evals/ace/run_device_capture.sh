#!/usr/bin/env bash
# Device truth-run capture helper for the LOLA ACE reference backend.
#
# Collects the ambient evidence (device identity, memory, thermal) around a
# manual golden-suite run driven through the app UI. Timing evidence (prompt
# tokens, prefill, TTFT, decode, total) comes from the native metrics surfaced
# on the Diagnostics screen and from filtered logcat.
#
# Usage: ./run_device_capture.sh <adb-serial> <capture-dir>
set -euo pipefail

SERIAL="${1:?usage: run_device_capture.sh <adb-serial> <capture-dir>}"
CAPTURE_DIR="${2:?usage: run_device_capture.sh <adb-serial> <capture-dir>}"
ADB="/Users/cristianovb/Library/Android/sdk/platform-tools/adb -s $SERIAL"

mkdir -p "$CAPTURE_DIR"

echo "== device identity =="
for prop in ro.product.brand ro.product.model ro.product.device ro.product.name \
            ro.board.platform ro.soc.manufacturer ro.soc.model \
            ro.build.version.release ro.build.version.sdk ro.hardware \
            ro.hardware.egl; do
  "$ADB" shell getprop "$prop"
done | tee "$CAPTURE_DIR/identity.txt"

"$ADB" shell cat /proc/meminfo | head -3 | tee -a "$CAPTURE_DIR/identity.txt"
"$ADB" shell cat /proc/version | tee -a "$CAPTURE_DIR/identity.txt"
"$ADB" shell dumpsys battery | grep -E 'level|status|temperature' | tee -a "$CAPTURE_DIR/identity.txt"

snapshot() {
  local tag="$1"
  local ts
  ts="$(date +%H%M%S)"
  {
    echo "===== snapshot $tag @ $ts ====="
    echo "--- thermal ---"
    "$ADB" shell dumpsys thermalservice 2>/dev/null | grep -E 'temperature|Status|mIsStatusOverride|throttling' | head -25
    echo "--- cpu frequencies (policy0..N kHz) ---"
    "$ADB" shell 'for p in /sys/devices/system/cpu/cpufreq/policy*; do echo "$p: $(cat $p/scaling_cur_freq)"; done'
    echo "--- app memory ---"
    "$ADB" shell dumpsys meminfo ai.lola.ace 2>/dev/null | sed -n '1,22p'
    echo "--- procrank pid ---"
    "$ADB" shell pidof ai.lola.ace || true
  } >> "$CAPTURE_DIR/snapshots.log" 2>&1
  echo "snapshot $tag captured"
}

case "${3:-snapshot}" in
  snapshot) snapshot "${4:-manual}" ;;
  idle)     snapshot idle ;;
  loaded)   snapshot loaded ;;
  post)     snapshot post-run ;;
esac

echo "done -> $CAPTURE_DIR"
