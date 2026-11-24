import ObjectiveC
import UIKit

/// Debug builds driven by a simulator script (`AROH_DEV_SCRIPT` set) only: lets a scripted
/// `focus()` open the software keyboard, which WKWebView otherwise shows only during a touch,
/// so simulator checks see the real keyboard layout. Release builds and normal launches are untouched.
enum DevKeyboard {
  static func install() {
    #if DEBUG
      guard ProcessInfo.processInfo.environment["AROH_DEV_SCRIPT"] != nil, let view = NSClassFromString("WKContentView") else { return }
      let selector = NSSelectorFromString("_elementDidFocus:userIsInteracting:blurPreviousNode:activityStateChanges:userObject:")
      guard let method = class_getInstanceMethod(view, selector) else { return }
      typealias Focus = @convention(c) (AnyObject, Selector, UnsafeRawPointer, Bool, Bool, UInt, AnyObject?) -> Void
      let original = unsafeBitCast(method_getImplementation(method), to: Focus.self)
      let forced: @convention(block) (AnyObject, UnsafeRawPointer, Bool, Bool, UInt, AnyObject?) -> Void = { me, info, _, blur, changes, object in
        original(me, selector, info, true, blur, changes, object)
      }
      method_setImplementation(method, imp_implementationWithBlock(forced))
    #endif
  }
}
