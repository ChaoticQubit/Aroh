import Foundation
import HealthKit
import UIKit

/// One discrete sample as the Rust import reads it: body mass `value` in kg, or a workout
/// with `energy_kcal` and `activity`. Times are ISO 8601 in UTC.
struct SampleOut: Encodable {
  let uuid: String
  let start: String
  let end: String
  let value: Double?
  let energy_kcal: Double?
  let activity: String?
}

/// The cumulative total of one local day: `date` (yyyy-MM-dd), its start instant and the sum.
struct DayOut: Encodable {
  let date: String
  let start: String
  let total: Double
}

/// What one anchored query of a type returned, plus recomputed day totals for cumulative types.
struct TypeOut: Encodable {
  let anchor: String?
  let samples: [SampleOut]
  let deleted: [String]
  let days: [DayOut]
}

/// A sample `health_seed` writes in debug builds: a type name, a value in kg, steps or kcal
/// (or workout kcal), its times, and a workout activity name.
struct SeedSample: Decodable {
  let type: String
  let value: Double
  let start: String
  let end: String
  let activity: String?
}

/// HealthKit access for Aroh: authorization, anchored incremental queries and day totals.
/// Type names are `body_mass`, `workout`, `steps` and `dietary_energy`.
final class HealthBridge {
  private let store = HKHealthStore()
  private let iso = ISO8601DateFormatter()

  private func sampleType(_ name: String) -> HKSampleType? {
    switch name {
    case "body_mass": return HKQuantityType(.bodyMass)
    case "workout": return HKObjectType.workoutType()
    case "steps": return HKQuantityType(.stepCount)
    case "dietary_energy": return HKQuantityType(.dietaryEnergyConsumed)
    default: return nil
    }
  }

  /// Shows the HealthKit sheet for `read` (plus active energy for workouts) and `share`.
  /// Calls `done(nil)` once the owner answered, or `done((code, message))` when Health is
  /// not available. HealthKit never reveals whether reading was allowed.
  func authorize(read: [String], share: [String], done: @escaping ((String, String)?) -> Void) {
    guard HKHealthStore.isHealthDataAvailable() else {
      done(("unavailable", "Apple Health is not available on this device."))
      return
    }
    var readTypes = Set<HKObjectType>(read.compactMap(sampleType))
    if read.contains("workout") {
      readTypes.insert(HKQuantityType(.activeEnergyBurned))
    }
    let shareTypes = Set<HKSampleType>(share.compactMap(sampleType))
    DispatchQueue.main.async {
      self.makeWindowKey()
      self.store.requestAuthorization(toShare: shareTypes, read: readTypes) { ok, error in
        DispatchQueue.main.async {
          if let error = error {
            done(("denied", error.localizedDescription))
          } else {
            done(ok ? nil : ("denied", "Apple Health access was not granted."))
          }
        }
      }
    }
  }

  /// HealthKit presents its sheet on the key window. The Tauri window is not always key
  /// (it was not on a fresh simulator launch), so the sheet would load but never show.
  /// Makes the first window of the foreground scene key when no window is.
  private func makeWindowKey() {
    let windows = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.flatMap { $0.windows }
    if !windows.contains(where: { $0.isKeyWindow }) {
      windows.first?.makeKey()
    }
  }

  /// Runs one anchored query per type in `types`, from its stored anchor or from `since`,
  /// and for steps and dietary energy recomputes the totals of every day with new samples.
  /// Calls `done` once with every type's result, keyed by type name.
  func fetch(types: [String], anchors: [String: String], since: String, done: @escaping (Result<[String: TypeOut], Error>) -> Void) {
    let start = iso.date(from: since) ?? Date().addingTimeInterval(-30 * 86400)
    let group = DispatchGroup()
    let lock = NSLock()
    var out: [String: TypeOut] = [:]
    var failure: Error?
    for name in types {
      guard let type = sampleType(name) else { continue }
      group.enter()
      let anchor = anchors[name].flatMap { Data(base64Encoded: $0) }.flatMap {
        try? NSKeyedUnarchiver.unarchivedObject(ofClass: HKQueryAnchor.self, from: $0)
      }
      let predicate = anchor == nil ? HKQuery.predicateForSamples(withStart: start, end: nil) : nil
      let query = HKAnchoredObjectQuery(type: type, predicate: predicate, anchor: anchor, limit: HKObjectQueryNoLimit) { _, added, deleted, next, error in
        if let error = error {
          lock.lock()
          failure = error
          lock.unlock()
          group.leave()
          return
        }
        let anchorText = next.flatMap { try? NSKeyedArchiver.archivedData(withRootObject: $0, requiringSecureCoding: true) }?.base64EncodedString()
        let gone = (deleted ?? []).map { $0.uuid.uuidString }
        let samples = added ?? []
        let finish: ([SampleOut], [DayOut]) -> Void = { outSamples, days in
          lock.lock()
          out[name] = TypeOut(anchor: anchorText, samples: outSamples, deleted: gone, days: days)
          lock.unlock()
          group.leave()
        }
        switch name {
        case "steps", "dietary_energy":
          self.dayTotals(type as! HKQuantityType, unit: name == "steps" ? .count() : .kilocalorie(), samples: samples) { finish([], $0) }
        default:
          finish(samples.compactMap(self.sampleOut), [])
        }
      }
      store.execute(query)
    }
    group.notify(queue: .main) {
      if let failure = failure {
        done(.failure(failure))
      } else {
        done(.success(out))
      }
    }
  }

  private func sampleOut(_ sample: HKSample) -> SampleOut? {
    let uuid = sample.uuid.uuidString
    let start = iso.string(from: sample.startDate)
    let end = iso.string(from: sample.endDate)
    if let workout = sample as? HKWorkout {
      var active: HKQuantity?
      if #available(iOS 16.0, *) {
        active = workout.statistics(for: HKQuantityType(.activeEnergyBurned))?.sumQuantity()
      }
      let energy = (active ?? workout.totalEnergyBurned)?.doubleValue(for: .kilocalorie())
      return SampleOut(uuid: uuid, start: start, end: end, value: nil, energy_kcal: energy, activity: activityName(workout.workoutActivityType))
    }
    guard let quantity = sample as? HKQuantitySample else { return nil }
    return SampleOut(uuid: uuid, start: start, end: end, value: quantity.quantity.doubleValue(for: .gramUnit(with: .kilo)), energy_kcal: nil, activity: nil)
  }

  private func dayTotals(_ type: HKQuantityType, unit: HKUnit, samples: [HKSample], done: @escaping ([DayOut]) -> Void) {
    let calendar = Calendar.current
    let days = Set(samples.map { calendar.startOfDay(for: $0.startDate) }).sorted()
    let formatter = DateFormatter()
    formatter.calendar = calendar
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.dateFormat = "yyyy-MM-dd"
    let group = DispatchGroup()
    let lock = NSLock()
    var out: [DayOut] = []
    for day in days {
      guard let next = calendar.date(byAdding: .day, value: 1, to: day) else { continue }
      group.enter()
      let predicate = HKQuery.predicateForSamples(withStart: day, end: next, options: .strictStartDate)
      let query = HKStatisticsQuery(quantityType: type, quantitySamplePredicate: predicate, options: .cumulativeSum) { _, stats, _ in
        let total = stats?.sumQuantity()?.doubleValue(for: unit) ?? 0
        lock.lock()
        out.append(DayOut(date: formatter.string(from: day), start: self.iso.string(from: day), total: total))
        lock.unlock()
        group.leave()
      }
      store.execute(query)
    }
    group.notify(queue: .global()) { done(out.sorted { $0.date < $1.date }) }
  }

  private func activityName(_ type: HKWorkoutActivityType) -> String {
    switch type {
    case .running: return "Running"
    case .walking: return "Walking"
    case .cycling: return "Cycling"
    case .swimming: return "Swimming"
    case .hiking: return "Hiking"
    case .yoga: return "Yoga"
    case .traditionalStrengthTraining, .functionalStrengthTraining: return "Strength training"
    case .highIntensityIntervalTraining: return "HIIT"
    case .coreTraining: return "Core training"
    case .rowing: return "Rowing"
    case .elliptical: return "Elliptical"
    case .dance: return "Dance"
    default: return "Workout"
    }
  }

  #if DEBUG
    /// Writes `samples` to HealthKit so simulator checks have data to import.
    /// Needs share access, which only debug builds request. Calls `done` with the error, if any.
    func seed(_ samples: [SeedSample], done: @escaping (Error?) -> Void) {
      let objects: [HKObject] = samples.compactMap { s in
        guard let start = iso.date(from: s.start), let end = iso.date(from: s.end) else { return nil }
        switch s.type {
        case "body_mass":
          return HKQuantitySample(type: HKQuantityType(.bodyMass), quantity: HKQuantity(unit: .gramUnit(with: .kilo), doubleValue: s.value), start: start, end: end)
        case "steps":
          return HKQuantitySample(type: HKQuantityType(.stepCount), quantity: HKQuantity(unit: .count(), doubleValue: s.value), start: start, end: end)
        case "dietary_energy":
          return HKQuantitySample(type: HKQuantityType(.dietaryEnergyConsumed), quantity: HKQuantity(unit: .kilocalorie(), doubleValue: s.value), start: start, end: end)
        case "workout":
          let activity: HKWorkoutActivityType = s.activity == "Running" ? .running : .traditionalStrengthTraining
          return HKWorkout(activityType: activity, start: start, end: end, duration: end.timeIntervalSince(start), totalEnergyBurned: HKQuantity(unit: .kilocalorie(), doubleValue: s.value), totalDistance: nil, metadata: nil)
        default:
          return nil
        }
      }
      store.save(objects) { _, error in
        DispatchQueue.main.async { done(error) }
      }
    }
  #endif
}
