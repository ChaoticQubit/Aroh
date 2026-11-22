#!/bin/sh
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
state="$here/.local-state"
shots="$root/docs/screenshots/pod13"
bundle=io.aroh.app
device=aroh-p13
mode="${1:-run}"
mkdir -p "$state" "$shots"

udid="$(xcrun simctl list devices | sed -n "s/^ *$device (\([0-9A-F-]*\)).*/\1/p" | head -1)"
[ -n "$udid" ] || { echo "no $device simulator; xcrun simctl create $device \"iPhone 15 Pro\""; exit 1; }

if [ "$mode" = build ]; then
  port="${AROH_DEV_PORT:-1420}"
  curl -fsS -o /dev/null "http://localhost:$port" || { echo "start USER=\$USER pnpm tauri ios dev $device in apps/aroh first, serving port $port"; exit 1; }
  cd "$root/apps/aroh/src-tauri/gen/apple"
  xcodebuild -project aroh.xcodeproj -scheme aroh_iOS -configuration debug -sdk iphonesimulator -destination "id=$udid" -derivedDataPath "$state/app-dd" ARCHS=arm64 build > "$state/build.log" 2>&1 || { tail -30 "$state/build.log"; exit 1; }
  app="$state/app-dd/Build/Products/debug-iphonesimulator/Aroh.app"
  for key in com.apple.developer.healthkit aps-environment; do
    strings "$app/Aroh" | grep -q "<key>$key</key>" || { echo "entitlement $key missing"; exit 1; }
  done
  xcrun simctl boot "$udid" 2>/dev/null || true
  xcrun simctl bootstatus "$udid" -b >/dev/null
  xcrun simctl install "$udid" "$app"
  echo "installed $app"
  exit 0
fi

app="$state/app-dd/Build/Products/debug-iphonesimulator/Aroh.app"
[ -d "$app" ] || { echo "no build; run sh $0 build first"; exit 1; }
xcrun simctl shutdown "$udid" 2>/dev/null || true
xcrun simctl erase "$udid"
xcrun simctl boot "$udid"
xcrun simctl bootstatus "$udid" -b >/dev/null
xcrun simctl install "$udid" "$app"
container="$(xcrun simctl get_app_container "$udid" "$bundle" data)"
xcrun simctl shutdown "$udid" 2>/dev/null || true
python3 "$root/e2e/grant.py" "$HOME/Library/Developer/CoreSimulator/Devices/$udid/data" "$bundle"
xcrun simctl boot "$udid"
xcrun simctl bootstatus "$udid" -b >/dev/null

xcodegen generate --quiet --spec "$here/ui/project.yml" --project "$state"
cp "$here/drive.js" "$state/drive.js"
rm -f "$state/done" "$state/sheet"
log="$container/Library/Application Support/$bundle/dev-marks.log"
TEST_RUNNER_AROH_TAP_SCRIPT="$state/drive.js" TEST_RUNNER_AROH_TAP_DONE="$state/done" TEST_RUNNER_AROH_TAP_SHEET="$state/sheet" \
  xcodebuild test -project "$state/ArohTap.xcodeproj" -scheme Tap -destination "id=$udid" -derivedDataPath "$state/tap-dd" > "$state/tap.log" 2>&1 &
runner=$!
seen=0
status=1
deadline=$(( $(date +%s) + 900 ))
while [ "$(date +%s)" -lt "$deadline" ]; do
  if [ -f "$state/sheet" ]; then
    rm -f "$state/sheet"
    xcrun simctl io "$udid" screenshot "$shots/01-health-permission.png" >/dev/null 2>&1
    echo "shot 01-health-permission"
  fi
  lines="$(grep -a 'aroh-dev: ' "$log" 2>/dev/null || true)"
  count="$(printf '%s\n' "$lines" | grep -c 'aroh-dev: ' || true)"
  while [ "$seen" -lt "$count" ]; do
    seen=$((seen + 1))
    line="$(printf '%s\n' "$lines" | sed -n "${seen}p")"
    echo "$line"
    case "$line" in
      *"aroh-dev: shot 99-failure"*)
        xcrun simctl io "$udid" screenshot "$shots/99-failure.png" >/dev/null 2>&1
        touch "$state/done"
        deadline=0
        ;;
      *"aroh-dev: shot "*)
        name="${line##*aroh-dev: shot }"
        xcrun simctl io "$udid" screenshot "$shots/$name.png" >/dev/null 2>&1
        ;;
      *"aroh-dev: background"*)
        xcrun simctl launch "$udid" com.apple.Preferences >/dev/null
        sleep 3
        xcrun simctl push "$udid" "$bundle" "$here/payload.json"
        for i in 01 02 03 04 05 06 07 08 09 10 11 12 13 14 15 16 17 18 19 20; do
          xcrun simctl io "$udid" screenshot "$state/push-$i.png" >/dev/null 2>&1
        done
        cp "$(ls -S "$state"/push-*.png | head -1)" "$shots/05-push-banner.png"
        rm -f "$state"/push-*.png
        touch "$state/done"
        status=0
        deadline=0
        ;;
    esac
  done
  if ! kill -0 "$runner" 2>/dev/null && [ "$deadline" -ne 0 ]; then
    echo "tap runner exited before the drive finished"
    xcrun simctl io "$udid" screenshot "$shots/99-failure.png" >/dev/null 2>&1
    break
  fi
  sleep 1
done
wait "$runner" || true
grep -E "Test Case|error:" "$state/tap.log" | tail -5
xcrun simctl shutdown "$udid" 2>/dev/null || true
[ "$status" -eq 0 ] && echo "done" || echo "failed"
exit "$status"
