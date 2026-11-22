#!/bin/sh
set -eu
mode="${1:-device}"
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
apple="$here/src-tauri/gen/apple"
out="${AROH_IOS_OUT:-$root/target/ios-release}"
case "$mode" in
  device|testflight) target=aarch64-apple-ios; sdk=iphoneos ;;
  simulator) target=aarch64-apple-ios-sim; sdk=iphonesimulator ;;
  *) echo "usage: ios-release.sh [device|simulator|testflight]"; exit 2 ;;
esac
if [ "$mode" = testflight ]; then
  team="${AROH_TEAM_ID:-GK729B5HZQ}"
  build="${AROH_BUILD_NUMBER:-$(date -u +%Y%m%d%H%M)}"
  case "$build" in
    ''|*[!0-9.]*|.*|*.|*..*) echo "AROH_BUILD_NUMBER must be digits and single dots: $build"; exit 2 ;;
  esac
  key_set=0
  for v in "${AROH_ASC_KEY_PATH:-}" "${AROH_ASC_KEY_ID:-}" "${AROH_ASC_ISSUER_ID:-}"; do
    if [ -n "$v" ]; then key_set=$((key_set + 1)); fi
  done
  case "$key_set" in
    0) set -- ;;
    3)
      if [ ! -f "$AROH_ASC_KEY_PATH" ]; then echo "AROH_ASC_KEY_PATH is not a file: $AROH_ASC_KEY_PATH"; exit 2; fi
      set -- -authenticationKeyPath "$AROH_ASC_KEY_PATH" -authenticationKeyID "$AROH_ASC_KEY_ID" -authenticationKeyIssuerID "$AROH_ASC_ISSUER_ID"
      ;;
    *) echo "set all of AROH_ASC_KEY_PATH, AROH_ASC_KEY_ID, AROH_ASC_ISSUER_ID, or none"; exit 2 ;;
  esac
fi
shim="$(mktemp -d)"
trap 'rm -rf "$shim"' EXIT
trap 'exit 130' INT TERM
printf '#!/bin/sh\nexit 0\n' > "$shim/pnpm"
chmod +x "$shim/pnpm"
(cd "$here" && pnpm build)
(cd "$root" && SDKROOT="$(xcrun --sdk "$sdk" --show-sdk-path)" cargo rustc -p aroh --lib --crate-type staticlib --release --features custom-protocol --target "$target")
mkdir -p "$apple/Externals/arm64/release" "$apple/assets" "$out"
cp "$root/target/$target/release/libaroh_lib.a" "$apple/Externals/arm64/release/libapp.a"
cd "$apple"
if [ "$mode" = device ]; then
  PATH="$shim:$PATH" xcodebuild -project aroh.xcodeproj -scheme aroh_iOS -configuration release -sdk iphoneos -destination generic/platform=iOS -archivePath "$out/Aroh.xcarchive" ARCHS=arm64 CODE_SIGNING_ALLOWED=NO archive
  echo "archive: $out/Aroh.xcarchive"
elif [ "$mode" = simulator ]; then
  PATH="$shim:$PATH" xcodebuild -project aroh.xcodeproj -scheme aroh_iOS -configuration release -sdk iphonesimulator -destination "generic/platform=iOS Simulator" -derivedDataPath "$out/dd" ARCHS=arm64 CODE_SIGN_IDENTITY=- CODE_SIGN_STYLE=Manual DEVELOPMENT_TEAM= PROVISIONING_PROFILE_SPECIFIER= build
  echo "app: $out/dd/Build/Products/release-iphonesimulator/Aroh.app"
else
  plist="$shim/Info.plist"
  cp aroh_iOS/Info.plist "$plist"
  /usr/libexec/PlistBuddy -c "Set :CFBundleVersion $build" "$plist"
  [ "$(/usr/libexec/PlistBuddy -c 'Print :ITSAppUsesNonExemptEncryption' "$plist")" = false ] || { echo "Info.plist must set ITSAppUsesNonExemptEncryption to false"; exit 1; }
  archive="$out/testflight/Aroh-$build.xcarchive"
  opts="$out/testflight/ExportOptions-$build.plist"
  mkdir -p "$out/testflight"
  plutil -create xml1 "$opts"
  plutil -insert method -string app-store-connect "$opts"
  plutil -insert destination -string upload "$opts"
  plutil -insert teamID -string "$team" "$opts"
  plutil -insert signingStyle -string automatic "$opts"
  plutil -insert manageAppVersionAndBuildNumber -bool NO "$opts"
  plutil -insert testFlightInternalTestingOnly -bool YES "$opts"
  plutil -insert uploadSymbols -bool YES "$opts"
  plutil -lint "$opts"
  if [ "${AROH_UNSIGNED:-0}" = 1 ]; then
    PATH="$shim:$PATH" xcodebuild -project aroh.xcodeproj -scheme aroh_iOS -configuration release -sdk iphoneos -destination generic/platform=iOS -archivePath "$archive" ARCHS=arm64 INFOPLIST_FILE="$plist" CODE_SIGNING_ALLOWED=NO archive
  else
    PATH="$shim:$PATH" xcodebuild -project aroh.xcodeproj -scheme aroh_iOS -configuration release -sdk iphoneos -destination generic/platform=iOS -archivePath "$archive" -allowProvisioningUpdates "$@" ARCHS=arm64 INFOPLIST_FILE="$plist" DEVELOPMENT_TEAM="$team" CODE_SIGN_STYLE=Automatic CODE_SIGN_IDENTITY="Apple Development" archive
  fi
  app="$archive/Products/Applications/Aroh.app"
  [ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$app/Info.plist")" = "$build" ] || { echo "archived app does not carry build $build"; exit 1; }
  [ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$app/Info.plist")" = io.aroh.app ] || { echo "archived app is not io.aroh.app"; exit 1; }
  echo "archive: $archive (build $build)"
  if [ "${AROH_UNSIGNED:-0}" = 1 ]; then
    echo "unsigned: export and upload skipped, options: $opts"
    exit 0
  fi
  xcodebuild -exportArchive -archivePath "$archive" -exportPath "$out/testflight/export-$build" -exportOptionsPlist "$opts" -allowProvisioningUpdates "$@"
  echo "uploaded build $build of io.aroh.app to App Store Connect"
fi
