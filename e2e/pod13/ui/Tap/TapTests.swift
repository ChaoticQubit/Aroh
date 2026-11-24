import XCTest

/// Drives `io.aroh.app` through the system sheets a headless simulator cannot answer otherwise.
/// `testHealthFlow` launches the app with the dev script named in `AROH_TAP_SCRIPT`, turns on
/// every Health type in the HealthKit sheet and taps Allow, then waits for the script's done mark.
/// It touches `AROH_TAP_SHEET` once the sheet is up, so `sim.sh` can take its screenshot.
final class TapTests: XCTestCase {
  private let bundle = "io.aroh.app"

  /// Reads one value from the runner's environment, set by `sim.sh` as `TEST_RUNNER_<name>`.
  private func env(_ name: String) -> String {
    ProcessInfo.processInfo.environment[name] ?? ""
  }

  /// Launches Aroh with the dev script, answers the Health sheet and waits for the done file.
  func testHealthFlow() throws {
    let app = XCUIApplication(bundleIdentifier: bundle)
    app.launchEnvironment["AROH_DEV_SCRIPT"] = env("AROH_TAP_SCRIPT")
    app.launch()
    XCTAssertTrue(app.wait(for: .runningForeground, timeout: 30))
    let turnOn = app.staticTexts["Turn On All"]
    XCTAssertTrue(turnOn.waitForExistence(timeout: 180), "Health sheet did not appear")
    FileManager.default.createFile(atPath: env("AROH_TAP_SHEET"), contents: nil)
    sleep(6)
    turnOn.tap()
    let allow = app.buttons["Allow"]
    XCTAssertTrue(allow.waitForExistence(timeout: 10))
    allow.tap()
    let done = env("AROH_TAP_DONE")
    let deadline = Date().addingTimeInterval(240)
    while !FileManager.default.fileExists(atPath: done) && Date() < deadline {
      sleep(1)
    }
    XCTAssertTrue(FileManager.default.fileExists(atPath: done), "drive did not finish")
  }
}
