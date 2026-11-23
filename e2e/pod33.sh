#!/bin/sh
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
state="$here/.local-state"
device="${AROH_E2E_DEVICE:-aroh-p33}"
port="${AROH_DEV_PORT:?set AROH_DEV_PORT to the vite dev port}"
mode="${1:-run}"
bundle=io.aroh.app
shots="$root/docs/screenshots/pod33"
mkdir -p "$state" "$shots"

udid="$(xcrun simctl list devices | sed -n "s/^ *$device (\([0-9A-F-]*\)).*/\1/p" | head -1)"
[ -n "$udid" ] || udid="$(xcrun simctl create "$device" "iPhone 15 Pro")"
curl -fsS -o /dev/null "http://localhost:$port" || { echo "start pnpm dev --port $port --strictPort in apps/aroh first"; exit 1; }

if [ "$mode" = build ]; then
  cd "$root"
  TAURI_CONFIG="{\"build\":{\"devUrl\":\"http://localhost:$port\"}}" SDKROOT="$(xcrun --sdk iphonesimulator --show-sdk-path)" \
    cargo rustc -p aroh --lib --crate-type staticlib --target aarch64-apple-ios-sim
  mkdir -p apps/aroh/src-tauri/gen/apple/Externals/arm64/debug apps/aroh/src-tauri/gen/apple/assets
  cp target/aarch64-apple-ios-sim/debug/libaroh_lib.a apps/aroh/src-tauri/gen/apple/Externals/arm64/debug/libapp.a
  mkdir -p "$state/shim"
  printf '#!/bin/sh\nexit 0\n' > "$state/shim/pnpm"
  chmod +x "$state/shim/pnpm"
  cd apps/aroh/src-tauri/gen/apple
  PATH="$state/shim:$PATH" xcodebuild -project aroh.xcodeproj -scheme aroh_iOS -configuration debug -sdk iphonesimulator -destination "id=$udid" \
    -derivedDataPath "$state/p33-dd" ARCHS=arm64 CODE_SIGN_IDENTITY=- CODE_SIGN_STYLE=Manual DEVELOPMENT_TEAM= PROVISIONING_PROFILE_SPECIFIER= build \
    > "$state/p33-build.log" 2>&1 || { grep -E "error:" "$state/p33-build.log" | head -20; tail -30 "$state/p33-build.log"; exit 1; }
  xcrun simctl boot "$udid" 2>/dev/null || true
  xcrun simctl bootstatus "$udid" -b >/dev/null
  xcrun simctl install "$udid" "$state/p33-dd/Build/Products/debug-iphonesimulator/Aroh.app"
  echo "installed on $device ($udid)"
  exit 0
fi

xcrun simctl boot "$udid" 2>/dev/null || true
xcrun simctl bootstatus "$udid" -b >/dev/null
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
container="$(xcrun simctl get_app_container "$udid" "$bundle" data)"
rm -rf "$container/Library/Application Support/$bundle" "$container/Library/Application Support/aroh-location"
xcrun simctl keychain "$udid" reset
xcrun simctl shutdown "$udid" 2>/dev/null || true
python3 "$here/grant.py" "$HOME/Library/Developer/CoreSimulator/Devices/$udid/data" "$bundle"
xcrun simctl boot "$udid"
xcrun simctl bootstatus "$udid" -b >/dev/null

{ echo "window.AROH_E2E = {};"; cat "$here/drive-lib.js" "$here/pod33.js"; } > "$state/pod33-drive.js"
log="$container/Library/Application Support/$bundle/dev-marks.log"
SIMCTL_CHILD_AROH_DEV_SCRIPT="$state/pod33-drive.js" xcrun simctl launch --console --terminate-running-process "$udid" "$bundle" > "$state/pod33-app.log" 2>&1 &
launcher=$!
seen=0
code=1
deadline=$(( $(date +%s) + 1200 ))
while [ "$(date +%s)" -lt "$deadline" ]; do
  lines="$(grep -a 'aroh-dev: ' "$log" 2>/dev/null || true)"
  count="$(printf '%s\n' "$lines" | grep -c 'aroh-dev: ' || true)"
  while [ "$seen" -lt "$count" ]; do
    seen=$((seen + 1))
    line="$(printf '%s\n' "$lines" | sed -n "${seen}p")"
    echo "$line"
    case "$line" in
      *"aroh-dev: shot 99-failure"*)
        xcrun simctl io "$udid" screenshot "$shots/99-failure.png" >/dev/null 2>&1
        break 2
        ;;
      *"aroh-dev: shot "*)
        xcrun simctl io "$udid" screenshot "$shots/${line##*aroh-dev: shot }.png" >/dev/null 2>&1
        ;;
      *"aroh-dev: locate "*)
        xcrun simctl location "$udid" set "${line##*aroh-dev: locate }"
        ;;
      *"aroh-dev: burst "*)
        name="${line##*aroh-dev: burst }"
        for i in 01 02 03 04 05 06 07 08 09 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30; do
          xcrun simctl io "$udid" screenshot "$state/burst-$i.png" >/dev/null 2>&1
          sleep 1
        done
        cp "$(ls -S "$state"/burst-*.png | head -1)" "$shots/$name.png"
        rm -f "$state"/burst-*.png
        ;;
      *"aroh-dev: done"*)
        code=0
        break 2
        ;;
    esac
  done
  kill -0 "$launcher" 2>/dev/null || { echo "app exited"; break; }
  sleep 1
done
kill "$launcher" 2>/dev/null || true
[ "$code" -eq 0 ] && echo "done" || echo "failed"
exit "$code"
