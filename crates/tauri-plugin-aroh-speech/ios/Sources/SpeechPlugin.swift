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
class SpeechPlugin: Plugin {
  private let engine = AVAudioEngine()
  private let finalWait: TimeInterval = 2.0
  private var request: SFSpeechAudioBufferRecognitionRequest?
  private var task: SFSpeechRecognitionTask?
  private var heard = Transcript()
  private var unclaimed: String?
  private var pendingStop: Invoke?
  private var starting = false
  private var generation = 0

  /// Lets the web layout fill the whole screen. Without this the web view's scroll view
  /// shrinks the page by the safe areas while CSS `env(safe-area-inset-*)` still reports
  /// them, so the insets count twice and a blank band shows below the tab bar.
  override func load(webview: WKWebView) {
    super.load(webview: webview)
    webview.scrollView.contentInsetAdjustmentBehavior = .never
  }

  @objc public func start(_ invoke: Invoke) {
    DispatchQueue.main.async {
      guard self.task == nil, !self.starting, self.pendingStop == nil else {
        invoke.reject("Already listening.", code: "busy")
        return
      }
      self.starting = true
      self.authorize { error in
        if let (code, message) = error {
          self.starting = false
          invoke.reject(message, code: code)
          return
        }
        self.begin(invoke)
      }
    }
  }

  @objc public func stop(_ invoke: Invoke) {
    DispatchQueue.main.async {
      guard self.pendingStop == nil else {
        invoke.reject("Already stopping.", code: "busy")
        return
      }
      guard self.task != nil else {
        if let text = self.unclaimed {
          self.unclaimed = nil
          invoke.resolve(["transcript": text])
        } else {
          invoke.reject("Voice input is not running.", code: "not_started")
        }
        return
      }
      self.pendingStop = invoke
      self.stopAudio()
      self.request?.endAudio()
      let generation = self.generation
      DispatchQueue.main.asyncAfter(deadline: .now() + self.finalWait) { [weak self] in
        guard let self = self, self.generation == generation else { return }
        self.finish()
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

  private func begin(_ invoke: Invoke) {
    defer { starting = false }
    guard let recognizer = onDeviceRecognizer() else {
      invoke.reject("On-device speech recognition is not available for this language on this device.", code: "on_device_unavailable")
      return
    }
    guard recognizer.isAvailable else {
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
      generation += 1
      heard = Transcript()
      unclaimed = nil
      self.request = request
      task = recognizer.recognitionTask(with: request) { [weak self] result, error in
        DispatchQueue.main.async { self?.handle(result, error) }
      }
      engine.prepare()
      try engine.start()
      invoke.resolve()
    } catch {
      teardown()
      invoke.reject("Could not start the microphone: \(error.localizedDescription)", code: "audio_failed")
    }
  }

  private func handle(_ result: SFSpeechRecognitionResult?, _ error: Error?) {
    guard task != nil else { return }
    if let result = result {
      heard.add(result.bestTranscription.formattedString, utteranceEnded: !result.isFinal && utteranceEnded(result))
      trigger("partial", data: ["text": heard.text])
      if result.isFinal {
        finish()
        return
      }
    }
    if error != nil {
      finish()
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

  private func finish() {
    guard task != nil else { return }
    let text = heard.text
    teardown()
    if let invoke = pendingStop {
      pendingStop = nil
      invoke.resolve(["transcript": text])
    } else {
      unclaimed = text
      trigger("ended", data: ["text": text])
    }
  }

  private func stopAudio() {
    if engine.isRunning {
      engine.stop()
    }
    engine.inputNode.removeTap(onBus: 0)
  }

  private func teardown() {
    stopAudio()
    task?.cancel()
    task = nil
    request = nil
    try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
  }
}

/// Entry point the Rust side binds with `ios_plugin_binding!(init_plugin_aroh_speech)`.
@_cdecl("init_plugin_aroh_speech")
public func initPlugin() -> Plugin {
  return SpeechPlugin()
}
