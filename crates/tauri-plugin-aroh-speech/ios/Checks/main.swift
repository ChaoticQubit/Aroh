import Foundation

/// One recognizer result fed to a `Transcript`: its `formattedString` and whether it carried
/// utterance-end metadata.
struct Step {
  let text: String
  let ended: Bool
}

/// A partial result without utterance-end metadata.
func say(_ text: String) -> Step { Step(text: text, ended: false) }

/// A result that carries utterance-end metadata.
func pause(_ text: String) -> Step { Step(text: text, ended: true) }

var passed = 0
var failed: [String] = []

/// Feeds `steps` to a fresh `Transcript` and records whether its text equals `expected`.
/// Prints only the check name on failure, never transcript text.
func check(_ name: String, _ steps: [Step], _ expected: String) {
  var transcript = Transcript()
  for step in steps {
    transcript.add(step.text, utteranceEnded: step.ended)
  }
  if transcript.text == expected {
    passed += 1
  } else {
    failed.append(name)
  }
}

check("nothing heard is empty", [], "")
check("only blank results is empty", [say(""), say("  "), pause("")], "")
check("growth within one utterance", [say("I"), say("I read"), say("I read two")], "I read two")
check("surrounding whitespace is trimmed", [say("  I read two ")], "I read two")
check("blank result keeps the current utterance", [say("I read"), say(""), say("   ")], "I read")
check("blank final after a marked utterance keeps it", [say("I read"), pause("I read two"), say("")], "I read two")
check("shorter revision is not a new utterance", [say("I read two chapters"), say("I read two")], "I read two")
check("case and punctuation revision is not a new utterance", [say("i read two"), say("I read two.")], "I read two.")
check("first word revised while short is not a new utterance", [say("Eight"), say("I ate eggs")], "I ate eggs")
check("longer restart after a pause keeps earlier words", [say("I ate eggs"), pause("I ate eggs"), say("Then I walked the dog for an hour")], "I ate eggs Then I walked the dog for an hour")
check("same first word after a pause keeps earlier words", [say("I ate eggs"), pause("I ate eggs"), say("I walked")], "I ate eggs I walked")
check("longer same first word after a pause keeps earlier words", [pause("I ate eggs"), say("I walked the dog for an hour")], "I ate eggs I walked the dog for an hour")
check("three utterances", [pause("I ate eggs"), say("Then"), pause("Then I walked"), say("Later"), say("Later I slept")], "I ate eggs Then I walked Later I slept")
check("growth after the pause signal is not duplicated", [say("I read"), pause("I read"), say("I read two chapters")], "I read two chapters")
check("growth after the signal with new punctuation is not duplicated", [pause("I read."), say("I read, then slept")], "I read, then slept")
check("end signal on a blank result marks the current utterance", [say("I ate eggs"), pause(""), say("I walked")], "I ate eggs I walked")
check("final equal to the last utterance is not duplicated", [pause("I ate eggs"), say("Then slept"), say("Then slept")], "I ate eggs Then slept")
check("cumulative final after a pause is not duplicated", [pause("I ate eggs"), say("I ate eggs then slept")], "I ate eggs then slept")
check("signal on every growing result is not duplicated", [pause("I"), pause("I read"), pause("I read two")], "I read two")
check("longer restart without a signal keeps earlier words", [say("I ate eggs and toast"), say("Then I walked the dog for an hour")], "I ate eggs and toast Then I walked the dog for an hour")
check("shorter restart without a signal keeps earlier words", [say("I ate eggs and toast"), say("Then slept")], "I ate eggs and toast Then slept")
check("same first word restart without a signal keeps earlier words", [say("I ate eggs and toast"), say("I"), say("I walked the dog")], "I ate eggs and toast I walked the dog")
check("losing a trailing word is a revision", [say("I read two chapters"), say("I read two")], "I read two")
check("halving with the same first word is a revision", [say("I read two chapters"), say("I read")], "I read")
check("a result holding the whole session replaces it", [pause("I ate eggs and toast"), say("Then slept"), say("I ate eggs and toast then slept")], "I ate eggs and toast then slept")
check("an unmarked whole-session result replaces it", [say("I ate eggs and toast"), say("Then slept"), say("I ate eggs and toast. Then slept.")], "I ate eggs and toast. Then slept.")

sessionChecks()

if failed.isEmpty {
  print("transcript and session checks passed: \(passed)")
} else {
  print("transcript and session checks passed: \(passed), failed: \(failed.count)")
  failed.forEach { print("failed: \($0)") }
  exit(1)
}
