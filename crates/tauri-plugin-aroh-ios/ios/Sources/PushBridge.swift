import ObjectiveC
import UIKit

/// A tapped run or proposal push (Q83): the `aroh` dictionary of its payload with `kind`
/// `run` or `proposal`, the row `id` and the `goal_id`, absent for a run without a goal.
/// Only short strings pass; the app checks the ids again before it opens a screen.
struct PushTap: Encodable, Equatable {
  let kind: String
  let id: String
  let goal_id: String?

  /// Reads the tap from a notification's `userInfo`; nil for any other push or shape.
  init?(_ userInfo: [AnyHashable: Any]) {
    guard let aroh = userInfo["aroh"] as? [String: Any],
      let kind = aroh["kind"] as? String, kind == "run" || kind == "proposal",
      let id = aroh["id"] as? String, !id.isEmpty, id.count <= 64
    else { return nil }
    var goal: String? = nil
    if let raw = aroh["goal_id"], !(raw is NSNull) {
      guard let text = raw as? String, !text.isEmpty, text.count <= 64 else { return nil }
      goal = text
    }
    self.kind = kind
    self.id = id
    self.goal_id = goal
  }
}

/// Gets the APNs device token. Tauri's app delegate has no remote notification callbacks, so
/// `install` adds `didRegister...` and `didFailToRegister...` to the delegate's class at
/// runtime, leaving any implementation already there alone; `register` waits for one of
/// them, at most `timeout` seconds.
final class PushBridge {
  static let shared = PushBridge()
  private var waiting: [(Result<String, Error>) -> Void] = []
  private var taps: [PushTap] = []
  private var tapWaiters: [(PushTap) -> Void] = []
  private let tapsMax = 8
  private var installed = false
  private let timeout = 20.0

  /// Adds the two delegate methods once. Safe to call again.
  func install() {
    guard !installed, let delegate = UIApplication.shared.delegate, let cls = object_getClass(delegate) else { return }
    installed = true
    let registered: @convention(block) (AnyObject, UIApplication, Data) -> Void = { _, _, data in
      PushBridge.shared.finish(.success(data.map { String(format: "%02x", $0) }.joined()))
    }
    let failed: @convention(block) (AnyObject, UIApplication, Error) -> Void = { _, _, error in
      PushBridge.shared.finish(.failure(error))
    }
    add(cls, #selector(UIApplicationDelegate.application(_:didRegisterForRemoteNotificationsWithDeviceToken:)), imp_implementationWithBlock(registered))
    add(cls, #selector(UIApplicationDelegate.application(_:didFailToRegisterForRemoteNotificationsWithError:)), imp_implementationWithBlock(failed))
  }

  private func add(_ cls: AnyClass, _ selector: Selector, _ imp: IMP) {
    if !class_addMethod(cls, selector, imp, "v@:@@") {
      NSLog("aroh-ios: app delegate already implements %@; push token callbacks not added", NSStringFromSelector(selector))
    }
  }

  /// Asks iOS for the device token and calls `done` with the lowercase hex token, the error,
  /// or a timeout error when iOS answers neither way within `timeout` seconds.
  func register(_ done: @escaping (Result<String, Error>) -> Void) {
    DispatchQueue.main.async {
      self.waiting.append(done)
      UIApplication.shared.registerForRemoteNotifications()
      DispatchQueue.main.asyncAfter(deadline: .now() + self.timeout) {
        guard !self.waiting.isEmpty else { return }
        let error = NSError(domain: "aroh", code: 1, userInfo: [NSLocalizedDescriptionKey: "no push token from iOS"])
        self.finish(.failure(error))
      }
    }
  }

  /// Hands `tap` to the oldest waiting `nextTap` caller, or queues it (at most `tapsMax`,
  /// oldest dropped) until one asks, so a tap that opens the app cold is not lost.
  func tapped(_ tap: PushTap) {
    DispatchQueue.main.async {
      if self.tapWaiters.isEmpty {
        self.taps.append(tap)
        if self.taps.count > self.tapsMax { self.taps.removeFirst() }
      } else {
        self.tapWaiters.removeFirst()(tap)
      }
    }
  }

  /// Calls `done` with the oldest queued tap, or with the next tap once one arrives.
  func nextTap(_ done: @escaping (PushTap) -> Void) {
    DispatchQueue.main.async {
      if self.taps.isEmpty {
        self.tapWaiters.append(done)
      } else {
        done(self.taps.removeFirst())
      }
    }
  }

  private func finish(_ result: Result<String, Error>) {
    DispatchQueue.main.async {
      let callbacks = self.waiting
      self.waiting = []
      callbacks.forEach { $0(result) }
    }
  }
}
