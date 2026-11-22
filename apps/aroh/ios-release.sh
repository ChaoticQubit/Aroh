#!/bin/sh
set -eu
mode="${1:-device}"
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
apple="$here/src-tauri/gen/apple"
out="${AROH_IOS_OUT:-$root/target/ios-release}"
case "$mode" in
  device) target=aarch64-apple-ios; sdk=iphoneos ;;
  simulator) target=aarch64-apple-ios-sim; sdk=iphonesimulator ;;
  *) echo "usage: ios-release.sh [device|simulator]"; exit 2 ;;
esac
shim="$(mktemp -d)"
trap 'rm -rf "$shim"' EXIT
printf '#!/bin/sh\nexit 0\n' > "$shim/pnpm"
chmod +x "$shim/pnpm"
(cd "$here" && pnpm build)
(cd "$root" && SDKROOT="$(xcrun --sdk "$sdk" --show-sdk-path)" cargo rustc -p aroh --lib --crate-type staticlib --release --features custom-protocol --target "$target")
mkdir -p "$apple/Externals/arm64/release" "$out"
cp "$root/target/$target/release/libaroh_lib.a" "$apple/Externals/arm64/release/libapp.a"
cd "$apple"
if [ "$mode" = device ]; then
  PATH="$shim:$PATH" xcodebuild -project aroh.xcodeproj -scheme aroh_iOS -configuration release -sdk iphoneos -destination generic/platform=iOS -archivePath "$out/Aroh.xcarchive" ARCHS=arm64 CODE_SIGNING_ALLOWED=NO archive
  echo "archive: $out/Aroh.xcarchive"
else
  PATH="$shim:$PATH" xcodebuild -project aroh.xcodeproj -scheme aroh_iOS -configuration release -sdk iphonesimulator -destination "generic/platform=iOS Simulator" -derivedDataPath "$out/dd" ARCHS=arm64 CODE_SIGN_IDENTITY=- CODE_SIGN_STYLE=Manual DEVELOPMENT_TEAM= PROVISIONING_PROFILE_SPECIFIER= build
  echo "app: $out/dd/Build/Products/release-iphonesimulator/Aroh.app"
fi
