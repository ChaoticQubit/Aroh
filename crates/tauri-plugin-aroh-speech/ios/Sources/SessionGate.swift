import Foundation

/// The life of the app's one native speech session, without audio or recognizer types.
/// Every start claims a fresh token and replaces whatever came before, so a tap on a mic is never
/// refused as busy; callbacks and timers carry their token and are dropped once it is stale.
/// Input: `claim`, `run`, `stop`, `end`. Output: `phase`, and the plan each call returns.
struct SessionGate {
  enum Phase: Equatable {
    case idle, starting, running, stopping
  }

  /// What a stop request should do: wait on session `token` (`first` when this request ends the
  /// audio), report a cancelled start, or report that nothing is running.
  enum StopPlan: Equatable {
    case wait(Int, first: Bool)
    case cancelled
    case idle
  }

  private(set) var phase = Phase.idle
  private(set) var token = 0

  /// True while a started session holds the microphone or waits for its final words.
  var live: Bool { phase == .running || phase == .stopping }

  /// True when `token` names the newest claimed session.
  func isCurrent(_ token: Int) -> Bool { token == self.token }

  /// A mic tap: claims a new session in `starting` and returns its token and the phase it
  /// replaced, so the caller ends a live session first.
  mutating func claim() -> (token: Int, replaced: Phase) {
    let replaced = phase
    token += 1
    phase = .starting
    assert(token > 0, "session tokens only grow")
    return (token, replaced)
  }

  /// Marks the claimed start live. False when it was superseded or cancelled meanwhile.
  mutating func run(_ token: Int) -> Bool {
    guard isCurrent(token), phase == .starting else { return false }
    phase = .running
    return true
  }

  /// A stop request: a live session moves to `stopping`, a start in flight is cancelled.
  mutating func stop() -> StopPlan {
    switch phase {
    case .running:
      phase = .stopping
      return .wait(token, first: true)
    case .stopping:
      return .wait(token, first: false)
    case .starting:
      phase = .idle
      return .cancelled
    case .idle:
      return .idle
    }
  }

  /// Session `token` ended (final words, error, interruption, failed start). True when it was the
  /// current session and the gate is idle now; false for a stale or already ended token.
  mutating func end(_ token: Int) -> Bool {
    guard isCurrent(token), phase != .idle else { return false }
    phase = .idle
    assert(!live, "an ended session is not live")
    return true
  }
}
