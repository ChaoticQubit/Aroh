import SwiftUI

/// Empty host app the UI test bundle needs. The tests drive `io.aroh.app`, not this app.
@main
struct HostApp: App {
  var body: some Scene {
    WindowGroup { Text("Aroh tap host") }
  }
}
