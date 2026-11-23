#!/bin/sh
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
state="$here/.local-state"
device="${AROH_E2E_DEVICE:-aroh-p20}"
port="${AROH_DEV_PORT:-1428}"
mode="${1:-run}"
export AROH_E2E_POD="${AROH_E2E_POD:-aroh-p20}" AROH_E2E_SUPABASE_PORT="${AROH_E2E_SUPABASE_PORT:-54420}" AROH_E2E_ENGINE_PORT="${AROH_E2E_ENGINE_PORT:-8420}"
export AROH_E2E_DEVICE="$device" AROH_E2E_SHOTS=pod20 AROH_E2E_FLOW=pod20.js
mkdir -p "$state"

udid="$(xcrun simctl list devices | sed -n "s/^ *$device (\([0-9A-F-]*\)).*/\1/p" | head -1)"
[ -n "$udid" ] || udid="$(xcrun simctl create "$device" "iPhone 15 Pro")"

if [ "$mode" = build ]; then
  curl -fsS -o /dev/null "http://localhost:$port" || { echo "start pnpm dev --port $port --strictPort in apps/aroh first"; exit 1; }
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
    -derivedDataPath "$state/p20-dd" ARCHS=arm64 CODE_SIGN_IDENTITY=- CODE_SIGN_STYLE=Manual DEVELOPMENT_TEAM= PROVISIONING_PROFILE_SPECIFIER= build \
    > "$state/p20-build.log" 2>&1 || { tail -30 "$state/p20-build.log"; exit 1; }
  xcrun simctl boot "$udid" 2>/dev/null || true
  xcrun simctl bootstatus "$udid" -b >/dev/null
  xcrun simctl install "$udid" "$state/p20-dd/Build/Products/debug-iphonesimulator/Aroh.app"
  echo "installed on $device ($udid)"
  exit 0
fi

curl -fsS -o /dev/null "http://localhost:$port" || { echo "start pnpm dev --port $port --strictPort in apps/aroh first"; exit 1; }
sh "$here/up.sh"
code=0
sh "$here/sim.sh" || code=$?
if [ "$code" -eq 0 ]; then
  fixed="$(kiri exec -i "$AROH_E2E_POD-pg-1" psql -U postgres -At -c "select string_agg(title, ',' order by title) from public.actions where locked and not deleted and status = 'planned'")"
  days="$(kiri exec -i "$AROH_E2E_POD-pg-1" psql -U postgres -At -c "select string_agg(title || '=' || day_kind, ',' order by title) from public.busy_blocks where all_day and not deleted")"
  rules="$(kiri exec -i "$AROH_E2E_POD-pg-1" psql -U postgres -At -c "select string_agg(group_name || '=' || decision, ',' order by group_name) from public.calendar_rules where not deleted")"
  echo "server fixed actions: $fixed"
  echo "server all-day kinds: $days"
  echo "server calendar rules: $rules"
  [ "$fixed" = "Dentist" ] || code=1
  [ "$days" = "Conference=busy" ] || code=1
fi
sh "$here/down.sh"
kiri volume rm "$AROH_E2E_POD-target" "$AROH_E2E_POD-cargo" >/dev/null 2>&1 || true
exit "$code"
