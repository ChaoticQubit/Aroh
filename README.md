# Aroh

Aroh is a goal-based calendar. You set a goal with a deadline (for example "finish this course by March 1", with its link). An agent reads the course, asks you a few questions about how much time you have, and plans actions into your calendar up to the deadline. It reminds you before each action. When you miss one, it replans the rest so the goal still lands on time. A small Log screen tracks food, water and workouts by text, quick chips or voice.

The iPhone app is the main target. Android builds from the same code, and the Mac app comes second. The UI is Svelte 5 + SvelteKit inside Tauri 2, the core is Rust, agents use Claude through `rig-core`, and data is local SQLite with optional Supabase sync. YAML plugins place fixed components into named slots (see `docs/plugins.md`).

## Layout

- `apps/aroh` - the app: Svelte UI in `src/`, Tauri shell in `src-tauri/`
- `crates/` - Rust core: `aroh-core` (data, keychain), `aroh-agents`, `aroh-replan`, `aroh-reminders`, `aroh-intake`, `aroh-plugins`, `aroh-sync`, `tauri-plugin-aroh-speech`
- `docs/decisions.md` - settled decisions

## Tests

Workspace tests (all crates except the Tauri app) run in a small Kiri container. Agent shells have a sandbox HOME, so prefix `kiri` with the owner's HOME:

```sh
HOME=/Users/chaoticqubit kiri build -f Dockerfile.test -t aroh-test .
HOME=/Users/chaoticqubit kiri run --rm aroh-test
```

Type check and build of the UI:

```sh
cd apps/aroh
pnpm install
pnpm check
```

Import shared UI code with `$lib/...`. It resolves through `paths` in `apps/aroh/tsconfig.json` and `resolve.alias` in `apps/aroh/vite.config.ts`, because SvelteKit 3 no longer provides `$lib` by itself.

## Run on Mac

```sh
cd apps/aroh
pnpm install
pnpm tauri dev
```

`pnpm tauri dev` serves the UI on port 1420 and opens the app window. For a debug app without a dev server, run `pnpm tauri build --debug --no-bundle` and start `target/debug/aroh` from the repo root. Windows 768 px and wider show the sidebar layout; narrower ones show the phone tab bar. On the Mac, reminders come from an in-app timer, so they only fire while the app runs.

Debug builds read two optional variables: `AROH_DEV_CLAUDE_KEY` saves a Claude key to the keychain, and `AROH_DEV_SCRIPT` names a JavaScript file that runs in the window 3 s after start. Release builds contain neither.

## Run in the iOS simulator

Do not run plain `tauri ios dev`: it tries to Homebrew-install `ios-deploy`. `tauri ios build` also fails here ("Xcode Simulator SDK 27.2 is not installed"), so build by hand:

1. Build the Rust library for the simulator, from the repo root:
   `SDKROOT=$(xcrun --sdk iphonesimulator --show-sdk-path) cargo rustc -p aroh --lib --crate-type staticlib --target aarch64-apple-ios-sim`
2. Copy `target/aarch64-apple-ios-sim/debug/libaroh_lib.a` to `apps/aroh/src-tauri/gen/apple/Externals/arm64/debug/libapp.a`.
3. In `apps/aroh/src-tauri/gen/apple`, with a no-op `pnpm` shim first on `PATH`, run:
   `xcodebuild -project aroh.xcodeproj -scheme aroh_iOS -configuration debug -sdk iphonesimulator -destination id=<simulator id> -derivedDataPath <dir> ARCHS=arm64 CODE_SIGN_IDENTITY=- CODE_SIGN_STYLE=Manual DEVELOPMENT_TEAM= PROVISIONING_PROFILE_SPECIFIER= build`
   Ad-hoc signing (`CODE_SIGN_IDENTITY=-`) is required, or the keychain fails with "keychain platform failure".
4. In `apps/aroh`, run `pnpm dev` (port 1420).
5. Install and launch on the iPhone 15 Pro (iOS 26 runtime):
   `xcrun simctl install booted <dir>/Build/Products/debug-iphonesimulator/Aroh.app`, then `xcrun simctl launch booted io.aroh.app`, and capture with `xcrun simctl io booted screenshot <file>`.

Port 1420 is shared, so run one simulator check at a time.

## Android

Android is not set up on this Mac yet. It needs the Android SDK (platform, build-tools, platform-tools), the NDK, JDK 17 or newer, the four Rust Android targets, and `ANDROID_HOME`, `NDK_HOME` and `JAVA_HOME` set. Then run `pnpm tauri android init` and `pnpm tauri android build --debug`.
