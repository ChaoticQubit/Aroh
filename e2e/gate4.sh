#!/bin/sh
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
state="$here/.local-state"
device="${AROH_E2E_DEVICE:-aroh-gate4}"
port="${AROH_DEV_PORT:-1439}"
bundle=io.aroh.app
shots="${AROH_GATE_SHOTS:-$root/docs/screenshots/gate4}"
results="$state/gate4-results.txt"
taps="$state/gate4-taps.txt"
mkdir -p "$state" "$shots/light" "$shots/dark" "$shots/keyboard"

curl -fsS -o /dev/null "http://localhost:$port" || { echo "start pnpm dev --port $port --strictPort in apps/aroh first"; exit 1; }
udid="$(xcrun simctl list devices | sed -n "s/^ *$device (\([0-9A-F-]*\)).*/\1/p" | head -1)"
[ -n "$udid" ] || { echo "no $device simulator; run AROH_E2E_DEVICE=$device AROH_DEV_PORT=$port sh e2e/pod33.sh build first"; exit 1; }
zone="${AROH_E2E_ZONE:-$(readlink /etc/localtime | sed 's|.*/zoneinfo/||')}"

xcrun simctl boot "$udid" 2>/dev/null || true
xcrun simctl bootstatus "$udid" -b >/dev/null
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
container="$(xcrun simctl get_app_container "$udid" "$bundle" data)"
appdir="$container/Library/Application Support/$bundle"
rm -rf "$appdir" "$container/Library/Application Support/aroh-location"
xcrun simctl keychain "$udid" reset
xcrun simctl shutdown "$udid" 2>/dev/null || true
python3 "$here/grant.py" "$HOME/Library/Developer/CoreSimulator/Devices/$udid/data" "$bundle" >/dev/null
xcrun simctl boot "$udid"
xcrun simctl bootstatus "$udid" -b >/dev/null
xcrun simctl ui "$udid" appearance light
xcrun simctl privacy "$udid" grant location-always "$bundle" 2>/dev/null || true
xcrun simctl spawn "$udid" defaults write com.apple.keyboard.preferences DidShowContinuousPathIntroduction -bool true
: > "$results"
: > "$taps"

drive() {
  flow="$1"
  minutes="$2"
  { printf 'window.AROH_E2E = {"smoke":%s,"only":%s,"nokb":%s,"notaps":%s};\n' "${AROH_GATE_SMOKE:-false}" "${AROH_GATE_ONLY:-[]}" "${AROH_GATE_NOKB:-false}" "${AROH_GATE_NOTAPS:-false}"; cat "$here/drive-lib.js" "$here/$flow"; } > "$state/gate4-drive.js"
  rm -f "$appdir/dev-marks.log"
  SIMCTL_CHILD_AROH_DEV_SCRIPT="$state/gate4-drive.js" xcrun simctl launch --console --terminate-running-process "$udid" "$bundle" > "$state/gate4-app.log" 2>&1 &
  launcher=$!
  seen=0
  last=$(date +%s)
  deadline=$(( $(date +%s) + minutes * 60 ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    lines="$(grep -a 'aroh-dev: ' "$appdir/dev-marks.log" 2>/dev/null || true)"
    count="$(printf '%s\n' "$lines" | grep -c 'aroh-dev: ' || true)"
    while [ "$seen" -lt "$count" ]; do
      seen=$((seen + 1))
      last=$(date +%s)
      line="$(printf '%s\n' "$lines" | sed -n "${seen}p")"
      echo "$line"
      case "$line" in
        *"aroh-dev: shot 99-failure"*)
          xcrun simctl io "$udid" screenshot "$state/gate4-failure.png" >/dev/null 2>&1
          kill "$launcher" 2>/dev/null || true
          return 1
          ;;
        *"aroh-dev: shot "*) sleep 1; xcrun simctl io "$udid" screenshot "$shots/${line##*aroh-dev: shot }.png" >/dev/null 2>&1 ;;
        *"aroh-dev: appearance "*) xcrun simctl ui "$udid" appearance "${line##*aroh-dev: appearance }" ;;
        *"aroh-dev: result "*) echo "${line##*aroh-dev: result }" >> "$results" ;;
        *"aroh-dev: tap"*) echo "${line##*aroh-dev: }" >> "$taps" ;;
        *"aroh-dev: done"*)
          kill "$launcher" 2>/dev/null || true
          return 0
          ;;
      esac
    done
    kill -0 "$launcher" 2>/dev/null || { echo "app exited"; return 1; }
    if [ $(( $(date +%s) - last )) -gt 300 ]; then
      xcrun simctl io "$udid" screenshot "$state/gate4-stall.png" >/dev/null 2>&1
      kill "$launcher" 2>/dev/null || true
      echo "stalled: no mark for 5 minutes"
      return 1
    fi
    sleep 1
  done
  kill "$launcher" 2>/dev/null || true
  echo "timed out"
  return 1
}

drive gate4-setup.js 10
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
sleep 2
python3 "$here/gate4_seed.py" "$appdir/aroh-store.sqlite3" "$zone"
code=0
drive gate4.js "${AROH_GATE_MINUTES:-150}" || code=$?
xcrun simctl ui "$udid" appearance light
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
echo "results:"
cat "$results"
if grep -q '^[^ ]* fail' "$results"; then code=1; fi
exit "$code"
