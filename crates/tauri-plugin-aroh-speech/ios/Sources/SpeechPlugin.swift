import AVFoundation
import Speech
import SwiftRs
import Tauri
import UIKit
import WebKit

/// On-device speech to text for Aroh's log intake.
/// `start` asks for speech and microphone permission, then streams microphone audio into
/// `SFSpeechRecognizer` with `requiresOnDeviceRecognition`, emitting `partial` events `{text}`.
/// `stop` ends the audio and resolves `{transcript}` with the final text.
class SpeechPlugin: Plugin {
  private let engine = AVAudioEngine()
  private let finalWait: TimeInterval = 2.0
  private var request: SFSpeechAudioBufferRecognitionRequest?
  private var task: SFSpeechRecognitionTask?
  private var transcript = ""
  private var unclaimed: String?
  private var pendingStop: Invoke?
  private var starting = false
  private var generation = 0

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

  private func authorize(_ done: @escaping ((String, String)?) -> Void) {
    SFSpeechRecognizer.requestAuthorization { status in
      guard status == .authorized else {
        DispatchQueue.main.async {
          done(("permission_denied", "Speech recognition is off for Aroh. Turn it on in Settings."))
        }
        return
      }
      let micDone: (Bool) -> Void = { granted in
        DispatchQueue.main.async {
          done(granted ? nil : ("permission_denied", "Microphone access is off for Aroh. Turn it on in Settings."))
        }
      }
      if #available(iOS 17.0, *) {
        AVAudioApplication.requestRecordPermission(completionHandler: micDone)
      } else {
        AVAudioSession.sharedInstance().requestRecordPermission(micDone)
      }
    }
  }

  private func begin(_ invoke: Invoke) {
    defer { starting = false }
    guard let recognizer = SFSpeechRecognizer(locale: Locale.current) ?? SFSpeechRecognizer(locale: Locale(identifier: "en-US")),
      recognizer.supportsOnDeviceRecognition
    else {
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
      transcript = ""
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
      transcript = result.bestTranscription.formattedString
      trigger("partial", data: ["text": transcript])
      if result.isFinal {
        finish()
        return
      }
    }
    if error != nil {
      finish()
    }
  }

  private func finish() {
    guard task != nil else { return }
    let text = transcript
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
