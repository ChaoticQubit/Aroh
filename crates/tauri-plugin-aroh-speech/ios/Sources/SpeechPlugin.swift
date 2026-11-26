import AVFoundation
import Speech
import SwiftRs
import Tauri
import UIKit
import WebKit

/// On-device speech to text for Aroh's log intake.
/// `start` asks for microphone and then speech permission, then streams microphone audio into
/// `SFSpeechRecognizer` with `requiresOnDeviceRecognition`, emitting `partial` events `{text}`.
/// `stop` ends the audio and resolves `{transcript}` with the final text. Every `text` and
/// `transcript` is the whole session so far, kept across the recognizer's restarts after a pause.
/// A `start` always wins: it ends any older session (resolving its waiting stops) instead of
/// refusing. An audio interruption, a media services reset or the app going to the background ends
/// the session as if the recognizer finished.
class SpeechPlugin: Plugin {
  private let engine = AVAudioEngine()
  private let finalWait: TimeInterval = 2.0
  private var request: SFSpeechAudioBufferRecognitionRequest?
  private var task: SFSpeechRecognitionTask?
  private var heard = Transcript()
  private var unclaimed: String?
  private var pendingStops: [Invoke] = []
  private var gate = SessionGate()

  /// Lets the web layout fill the whole screen. Without this the web view's scroll view
  /// shrinks the page by the safe areas while CSS `env(safe-area-inset-*)` still reports
  /// them, so the insets count twice and a blank band shows below the tab bar.
  override func load(webview: WKWebView) {
    super.load(webview: webview)
    webview.scrollView.contentInsetAdjustmentBehavior = .never
    let center = NotificationCenter.default
    center.addObserver(self, selector: #selector(audioInterrupted(_:)), name: AVAudioSession.interruptionNotification, object: nil)
    center.addObserver(self, selector: #selector(audioLost), name: AVAudioSession.mediaServicesWereResetNotification, object: nil)
    center.addObserver(self, selector: #selector(audioLost), name: UIApplication.didEnterBackgroundNotification, object: nil)
  }

  /// Ends the live session, keeping its words, when another app or a call takes the audio.
  @objc private func audioInterrupted(_ note: Notification) {
    guard let raw = note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
      AVAudioSession.InterruptionType(rawValue: raw) == .began
    else { return }
    audioLost()
  }

  /// Ends the live session, keeping its words, when its audio can no longer arrive.
  @objc private func audioLost() {
    DispatchQueue.main.async {
      guard self.gate.live else { return }
      self.finish(self.gate.token)
    }
  }

  @objc public func start(_ invoke: Invoke) {
    DispatchQueue.main.async {
      let (token, replaced) = self.gate.claim()
      if replaced == .running || replaced == .stopping {
        self.release()
      }
      self.authorize { error in
        guard self.gate.isCurrent(token), self.gate.phase == .starting else {
          invoke.reject("A newer voice input took over.", code: "superseded")
          return
        }
        if let (code, message) = error {
          _ = self.gate.end(token)
          invoke.reject(message, code: code)
          return
        }
        self.begin(invoke, token)
      }
    }
  }

  @objc public func stop(_ invoke: Invoke) {
    DispatchQueue.main.async {
      switch self.gate.stop() {
      case .wait(let token, let first):
        self.pendingStops.append(invoke)
        guard first else { return }
        self.stopAudio()
        self.request?.endAudio()
        DispatchQueue.main.asyncAfter(deadline: .now() + self.finalWait) { [weak self] in
          self?.finish(token)
        }
      case .cancelled:
        invoke.resolve(["transcript": ""])
      case .idle:
        if let text = self.unclaimed {
          self.unclaimed = nil
          invoke.resolve(["transcript": text])
        } else {
          invoke.reject("Voice input is not running.", code: "not_started")
        }
      }
    }
  }

  @objc public func openSettings(_ invoke: Invoke) {
    DispatchQueue.main.async {
      guard let url = URL(string: UIApplication.openSettingsURLString) else {
        invoke.reject("Settings could not be opened.", code: "unsupported")
        return
      }
      UIApplication.shared.open(url)
      invoke.resolve()
    }
  }

  /// The recognizer to use: the phone's language when it can run on this iPhone, else US English
  /// on this iPhone, else nil. Recognition never falls back to Apple's servers, so audio stays local.
  private func onDeviceRecognizer() -> SFSpeechRecognizer? {
    let candidates = [SFSpeechRecognizer(locale: Locale.current), SFSpeechRecognizer(locale: Locale(identifier: "en-US"))]
    return candidates.compactMap { $0 }.first { $0.supportsOnDeviceRecognition }
  }

  /// Asks for microphone access, then for speech recognition, in one flow.
  /// Calls `done(nil)` on the main queue when both are granted, or `done((code, message))`
  /// naming the permission that is off, so the owner knows which switch to turn on.
  private func authorize(_ done: @escaping ((String, String)?) -> Void) {
    let micDone: (Bool) -> Void = { granted in
      guard granted else {
        DispatchQueue.main.async {
          done(("mic_denied", "Microphone is off for Aroh. Turn it on in Settings."))
        }
        return
      }
      SFSpeechRecognizer.requestAuthorization { status in
        DispatchQueue.main.async {
          done(status == .authorized ? nil : ("speech_denied", "Speech recognition is off for Aroh. Turn it on in Settings."))
        }
      }
    }
    if #available(iOS 17.0, *) {
      AVAudioApplication.requestRecordPermission(completionHandler: micDone)
    } else {
      AVAudioSession.sharedInstance().requestRecordPermission(micDone)
    }
  }

  /// Opens the audio and recognizer for claimed session `token`; on any failure the gate is idle again.
  private func begin(_ invoke: Invoke, _ token: Int) {
    guard let recognizer = onDeviceRecognizer() else {
      _ = gate.end(token)
      invoke.reject("On-device speech recognition is not available for this language on this device.", code: "on_device_unavailable")
      return
    }
    guard recognizer.isAvailable else {
      _ = gate.end(token)
      invoke.reject("Speech recognition is not available right now.", code: "recognizer_unavailable")
      return
    }
    do {
      let session = AVAudioSession.sharedInstance()
      try session.setCategory(.record, mode: .measurement, options: .duckOthers)
      try session.setActive(true, options: .notifyOthersOnDeactivation)

      let request = SFSpeechAudioBufferRecognitionRequest()
      request.requiresOnDeviceRecognition = true
      request.shouldReportPartialResults = true

      let input = engine.inputNode
      input.installTap(onBus: 0, bufferSize: 1024, format: input.outputFormat(forBus: 0)) { buffer, _ in
        request.append(buffer)
      }
      heard = Transcript()
      unclaimed = nil
      self.request = request
      task = recognizer.recognitionTask(with: request) { [weak self] result, error in
        DispatchQueue.main.async { self?.handle(result, error, token) }
      }
      engine.prepare()
      try engine.start()
      let ran = gate.run(token)
      assert(ran, "a claimed start runs on the main queue without being superseded")
      invoke.resolve()
    } catch {
      _ = gate.end(token)
      teardown()
      invoke.reject("Could not start the microphone.", code: "audio_failed")
    }
  }

  /// Takes one recognizer callback for session `token`; callbacks of a replaced session are dropped.
  private func handle(_ result: SFSpeechRecognitionResult?, _ error: Error?, _ token: Int) {
    guard gate.isCurrent(token), gate.live else { return }
    if let result = result {
      heard.add(result.bestTranscription.formattedString, utteranceEnded: !result.isFinal && utteranceEnded(result))
      trigger("partial", data: ["text": heard.text])
      if result.isFinal {
        finish(token)
        return
      }
    }
    if error != nil {
      finish(token)
    }
  }

  /// True when `result` carries speech metadata, which the recognizer attaches at the end of an
  /// utterance. Always false before iOS 14.5, where the metadata class does not exist.
  private func utteranceEnded(_ result: SFSpeechRecognitionResult) -> Bool {
    if #available(iOS 14.5, macOS 11.3, *) {
      return result.speechRecognitionMetadata != nil
    }
    return false
  }

  /// Ends session `token` with its words: waiting stops get them, else they are kept for the next
  /// stop and sent as `ended`. A stale or already ended token does nothing.
  private func finish(_ token: Int) {
    guard gate.end(token) else { return }
    let text = heard.text
    let waiting = pendingStops
    teardown()
    if waiting.isEmpty {
      unclaimed = text
      trigger("ended", data: ["text": text])
    } else {
      waiting.forEach { $0.resolve(["transcript": text]) }
    }
  }

  /// Ends a live session a newer start replaced: its waiting stops get its words, nothing else does.
  private func release() {
    let text = heard.text
    let waiting = pendingStops
    teardown()
    waiting.forEach { $0.resolve(["transcript": text]) }
  }

  private func stopAudio() {
    if engine.isRunning {
      engine.stop()
    }
    engine.inputNode.removeTap(onBus: 0)
  }

  /// Frees the audio, the recognizer task and the waiting stops, so nothing outlives a session.
  private func teardown() {
    stopAudio()
    task?.cancel()
    task = nil
    request = nil
    pendingStops = []
    try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
  }
}

/// Entry point the Rust side binds with `ios_plugin_binding!(init_plugin_aroh_speech)`.
@_cdecl("init_plugin_aroh_speech")
public func initPlugin() -> Plugin {
  return SpeechPlugin()
}
