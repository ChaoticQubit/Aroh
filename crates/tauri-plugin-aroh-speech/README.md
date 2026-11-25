# tauri-plugin-aroh-speech

On-device voice input for Aroh's log screen. Tauri 2 plugin named `aroh-speech`.

| Target | Behaviour |
|---|---|
| iOS | Swift `SpeechPlugin`: `SFSpeechRecognizer` with `requiresOnDeviceRecognition = true`, audio from `AVAudioEngine`. Audio never leaves the device. |
| macOS, Windows, Linux, Android | `start` and `stop` reject with code `unsupported`. Mac support is next; Android is a known gap. |

## Commands

| Command | Resolves | Notes |
|---|---|---|
| `plugin:aroh-speech\|start` | nothing | Asks for speech and microphone permission on first use, then listens. |
| `plugin:aroh-speech\|stop` | `{ transcript: string }` | Ends the audio and waits up to 2 s for the final result. `transcript` is the whole session. |

Events (plugin listener, `addPluginListener('aroh-speech', ...)`):

- `partial` with `{ text }`: the whole session transcript so far, while listening.
- `ended` with `{ text }`: recognition ended on its own (silence, error or the system time limit). The next `stop` still resolves this text.

The on-device recognizer can restart `formattedString` from empty after a pause, so `text` and `transcript` are never just the recognizer's latest string. `Transcript` (`ios/Sources/Transcript.swift`) keeps each finished utterance and joins it with the live one: a result with speech metadata marks the end of an utterance, and the next result that does not continue it starts a new one; without that mark, a result under half the old word count, or a changed first word once the old utterance is long enough, starts a new one. Blank results never wipe text. When a restart and a revision cannot be told apart, words are kept, so the failure is duplication, not loss.

Every rejection is `{ code, message }`. `message` is fit to show the owner.

| Code | When |
|---|---|
| `unsupported` | Not iOS. |
| `mic_denied` | Microphone access is off for the app. |
| `speech_denied` | Speech recognition is off for the app. |
| `on_device_unavailable` | The device or the current language has no on-device model. Aroh never falls back to server recognition. |
| `recognizer_unavailable` | The recognizer is temporarily unavailable. |
| `busy` | `start` while listening, or `stop` while a stop is in progress. |
| `not_started` | `stop` with nothing running and no unclaimed transcript. |
| `audio_failed` | The audio session or engine could not start. |

## Guest bindings

The plugin stays registered in the app. Its MVP guest wrapper went with the MVP log screen in pod 10; the rebuild quick-log bar has a mic button that is not wired to it yet.

## Wiring into the app

1. Add the crate to `apps/aroh/src-tauri/Cargo.toml`: `tauri-plugin-aroh-speech = { path = "../../../crates/tauri-plugin-aroh-speech" }`.
2. Register it in `lib.rs`: `.plugin(tauri_plugin_aroh_speech::init())`.
3. Add `"aroh-speech:default"` to the permissions of the app capability.
4. Add these keys to the iOS `Info.plist` (`apps/aroh/src-tauri/gen/apple/aroh_iOS/Info.plist`, or through `bundle.iOS` config). iOS terminates the app on first use without them.

| Key | Suggested string |
|---|---|
| `NSMicrophoneUsageDescription` | Aroh uses the microphone so you can say what you ate, drank or did instead of typing it. |
| `NSSpeechRecognitionUsageDescription` | Aroh turns your voice into log entries on this iPhone. Your audio is not sent anywhere. |

## Building

```sh
cargo build --target aarch64-apple-ios-sim
```

Runs on the Mac, outside containers, because it needs Xcode. The build script copies Tauri's Swift API into `.tauri/` and compiles the Swift package with swift-rs, so this command also proves the Swift code compiles against the iOS simulator SDK.

## Checks

`Transcript` depends only on Foundation. Its assert-based checks in `ios/Checks/main.swift` run on the Mac without the Tauri package; they print counts and fixture names only, never live transcript text:

```sh
swiftc -o "$TMPDIR/tcheck" ios/Sources/Transcript.swift ios/Checks/main.swift && "$TMPDIR/tcheck"
```
