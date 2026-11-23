#!/bin/sh
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
state="$here/.local-state"
device="${AROH_E2E_DEVICE:-aroh-p37}"
port="${AROH_DEV_PORT:-1443}"
mode="${1:-run}"
bundle=io.aroh.app
shots="$root/docs/screenshots/pod37"
theme="${AROH_THEME:-light}"
mkdir -p "$state" "$shots/light" "$shots/dark"

udid="$(xcrun simctl list devices | sed -n "s/^ *$device (\([0-9A-F-]*\)).*/\1/p" | head -1)"
[ -n "$udid" ] || udid="$(xcrun simctl create "$device" "iPhone 15 Pro")"
curl -fsS -o /dev/null "http://localhost:$port" || { echo "start pnpm dev --port $port --strictPort in apps/aroh first"; exit 1; }

if [ "$mode" = build ]; then
  cd "$root"
  [ -z "${AROH_PROD:-}" ] || (cd apps/aroh && pnpm build >/dev/null)
  TAURI_CONFIG="{\"build\":{\"devUrl\":\"http://localhost:$port\"}}" SDKROOT="$(xcrun --sdk iphonesimulator --show-sdk-path)" \
    cargo rustc -p aroh --lib --crate-type staticlib --target aarch64-apple-ios-sim ${AROH_PROD:+--features tauri/custom-protocol}
  mkdir -p apps/aroh/src-tauri/gen/apple/Externals/arm64/debug apps/aroh/src-tauri/gen/apple/assets
  cp target/aarch64-apple-ios-sim/debug/libaroh_lib.a apps/aroh/src-tauri/gen/apple/Externals/arm64/debug/libapp.a
  mkdir -p "$state/shim"
  printf '#!/bin/sh\nexit 0\n' > "$state/shim/pnpm"
  chmod +x "$state/shim/pnpm"
  cd apps/aroh/src-tauri/gen/apple
  PATH="$state/shim:$PATH" xcodebuild -project aroh.xcodeproj -scheme aroh_iOS -configuration debug -sdk iphonesimulator -destination "id=$udid" \
    -derivedDataPath "$state/p37-dd" ARCHS=arm64 CODE_SIGN_IDENTITY=- CODE_SIGN_STYLE=Manual DEVELOPMENT_TEAM= PROVISIONING_PROFILE_SPECIFIER= build \
    > "$state/p37-build.log" 2>&1 || { grep -E "error:" "$state/p37-build.log" | head -20; tail -30 "$state/p37-build.log"; exit 1; }
  xcrun simctl boot "$udid" 2>/dev/null || true
  xcrun simctl bootstatus "$udid" -b >/dev/null
  xcrun simctl install "$udid" "$state/p37-dd/Build/Products/debug-iphonesimulator/Aroh.app"
  echo "installed on $device ($udid)"
  exit 0
fi

xcrun simctl boot "$udid" 2>/dev/null || true
xcrun simctl bootstatus "$udid" -b >/dev/null
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
container="$(xcrun simctl get_app_container "$udid" "$bundle" data)"
rm -rf "$container/Library/Application Support/$bundle"
xcrun simctl keychain "$udid" reset
mkdir -p "$container/Library/Application Support/$bundle"
log="$container/Library/Application Support/$bundle/dev-marks.log"
rm -f "$log"
xcrun simctl ui "$udid" appearance "$theme"
{ echo "window.AROH_E2E = { theme: '$theme', kb: ${AROH_P37_KB:-false} };"; cat "$here/drive-lib.js" "$here/${AROH_FLOW:-pod37.js}"; } > "$state/pod37-drive.js"
SIMCTL_CHILD_AROH_DEV_SCRIPT="$state/pod37-drive.js" xcrun simctl launch --console --terminate-running-process "$udid" "$bundle" > "$state/pod37-app.log" 2>&1 &
launcher=$!
seen=0
code=1
deadline=$(( $(date +%s) + 600 ))
while [ "$(date +%s)" -lt "$deadline" ]; do
  lines="$(grep -a 'aroh-dev: ' "$log" 2>/dev/null || true)"
  count="$(printf '%s\n' "$lines" | grep -c 'aroh-dev: ' || true)"
  while [ "$seen" -lt "$count" ]; do
    seen=$((seen + 1))
    line="$(printf '%s\n' "$lines" | sed -n "${seen}p")"
    echo "$line"
    case "$line" in
      *"aroh-dev: shot 99-failure"*)
        break 2
        ;;
      *"aroh-dev: shot "*)
        xcrun simctl io "$udid" screenshot "$shots/${line##*aroh-dev: shot }.png" >/dev/null 2>&1
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
