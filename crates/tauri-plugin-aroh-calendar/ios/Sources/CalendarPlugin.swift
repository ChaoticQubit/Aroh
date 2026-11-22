import EventKit
import SwiftRs
import Tauri
import UIKit
import WebKit

/// Arguments of `events`: the window to read, as ISO 8601 instants.
class EventsArgs: Decodable {
  let from: String
  let to: String
}

/// One test event for `seed`: title, ISO 8601 start and end, and an optional
/// `FREQ=DAILY` or `FREQ=WEEKLY;BYDAY=..` recurrence.
class SeedEvent: Decodable {
  let title: String
  let start_at: String
  let end_at: String
  let rrule: String?
}

/// Arguments of `seed`: the test events to write.
class SeedArgs: Decodable {
  let events: [SeedEvent]
}

/// Read-only access to the device calendars for Aroh's calendar triage.
/// `events` asks for full access once, then returns every occurrence in the window with its
/// calendar, ids, title, times, all-day flag and recurrence. Nothing is ever written back;
/// `seed` writes test events only in simulator debug builds.
class CalendarPlugin: Plugin {
  private let store = EKEventStore()
  private let maxEvents = 5000
  private let iso = ISO8601DateFormatter()
  private let isoFraction: ISO8601DateFormatter = {
    let f = ISO8601DateFormatter()
    f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return f
  }()

  /// Parses an ISO 8601 instant with or without fractional seconds, as JavaScript
  /// `toISOString` sends them. Input: the text. Output: the date, or nil when it is not one.
  private func instant(_ text: String) -> Date? {
    iso.date(from: text) ?? isoFraction.date(from: text)
  }

  @objc public func events(_ invoke: Invoke) {
    guard let args = try? invoke.parseArgs(EventsArgs.self),
      let from = instant(args.from), let to = instant(args.to), from < to
    else {
      invoke.reject("The calendar window is not valid.", code: "bad_args")
      return
    }
    authorize { granted in
      guard granted else {
        invoke.reject("Calendar access is off for Aroh. Turn it on in Settings, Privacy, Calendars.", code: "calendar_denied")
        return
      }
      let predicate = self.store.predicateForEvents(withStart: from, end: to, calendars: nil)
      let found = self.store.events(matching: predicate).prefix(self.maxEvents)
      invoke.resolve(["events": found.map { self.row($0) }])
    }
  }

  @objc public func seed(_ invoke: Invoke) {
    #if DEBUG && targetEnvironment(simulator)
      guard let args = try? invoke.parseArgs(SeedArgs.self) else {
        invoke.reject("The seed events are not valid.", code: "bad_args")
        return
      }
      authorize { granted in
        guard granted, let calendar = self.store.defaultCalendarForNewEvents else {
          invoke.reject("Calendar access is off for Aroh.", code: "calendar_denied")
          return
        }
        do {
          let titles = Set(args.events.map { $0.title })
          let window = self.store.predicateForEvents(withStart: Date().addingTimeInterval(-7 * 86400), end: Date().addingTimeInterval(60 * 86400), calendars: [calendar])
          for old in self.store.events(matching: window) where titles.contains(old.title ?? "") {
            try self.store.remove(old, span: .futureEvents, commit: false)
          }
          for e in args.events {
            guard let start = self.instant(e.start_at), let end = self.instant(e.end_at) else { continue }
            let event = EKEvent(eventStore: self.store)
            event.calendar = calendar
            event.title = e.title
            event.startDate = start
            event.endDate = end
            if let rule = self.recurrence(e.rrule) {
              event.addRecurrenceRule(rule)
            }
            try self.store.save(event, span: .futureEvents, commit: false)
          }
          try self.store.commit()
          invoke.resolve(["seeded": args.events.count])
        } catch {
          invoke.reject("Could not seed: \(error.localizedDescription)", code: "seed_failed")
        }
      }
    #else
      invoke.reject("Seeding is only for simulator debug builds.", code: "unsupported")
    #endif
  }

  /// Asks for full calendar access when it is not decided yet and calls `done` on the main
  /// queue with whether events can be read. iOS 17 needs full access to read; older
  /// systems use the event entity request.
  private func authorize(_ done: @escaping (Bool) -> Void) {
    let finish: (Bool, Error?) -> Void = { granted, _ in DispatchQueue.main.async { done(granted) } }
    let status = EKEventStore.authorizationStatus(for: .event)
    if #available(iOS 17.0, *) {
      if status == .fullAccess { return done(true) }
      if status != .notDetermined { return done(false) }
      store.requestFullAccessToEvents(completion: finish)
    } else {
      if status == .authorized { return done(true) }
      if status != .notDetermined { return done(false) }
      store.requestAccess(to: .event, completion: finish)
    }
  }

  /// One occurrence as the app's `DeviceEvent`: ids, calendar name, title, ISO 8601 times,
  /// all-day flag and the RFC 5545 recurrence of its series, or null.
  private func row(_ e: EKEvent) -> [String: Any] {
    return [
      "calendar_id": e.calendar?.calendarIdentifier ?? "unknown",
      "calendar": String((e.calendar?.title ?? "").prefix(100)),
      "event_id": e.eventIdentifier ?? e.calendarItemIdentifier,
      "title": String((e.title ?? "").prefix(500)),
      "start_at": iso.string(from: e.startDate),
      "end_at": iso.string(from: max(e.endDate, e.startDate.addingTimeInterval(60))),
      "all_day": e.isAllDay,
      "rrule": (e.recurrenceRules?.first).map { rruleText($0) } ?? NSNull(),
    ]
  }

  /// RFC 5545 text for an EventKit rule: FREQ, INTERVAL above 1 and BYDAY.
  private func rruleText(_ r: EKRecurrenceRule) -> String {
    let freq: String
    switch r.frequency {
    case .daily: freq = "DAILY"
    case .weekly: freq = "WEEKLY"
    case .monthly: freq = "MONTHLY"
    case .yearly: freq = "YEARLY"
    @unknown default: freq = "DAILY"
    }
    var parts = ["FREQ=\(freq)"]
    if r.interval > 1 { parts.append("INTERVAL=\(r.interval)") }
    let names = ["SU", "MO", "TU", "WE", "TH", "FR", "SA"]
    if let days = r.daysOfTheWeek, !days.isEmpty {
      parts.append("BYDAY=" + days.map { names[$0.dayOfTheWeek.rawValue - 1] }.joined(separator: ","))
    }
    return parts.joined(separator: ";")
  }

  /// The EventKit rule for a seed recurrence: `FREQ=DAILY`, or `FREQ=WEEKLY` with BYDAY.
  /// Returns nil for none or anything else.
  private func recurrence(_ text: String?) -> EKRecurrenceRule? {
    guard let text = text else { return nil }
    if text == "FREQ=DAILY" { return EKRecurrenceRule(recurrenceWith: .daily, interval: 1, end: nil) }
    guard text.hasPrefix("FREQ=WEEKLY;BYDAY="), let list = text.split(separator: "=").last else { return nil }
    let names = ["SU", "MO", "TU", "WE", "TH", "FR", "SA"]
    let days = list.split(separator: ",").compactMap { d in names.firstIndex(of: String(d)).flatMap { EKWeekday(rawValue: $0 + 1) }.map { EKRecurrenceDayOfWeek($0) } }
    return EKRecurrenceRule(recurrenceWith: .weekly, interval: 1, daysOfTheWeek: days, daysOfTheMonth: nil, monthsOfTheYear: nil, weeksOfTheYear: nil, daysOfTheYear: nil, setPositions: nil, end: nil)
  }
}

/// Entry point the Rust side binds with `ios_plugin_binding!(init_plugin_aroh_calendar)`.
@_cdecl("init_plugin_aroh_calendar")
public func initPlugin() -> Plugin {
  return CalendarPlugin()
}
