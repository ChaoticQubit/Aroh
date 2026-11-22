#!/bin/sh
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
state="$here/.local-state"
shots="$root/docs/screenshots/pod11"
bundle=io.aroh.app
minutes="${AROH_E2E_REMINDER_MINUTES:-8}"
. "$state/env"

udid="$(xcrun simctl list devices | sed -n 's/^ *aroh-p11 (\([0-9A-F-]*\)).*/\1/p' | head -1)"
[ -n "$udid" ] || { echo "no aroh-p11 simulator; run pnpm tauri ios dev aroh-p11 first"; exit 1; }
xcrun simctl boot "$udid" 2>/dev/null || true
xcrun simctl bootstatus "$udid" -b >/dev/null
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
container="$(xcrun simctl get_app_container "$udid" "$bundle" data)"
rm -rf "$container/Library/Application Support/$bundle"
xcrun simctl keychain "$udid" reset
xcrun simctl shutdown "$udid" 2>/dev/null || true
python3 "$here/grant.py" "$HOME/Library/Developer/CoreSimulator/Devices/$udid/data" "$bundle"
xcrun simctl boot "$udid"
xcrun simctl bootstatus "$udid" -b >/dev/null

zone="${AROH_E2E_ZONE:-$(readlink /etc/localtime | sed 's|.*/zoneinfo/||')}"
python3 "$here/script.py" "$state/script.json" "$zone" "$minutes"
remind_at=$(( ($(date +%s) + minutes * 60) / 60 * 60 ))
stamp="$(date +%s)"
{
  printf 'window.AROH_E2E = %s;\n' "$(python3 -c 'import json,sys; print(json.dumps({"supabaseUrl": "http://127.0.0.1:54321", "anonKey": sys.argv[1], "engineUrl": "http://127.0.0.1:8080", "email": "owner-" + sys.argv[2] + "@example.com", "password": "e2e-password-" + sys.argv[2], "key": "sk-ant-e2e-not-a-real-key-4321"}))' "$AROH_E2E_ANON_KEY" "$stamp")"
  cat "$here/drive.js"
} > "$state/drive.js"

mkdir -p "$shots"
log="$container/Library/Application Support/$bundle/dev-marks.log"
SIMCTL_CHILD_AROH_DEV_SCRIPT="$state/drive.js" xcrun simctl launch --console --terminate-running-process "$udid" "$bundle" > "$state/app.log" 2>&1 &
launcher=$!
seen=0
deadline=$(( $(date +%s) + 1200 ))
while [ "$(date +%s)" -lt "$deadline" ]; do
  lines="$(grep -a 'aroh-dev: ' "$log" 2>/dev/null || true)"
  count="$(printf '%s\n' "$lines" | grep -c 'aroh-dev: ' || true)"
  while [ "$seen" -lt "$count" ]; do
    seen=$((seen + 1))
    line="$(printf '%s\n' "$lines" | sed -n "${seen}p")"
    echo "$line"
    case "$line" in
      *"aroh-dev: shot "*)
        name="${line##*aroh-dev: shot }"
        xcrun simctl io "$udid" screenshot "$shots/$name.png" >/dev/null 2>&1
        ;;
      *"aroh-dev: background"*)
        xcrun simctl launch "$udid" com.apple.Preferences >/dev/null
        wait_for=$(( remind_at - $(date +%s) - 3 ))
        [ "$wait_for" -gt 0 ] && sleep "$wait_for"
        for i in 01 02 03 04 05 06 07 08 09 10 11 12 13 14 15 16 17 18 19 20; do
          xcrun simctl io "$udid" screenshot "$state/reminder-$i.png" >/dev/null 2>&1
        done
        cp "$(ls -S "$state"/reminder-*.png | head -1)" "$shots/14-reminder.png"
        rm -f "$state"/reminder-*.png
        kill "$launcher" 2>/dev/null || true
        echo "done"
        exit 0
        ;;
    esac
  done
  case "$lines" in *"aroh-dev: shot 99-failure"*) kill "$launcher" 2>/dev/null || true; exit 1 ;; esac
  sleep 1
done
kill "$launcher" 2>/dev/null || true
echo "timed out"
exit 1
