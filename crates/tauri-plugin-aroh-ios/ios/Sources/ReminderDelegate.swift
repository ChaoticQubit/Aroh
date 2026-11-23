import UIKit
import UserNotifications

/// Notification center delegate that replaces the notification plugin's own. The plugin (2.8.1)
/// force-unwraps an in-memory map in `toActiveNotification`, so a reminder scheduled by an
/// earlier app process traps when it fires in the foreground or is tapped
/// (tauri-apps/plugins-workspace#3533). This one never reads that map.
final class ReminderDelegate: NSObject, UNUserNotificationCenterDelegate {
  static let shared = ReminderDelegate()

  /// Makes this object the center's delegate. The center holds its delegate weakly, so the
  /// static `shared` keeps it alive. Call after the notification plugin set its own.
  func install() {
    UNUserNotificationCenter.current().delegate = self
    assert(UNUserNotificationCenter.current().delegate === self)
  }

  /// Removes every pending local notification request of the app, then puts back the place
  /// nudges still waiting out their 120 s, so a reminder reschedule never drops one.
  /// Delivered ones stay. The notification plugin's `cancel_all` cannot do it on iOS (2.8.1),
  /// since it sends no arguments and its Swift side fails to decode them.
  func cancelPending() {
    UNUserNotificationCenter.current().removeAllPendingNotificationRequests()
    LocationBridge.shared.restorePending()
  }

  /// Shows a local reminder as a banner with sound while the app is open. A remote push gets
  /// no foreground banner, as the plugin did, since the server and the local schedule share ids.
  func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    willPresent notification: UNNotification,
    withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
  ) {
    let remote = notification.request.trigger is UNPushNotificationTrigger
    NSLog("aroh-ios: foreground notification %@ remote=%d", notification.request.identifier, remote ? 1 : 0)
    completionHandler(remote ? [] : [.banner, .list, .sound])
  }

  /// A nudge action (Mark done, Remind me in 1 hour, Skip today) goes to the location bridge;
  /// any other tap opens the app and nothing more.
  func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    didReceive response: UNNotificationResponse,
    withCompletionHandler completionHandler: @escaping () -> Void
  ) {
    _ = LocationBridge.shared.respond(response)
    completionHandler()
  }
}
