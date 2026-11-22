#!/bin/sh
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
state="$here/.local-state"
shots="$root/docs/screenshots/pod12"
bundle=io.aroh.app
device="${AROH_SIM_DEVICE:-aroh-p12}"

udid="$(xcrun simctl list devices | sed -n "s/^ *$device (\([0-9A-F-]*\)).*/\1/p" | head -1)"
[ -n "$udid" ] || { echo "no $device simulator"; exit 1; }
xcrun simctl boot "$udid" 2>/dev/null || true
xcrun simctl bootstatus "$udid" -b >/dev/null
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
container="$(xcrun simctl get_app_container "$udid" "$bundle" data)"
rm -rf "$container/Library/Application Support/$bundle"
mkdir -p "$state" "$shots"
cp "$here/pod12.js" "$state/pod12.js"
log="$container/Library/Application Support/$bundle/dev-marks.log"
SIMCTL_CHILD_AROH_DEV_SCRIPT="$state/pod12.js" xcrun simctl launch --console --terminate-running-process "$udid" "$bundle" > "$state/pod12-app.log" 2>&1 &
launcher=$!
seen=0
deadline=$(( $(date +%s) + 300 ))
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
        [ "$name" = "99-failure" ] && { kill "$launcher" 2>/dev/null || true; exit 1; }
        ;;
      *"aroh-dev: done"*)
        kill "$launcher" 2>/dev/null || true
        exit 0
        ;;
    esac
  done
  sleep 1
done
kill "$launcher" 2>/dev/null || true
echo "timed out"
exit 1
