import Foundation

/// The whole text of one dictation session, built from recognizer results that may cover only
/// the current utterance. The on-device recognizer can restart `formattedString` after a pause, so
/// finished utterances are kept in `committed` and the live one in `current`.
/// Input: each result through `add`. Output: `text`, every utterance joined with one space.
struct Transcript {
  private(set) var committed: [String] = []
  private(set) var current = ""
  private var ended = false

  /// The session text so far: committed utterances, then the current one.
  var text: String {
    (committed + [current]).filter { !$0.isEmpty }.joined(separator: " ")
  }

  /// Takes one result. `formatted` is its `formattedString`; `utteranceEnded` is true when it
  /// carried utterance-end metadata. A blank result never wipes text, and a result that begins with
  /// the whole session so far replaces it instead of repeating it. After an end signal, the
  /// next result that does not continue the ended utterance starts a new one; without a signal,
  /// a result under half the old word count, or one whose first word changed once the old one is
  /// long enough to tell a restart from a revision, starts a new one. When unsure the earlier words
  /// are kept, so loss becomes duplication.
  mutating func add(_ formatted: String, utteranceEnded: Bool) {
    let text = formatted.trimmingCharacters(in: .whitespacesAndNewlines)
    if !text.isEmpty {
      if !committed.isEmpty && Transcript.words(text).starts(with: Transcript.words(self.text)) {
        committed = []
      } else if !current.isEmpty && startsUtterance(text) {
        committed.append(current)
      }
      current = text
      ended = false
    }
    if utteranceEnded && !current.isEmpty {
      ended = true
    }
    assert(committed.allSatisfy { !$0.isEmpty }, "a committed utterance is blank")
    assert(committed.isEmpty || !current.isEmpty, "committed words without a current utterance")
    assert(!ended || !current.isEmpty, "an end mark on no words")
  }

  /// True when non-blank `next` begins a new utterance instead of continuing or revising `current`.
  private func startsUtterance(_ next: String) -> Bool {
    let old = Transcript.words(current)
    let new = Transcript.words(next)
    if ended {
      return !new.starts(with: old)
    }
    if !new.isEmpty && new.count * 2 < old.count {
      return true
    }
    guard let first = old.first, new.first != first else { return false }
    return new.count < old.count || old.count >= 3
  }

  /// Lowercased words of `text` with punctuation dropped, so case and punctuation edits compare equal.
  private static func words(_ text: String) -> [String] {
    text.lowercased().components(separatedBy: CharacterSet.alphanumerics.inverted).filter { !$0.isEmpty }
  }
}
