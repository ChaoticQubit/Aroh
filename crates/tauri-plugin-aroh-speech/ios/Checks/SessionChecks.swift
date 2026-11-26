import Foundation

/// Runs the `SessionGate` checks: a mic tap always claims the session, stale tokens are ignored,
/// and every path returns the gate to idle. Adds to the shared `passed` and `failed` tallies.
func sessionChecks() {
  func expect(_ name: String, _ ok: Bool) {
    if ok { passed += 1 } else { failed.append(name) }
  }

  var gate = SessionGate()
  let first = gate.claim()
  expect("a claim from idle replaces nothing", first.replaced == .idle && gate.phase == .starting)
  expect("the claimed start runs", gate.run(first.token) && gate.phase == .running)
  let second = gate.claim()
  expect("a tap over a running session replaces it", second.replaced == .running && second.token != first.token)
  expect("the replaced session's token is stale", !gate.isCurrent(first.token) && !gate.end(first.token))
  expect("a stale end leaves the new start alone", gate.phase == .starting)
  expect("the new start runs", gate.run(second.token))

  gate = SessionGate()
  let a = gate.claim()
  let b = gate.claim()
  expect("a tap over a start in flight supersedes it", b.replaced == .starting && !gate.run(a.token))
  expect("the superseding start runs", gate.run(b.token) && gate.phase == .running)

  gate = SessionGate()
  let c = gate.claim()
  _ = gate.run(c.token)
  expect("stop on a running session waits on it", gate.stop() == .wait(c.token, first: true) && gate.phase == .stopping)
  expect("a second stop joins the wait", gate.stop() == .wait(c.token, first: false))
  let d = gate.claim()
  expect("a tap over a stopping session replaces it", d.replaced == .stopping)
  _ = gate.run(d.token)

  gate = SessionGate()
  let e = gate.claim()
  expect("stop during a start cancels it", gate.stop() == .cancelled && gate.phase == .idle)
  expect("the cancelled start does not run", !gate.run(e.token))

  gate = SessionGate()
  expect("stop with nothing running is idle", gate.stop() == .idle)
  let f = gate.claim()
  _ = gate.run(f.token)
  expect("a current end returns to idle", gate.end(f.token) && gate.phase == .idle)
  expect("ending twice is a no-op", !gate.end(f.token))
  let g = gate.claim()
  expect("a failed start returns to idle", gate.end(g.token) && gate.phase == .idle && !gate.live)
  let h = gate.claim()
  _ = gate.run(h.token)
  expect("a running session is live", gate.live)
}
