import ObjectiveC
import UIKit

/// Gets the APNs device token. Tauri's app delegate has no remote notification callbacks, so
/// `install` adds `didRegister...` and `didFailToRegister...` to the delegate's class at
/// runtime, leaving any implementation already there alone; `register` waits for one of
/// them, at most `timeout` seconds.
final class PushBridge {
  static let shared = PushBridge()
  private var waiting: [(Result<String, Error>) -> Void] = []
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

  private func finish(_ result: Result<String, Error>) {
    DispatchQueue.main.async {
      let callbacks = self.waiting
      self.waiting = []
      callbacks.forEach { $0(result) }
    }
  }
}
