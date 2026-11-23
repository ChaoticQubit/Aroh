#!/bin/sh
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
state="$here/.local-state"
device="${AROH_E2E_DEVICE:-aroh-p43}"
port="${AROH_DEV_PORT:-1446}"
mode="${1:-run}"
shots="$root/docs/screenshots/pod43"
bundle=io.aroh.app
mkdir -p "$state" "$shots/light" "$shots/dark"

if [ "$mode" = build ]; then
  AROH_E2E_DEVICE="$device" AROH_DEV_PORT="$port" sh "$here/pod18.sh" build
  exit 0
fi

curl -fsS -o /dev/null "http://localhost:$port" || { echo "start pnpm dev --port $port --strictPort in apps/aroh first"; exit 1; }
udid="$(xcrun simctl list devices | sed -n "s/^ *$device (\([0-9A-F-]*\)).*/\1/p" | head -1)"
[ -n "$udid" ] || { echo "no $device simulator; run sh e2e/pod43.sh build first"; exit 1; }
xcrun simctl boot "$udid" 2>/dev/null || true
xcrun simctl bootstatus "$udid" -b >/dev/null
container="$(xcrun simctl get_app_container "$udid" "$bundle" data)"
log="$container/Library/Application Support/$bundle/dev-marks.log"

run() {
  theme="$1"
  xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
  rm -rf "$container/Library/Application Support/$bundle"
  xcrun simctl keychain "$udid" reset
  xcrun simctl ui "$udid" appearance "$theme"
  { printf 'window.AROH_E2E = {"prefix":"%s"};\n' "$theme"; cat "$here/drive-lib.js" "$here/pod43.js"; } > "$state/pod43-drive.js"
  SIMCTL_CHILD_AROH_DEV_SCRIPT="$state/pod43-drive.js" xcrun simctl launch --console --terminate-running-process "$udid" "$bundle" > "$state/pod43-app.log" 2>&1 &
  launcher=$!
  seen=0
  deadline=$(( $(date +%s) + 300 ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    lines="$(grep -a 'aroh-dev: ' "$log" 2>/dev/null || true)"
    count="$(printf '%s\n' "$lines" | grep -c 'aroh-dev: ' || true)"
    while [ "$seen" -lt "$count" ]; do
      seen=$((seen + 1))
      line="$(printf '%s\n' "$lines" | sed -n "${seen}p")"
      echo "$theme $line"
      case "$line" in
        *"aroh-dev: shot 99-failure"*)
          xcrun simctl io "$udid" screenshot "$state/pod43-failure.png" >/dev/null 2>&1
          kill "$launcher" 2>/dev/null || true
          return 1
          ;;
        *"aroh-dev: shot "*)
          name="${line##*aroh-dev: shot }"
          xcrun simctl io "$udid" screenshot "$shots/$theme/${name#"$theme"-}.png" >/dev/null 2>&1
          ;;
        *"aroh-dev: overflow "*)
          rest="${line##*aroh-dev: overflow }"
          rest="${rest#* }"
          [ "${rest%% *}" = 0 ] || bad="$bad $theme:${line##*aroh-dev: overflow }"
          ;;
        *"aroh-dev: phase "*) [ "${line##*aroh-dev: phase }" = not_started ] || bad="$bad $theme:phase" ;;
        *"aroh-dev: overlaps "*) [ "${line##*aroh-dev: overlaps }" -ge 2 ] || bad="$bad $theme:overlaps" ;;
        *"aroh-dev: dismissed "*) [ "${line##*aroh-dev: dismissed }" = true ] || bad="$bad $theme:dismiss" ;;
        *"aroh-dev: done"*)
          kill "$launcher" 2>/dev/null || true
          return 0
          ;;
      esac
    done
    sleep 1
  done
  kill "$launcher" 2>/dev/null || true
  echo "timed out"
  return 1
}

bad=""
run light
run dark
xcrun simctl ui "$udid" appearance light
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
echo "pod43 problems:${bad:- none}"
[ -z "$bad" ]
