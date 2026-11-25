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

  /// Shows a local reminder, and a remote run or proposal push (Q83), as a banner with sound
  /// while the app is open. Other remote pushes (reminders, check-ins) get none, since the
  /// server and the local schedule share their ids.
  func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    willPresent notification: UNNotification,
    withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
  ) {
    let remote = notification.request.trigger is UNPushNotificationTrigger
    let shown = !remote || PushTap(notification.request.content.userInfo) != nil
    NSLog("aroh-ios: foreground notification %@ remote=%d shown=%d", notification.request.identifier, remote ? 1 : 0, shown ? 1 : 0)
    completionHandler(shown ? [.banner, .list, .sound] : [])
  }

  /// A nudge action (Mark done, Remind me in 1 hour, Skip today) goes to the location bridge.
  /// A plain tap on a run or proposal push is queued for the app to open its screen (Q83);
  /// any other tap opens the app and nothing more.
  func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    didReceive response: UNNotificationResponse,
    withCompletionHandler completionHandler: @escaping () -> Void
  ) {
    _ = LocationBridge.shared.respond(response)
    if response.actionIdentifier == UNNotificationDefaultActionIdentifier,
      let tap = PushTap(response.notification.request.content.userInfo)
    {
      PushBridge.shared.tapped(tap)
    }
    completionHandler()
  }
}
