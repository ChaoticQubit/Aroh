#!/bin/sh
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
state="$here/.local-state"
device="${AROH_E2E_DEVICE:-aroh-p31}"
port="${AROH_DEV_PORT:-1436}"
phase="${1:-after}"
shots="$root/docs/screenshots/pod31/$phase"
bundle=io.aroh.app
mkdir -p "$state" "$shots"

if [ "$phase" = build ]; then
  AROH_E2E_DEVICE="$device" AROH_DEV_PORT="$port" sh "$here/pod18.sh" build
  exit 0
fi

curl -fsS -o /dev/null "http://localhost:$port" || { echo "start pnpm dev --port $port --strictPort in apps/aroh first"; exit 1; }
udid="$(xcrun simctl list devices | sed -n "s/^ *$device (\([0-9A-F-]*\)).*/\1/p" | head -1)"
[ -n "$udid" ] || { echo "no $device simulator; run sh e2e/pod31.sh build first"; exit 1; }
xcrun simctl boot "$udid" 2>/dev/null || true
xcrun simctl bootstatus "$udid" -b >/dev/null
container="$(xcrun simctl get_app_container "$udid" "$bundle" data)"
log="$container/Library/Application Support/$bundle/dev-marks.log"

run() {
  theme="$1"
  prefix="$2"
  wanted="$3"
  xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
  rm -rf "$container/Library/Application Support/$bundle"
  xcrun simctl keychain "$udid" reset
  xcrun simctl ui "$udid" appearance "$theme"
  { printf 'window.AROH_E2E = {"prefix":"%s","shots":%s};\n' "$prefix" "$wanted"; cat "$here/drive-lib.js" "$here/pod31.js"; } > "$state/pod31-drive.js"
  SIMCTL_CHILD_AROH_DEV_SCRIPT="$state/pod31-drive.js" xcrun simctl launch --console --terminate-running-process "$udid" "$bundle" > "$state/pod31-app.log" 2>&1 &
  launcher=$!
  seen=0
  deadline=$(( $(date +%s) + 600 ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    lines="$(grep -a 'aroh-dev: ' "$log" 2>/dev/null || true)"
    count="$(printf '%s\n' "$lines" | grep -c 'aroh-dev: ' || true)"
    while [ "$seen" -lt "$count" ]; do
      seen=$((seen + 1))
      line="$(printf '%s\n' "$lines" | sed -n "${seen}p")"
      echo "$theme $line"
      case "$line" in
        *"aroh-dev: shot 99-failure"*)
          xcrun simctl io "$udid" screenshot "$state/pod31-failure.png" >/dev/null 2>&1
          kill "$launcher" 2>/dev/null || true
          return 1
          ;;
        *"aroh-dev: shot "*) xcrun simctl io "$udid" screenshot "$shots/${line##*aroh-dev: shot }.png" >/dev/null 2>&1 ;;
        *"aroh-dev: overflow-routes "*) routes="${line##*aroh-dev: overflow-routes }" ;;
        *"aroh-dev: goals-tab "*) goals_tab="${line##*aroh-dev: goals-tab }" ;;
        *"aroh-dev: calendar-done "*) cal_done="${line##*aroh-dev: calendar-done }" ;;
        *"aroh-dev: swipe ok"*|*"aroh-dev: swipe broken"*) swiped="${line##*aroh-dev: swipe }" ;;
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

routes=""
goals_tab=""
cal_done=""
swiped=""
run light light '["today","calendar","goals","goal","inbox","log","settings"]'
light_routes="$routes"
light_tab="$goals_tab"
light_done="$cal_done"
light_swipe="$swiped"
[ "$phase" = before ] || run dark dark '["today","goal","inbox"]'
xcrun simctl ui "$udid" appearance light
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
echo "routes with horizontal overflow: light $light_routes, dark $routes; goals tab from a goal: light $light_tab, dark $goals_tab; done on calendar: light $light_done, dark $cal_done; swipe: light $light_swipe, dark $swiped"
[ "$light_routes" = 0 ] && [ "$routes" = 0 ] && [ "${light_tab%% *}" = ok ] && [ "${goals_tab%% *}" = ok ] && [ "${light_done%% *}" = ok ] && [ "${cal_done%% *}" = ok ] && [ "$light_swipe" = ok ] && [ "$swiped" = ok ]
