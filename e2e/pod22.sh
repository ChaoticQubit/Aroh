#!/bin/sh
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
state="$here/.local-state"
device="${AROH_E2E_DEVICE:-aroh-p22}"
port="${AROH_DEV_PORT:-1429}"
shots="$root/docs/screenshots/pod22"
bundle=io.aroh.app
mode="${1:-run}"
mkdir -p "$state" "$shots"

if [ "$mode" = build ]; then
  AROH_E2E_DEVICE="$device" AROH_DEV_PORT="$port" sh "$here/pod18.sh" build
  exit 0
fi

curl -fsS -o /dev/null "http://localhost:$port" || { echo "start pnpm dev --port $port --strictPort in apps/aroh first"; exit 1; }
udid="$(xcrun simctl list devices | sed -n "s/^ *$device (\([0-9A-F-]*\)).*/\1/p" | head -1)"
[ -n "$udid" ] || { echo "no $device simulator; run sh e2e/pod22.sh build first"; exit 1; }
xcrun simctl boot "$udid" 2>/dev/null || true
xcrun simctl bootstatus "$udid" -b >/dev/null
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
container="$(xcrun simctl get_app_container "$udid" "$bundle" data)"
rm -rf "$container/Library/Application Support/$bundle"
xcrun simctl keychain "$udid" reset
log="$container/Library/Application Support/$bundle/dev-marks.log"

phase() {
  zone="$1"
  config="$2"
  xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
  rm -f "$log"
  { printf 'window.AROH_E2E = %s;\n' "$config"; cat "$here/drive-lib.js" "$here/pod22.js"; } > "$state/pod22-drive.js"
  SIMCTL_CHILD_TZ="$zone" SIMCTL_CHILD_AROH_DEV_SCRIPT="$state/pod22-drive.js" xcrun simctl launch --console --terminate-running-process "$udid" "$bundle" > "$state/pod22-app.log" 2>&1 &
  launcher=$!
  seen=0
  deadline=$(( $(date +%s) + 300 ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    lines="$(grep -a 'aroh-dev: ' "$log" 2>/dev/null || true)"
    count="$(printf '%s\n' "$lines" | grep -c 'aroh-dev: ' || true)"
    while [ "$seen" -lt "$count" ]; do
      seen=$((seen + 1))
      line="$(printf '%s\n' "$lines" | sed -n "${seen}p")"
      echo "$zone $line"
      case "$line" in
        *"aroh-dev: shot 99-failure"*)
          xcrun simctl io "$udid" screenshot "$state/pod22-failure.png" >/dev/null 2>&1
          kill "$launcher" 2>/dev/null || true
          return 1
          ;;
        *"aroh-dev: shot "*) xcrun simctl io "$udid" screenshot "$shots/${line##*aroh-dev: shot }.png" >/dev/null 2>&1 ;;
        *"aroh-dev: day "*) day="${line##*aroh-dev: day }" ;;
        *"aroh-dev: locked "*) locked="${line##*aroh-dev: locked }" ;;
        *"aroh-dev: done"*)
          kill "$launcher" 2>/dev/null || true
          return 0
          ;;
      esac
    done
    sleep 1
  done
  kill "$launcher" 2>/dev/null || true
  echo "timed out in $zone"
  return 1
}

day=""
locked=""
phase Europe/Berlin '{"phase":"home"}'
[ -n "$day" ] && [ -n "$locked" ] || { echo "home phase reported no day or locked action"; exit 1; }
phase America/New_York "{\"phase\":\"away\",\"zone\":\"America/New_York\",\"notes\":1,\"prefix\":\"02-newyork\",\"day\":\"$day\",\"locked\":\"$locked\"}"
phase Europe/Berlin "{\"phase\":\"back\",\"zone\":\"Europe/Berlin\",\"notes\":2,\"prefix\":\"03-berlin-back\",\"day\":\"$day\",\"locked\":\"$locked\"}"
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
echo "pod22 travel flow passed; screenshots in $shots"
