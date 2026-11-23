import SwiftRs
import Tauri
import UIKit
import WebKit

/// Arguments of `health_authorize`: HealthKit type names to read and, in debug builds, to write.
class AuthorizeArgs: Decodable {
  let read: [String]
  let share: [String]
}

/// Arguments of `health_fetch`: types to query, the stored anchor per type and the earliest
/// sample start (ISO 8601) for a type without an anchor.
class FetchArgs: Decodable {
  let types: [String]
  let anchors: [String: String]
  let since: String
}

/// Arguments of `health_seed`: samples to write, debug builds only.
class SeedArgs: Decodable {
  let samples: [SeedSample]
}

/// Arguments of `location_authorize`: `when_in_use` first, then `always`.
class LocationStepArgs: Decodable {
  let step: String
}

/// Arguments of `location_enable`: start or stop visit monitoring.
class LocationEnableArgs: Decodable {
  let on: Bool
}

/// Arguments of `location_search`: the owner's search text.
class LocationSearchArgs: Decodable {
  let query: String
}

/// Arguments of `location_lookup`: a spot in degrees.
class LocationPointArgs: Decodable {
  let lat: Double
  let lon: Double
}

/// Arguments of `location_snapshot`: a spot in degrees and the image size in points.
class LocationSnapshotArgs: Decodable {
  let lat: Double
  let lon: Double
  let width: Double
  let height: Double
}

/// Native bridges the Aroh Rust commands call: HealthKit import, remote notification
/// registration and cancelling pending reminders. Loading it also installs the reminder delegate. Every command answers on
/// the main queue's completion of its work and rejects with a code the Rust side turns into
/// a status line.
class ArohPlugin: Plugin {
  private let health = HealthBridge()
  private var pinned: NSKeyValueObservation?

  override func load(webview: WKWebView) {
    super.load(webview: webview)
    webview.scrollView.contentInsetAdjustmentBehavior = .never
    webview.scrollView.bounces = false
    webview.scrollView.keyboardDismissMode = .interactive
    webview.scrollView.isScrollEnabled = false
    pinned = webview.scrollView.observe(\.contentOffset, options: [.new]) { view, _ in
      if view.contentOffset != .zero { view.contentOffset = .zero }
    }
    PushBridge.shared.install()
    ReminderDelegate.shared.install()
    LocationBridge.shared.install()
    DevKeyboard.install()
  }

  @objc public func health_authorize(_ invoke: Invoke) {
    guard let args = try? invoke.parseArgs(AuthorizeArgs.self) else {
      invoke.reject("Bad arguments.", code: "bad_args")
      return
    }
    health.authorize(read: args.read, share: args.share) { error in
      if let (code, message) = error {
        invoke.reject(message, code: code)
      } else {
        invoke.resolve()
      }
    }
  }

  @objc public func health_fetch(_ invoke: Invoke) {
    guard let args = try? invoke.parseArgs(FetchArgs.self) else {
      invoke.reject("Bad arguments.", code: "bad_args")
      return
    }
    health.fetch(types: args.types, anchors: args.anchors, since: args.since) { result in
      switch result {
      case .success(let batch): invoke.resolve(batch)
      case .failure(let error): invoke.reject(error.localizedDescription, code: "fetch_failed")
      }
    }
  }

  @objc public func health_seed(_ invoke: Invoke) {
    #if DEBUG
      guard let args = try? invoke.parseArgs(SeedArgs.self) else {
        invoke.reject("Bad arguments.", code: "bad_args")
        return
      }
      health.seed(args.samples) { error in
        if let error = error {
          invoke.reject(error.localizedDescription, code: "seed_failed")
        } else {
          invoke.resolve()
        }
      }
    #else
      invoke.reject("Seeding is for debug builds only.", code: "unsupported")
    #endif
  }

  @objc public func reminders_cancel(_ invoke: Invoke) {
    ReminderDelegate.shared.cancelPending()
    invoke.resolve()
  }

  @objc public func location_authorize(_ invoke: Invoke) {
    guard let args = try? invoke.parseArgs(LocationStepArgs.self) else {
      invoke.reject("Bad arguments.", code: "bad_args")
      return
    }
    LocationBridge.shared.authorize(step: args.step) { status in
      invoke.resolve(["status": status] as [String: String])
    }
  }

  @objc public func location_enable(_ invoke: Invoke) {
    guard let args = try? invoke.parseArgs(LocationEnableArgs.self) else {
      invoke.reject("Bad arguments.", code: "bad_args")
      return
    }
    LocationBridge.shared.setEnabled(args.on)
    invoke.resolve()
  }

  @objc public func location_drain(_ invoke: Invoke) {
    invoke.resolve(LocationBridge.shared.drain())
  }

  @objc public func location_watch(_ invoke: Invoke) {
    guard let plan = try? invoke.parseArgs(LocationPlan.self) else {
      invoke.reject("Bad arguments.", code: "bad_args")
      return
    }
    LocationBridge.shared.watch(plan)
    invoke.resolve()
  }

  @objc public func location_search(_ invoke: Invoke) {
    guard let args = try? invoke.parseArgs(LocationSearchArgs.self) else {
      invoke.reject("Bad arguments.", code: "bad_args")
      return
    }
    LocationBridge.shared.search(args.query) { hits in
      invoke.resolve(["hits": hits])
    }
  }

  @objc public func location_lookup(_ invoke: Invoke) {
    guard let args = try? invoke.parseArgs(LocationPointArgs.self) else {
      invoke.reject("Bad arguments.", code: "bad_args")
      return
    }
    LocationBridge.shared.lookup(lat: args.lat, lon: args.lon) { hit in
      invoke.resolve(hit)
    }
  }

  @objc public func location_snapshot(_ invoke: Invoke) {
    guard let args = try? invoke.parseArgs(LocationSnapshotArgs.self) else {
      invoke.reject("Bad arguments.", code: "bad_args")
      return
    }
    LocationBridge.shared.snapshot(lat: args.lat, lon: args.lon, width: args.width, height: args.height) { url in
      if let url = url {
        invoke.resolve(["url": url] as [String: String])
      } else {
        invoke.reject("No map image.", code: "snapshot_failed")
      }
    }
  }

  @objc public func push_register(_ invoke: Invoke) {
    PushBridge.shared.register { result in
      switch result {
      case .success(let token): invoke.resolve(["token": token] as [String: String])
      case .failure(let error): invoke.reject(error.localizedDescription, code: "register_failed")
      }
    }
  }
}

/// Entry point the Rust side binds with `ios_plugin_binding!(init_plugin_aroh_ios)`.
@_cdecl("init_plugin_aroh_ios")
public func initPlugin() -> Plugin {
  return ArohPlugin()
}
