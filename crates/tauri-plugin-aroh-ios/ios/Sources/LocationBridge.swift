import CoreLocation
import MapKit
import UIKit
import UserNotifications

/// A visit as the Rust drain reads it: center in degrees, accuracy in meters, ISO 8601 times.
struct VisitOut: Codable {
  let lat: Double
  let lon: Double
  let accuracy_m: Double
  let arrived_at: String
  let left_at: String?
}

/// A nudge iOS showed, by place id and local day `yyyy-MM-dd`.
struct NudgedOut: Codable, Equatable {
  let place_id: String
  let date: String
}

/// The owner's answer to a nudge: `done` or `skip` for action `action_id`.
struct ResponseOut: Codable {
  let action_id: String
  let choice: String
}

/// Everything sensed since the last drain, kept in a file until Rust drains it.
struct LocationBuffer: Codable {
  var visits: [VisitOut] = []
  var nudged: [NudgedOut] = []
  var responses: [ResponseOut] = []
}

/// A place to monitor, center in degrees.
struct WatchedPlace: Codable {
  let id: String
  let name: String
  let lat: Double
  let lon: Double
  let radius_m: Int
}

/// One planned nudge from Rust: shown 120 s after arriving at `place_id` on local day `date`
/// when that instant falls in `[from, until)`.
struct PlannedNudge: Codable {
  let id: String
  let place_id: String
  let date: String
  let action_id: String
  let title: String
  let body: String
  let from: String
  let until: String
}

/// The watched set and nudge plan Rust last handed over.
struct LocationPlan: Codable {
  var places: [WatchedPlace] = []
  var nudges: [PlannedNudge] = []
}

/// A nudge scheduled on arrival: its content and when it shows, so a reminder reschedule
/// can put it back and a drain can report it once shown.
struct ScheduledNudge: Codable {
  let nudge: PlannedNudge
  let fire: Double
}

/// A map search result or reverse lookup.
struct PlaceHit: Encodable {
  let name: String?
  let kind: String?
  let address: String?
  let lat: Double
  let lon: Double
}

/// Location awareness on the iPhone (Q56): asks for When In Use then Always, records visits,
/// monitors at most 20 watched places (CLMonitor on iOS 17, region monitoring before), shows
/// a nudge after 120 s near a place, answers nudge actions, and runs MapKit search, reverse
/// lookup and snapshots. Raw positions stay in its files until the Rust drain stores them
/// locally; they are never logged.
final class LocationBridge: NSObject, CLLocationManagerDelegate {
  static let shared = LocationBridge()
  static let category = "aroh.nudge"
  static let prefix = "aroh.nudge."
  static let later = "aroh.nudge.later."
  static let dwell: TimeInterval = 120
  static let maxWatched = 20

  private let manager = CLLocationManager()
  private let queue = DispatchQueue(label: "aroh.location")
  private var waiting: [(String) -> Void] = []
  private var installed = false
  private var monitorBox: Any?

  private var folder: URL {
    let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
    let dir = base.appendingPathComponent("aroh-location", isDirectory: true)
    try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    return dir
  }

  private func load<T: Decodable>(_ name: String, _ fallback: T) -> T {
    guard let data = try? Data(contentsOf: folder.appendingPathComponent(name)) else { return fallback }
    return (try? JSONDecoder().decode(T.self, from: data)) ?? fallback
  }

  private func save<T: Encodable>(_ name: String, _ value: T) {
    guard let data = try? JSONEncoder().encode(value) else { return }
    try? data.write(to: folder.appendingPathComponent(name), options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
  }

  private static func iso(_ date: Date) -> String {
    ISO8601DateFormatter().string(from: date)
  }

  private static func parse(_ text: String) -> Date? {
    let f = ISO8601DateFormatter()
    if let d = f.date(from: text) { return d }
    f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return f.date(from: text)
  }

  private static func localDay(_ date: Date) -> String {
    let f = DateFormatter()
    f.calendar = Calendar(identifier: .gregorian)
    f.locale = Locale(identifier: "en_US_POSIX")
    f.timeZone = TimeZone.current
    f.dateFormat = "yyyy-MM-dd"
    return f.string(from: date)
  }

  /// Sets the manager delegate, registers the nudge category and, when location is on,
  /// resumes visits and monitoring, so a relaunch for a location event is handled. Call once
  /// at plugin load; safe to call again.
  func install() {
    guard !installed else { return }
    installed = true
    manager.delegate = self
    let center = UNUserNotificationCenter.current()
    center.getNotificationCategories { existing in
      let done = UNNotificationAction(identifier: "done", title: "Mark done", options: [])
      let later = UNNotificationAction(identifier: "later", title: "Remind me in 1 hour", options: [])
      let skip = UNNotificationAction(identifier: "skip", title: "Skip today", options: [.destructive])
      let nudge = UNNotificationCategory(identifier: LocationBridge.category, actions: [done, later, skip], intentIdentifiers: [], options: [])
      center.setNotificationCategories(existing.filter { $0.identifier != LocationBridge.category }.union([nudge]))
    }
    if manager.authorizationStatus == .authorizedAlways || manager.authorizationStatus == .authorizedWhenInUse {
      manager.startMonitoringVisits()
      apply(load("plan.json", LocationPlan()))
    }
  }

  private func status() -> String {
    switch manager.authorizationStatus {
    case .authorizedAlways: return "always"
    case .authorizedWhenInUse: return "when_in_use"
    case .denied: return "denied"
    case .restricted: return "restricted"
    default: return "not_determined"
    }
  }

  /// Asks for location access. `step` is `when_in_use` first, then `always`; the second
  /// sheet only shows after the first was granted. `done` gets the resulting status.
  func authorize(step: String, done: @escaping (String) -> Void) {
    DispatchQueue.main.async {
      let now = self.status()
      let wantsAlways = step == "always"
      if now == "always" || now == "denied" || now == "restricted" || (!wantsAlways && now == "when_in_use") {
        done(now)
        return
      }
      self.waiting.append(done)
      if wantsAlways {
        self.manager.requestAlwaysAuthorization()
      } else {
        self.manager.requestWhenInUseAuthorization()
      }
      DispatchQueue.main.asyncAfter(deadline: .now() + 30) { self.finish() }
    }
  }

  private func finish() {
    let calls = waiting
    waiting.removeAll()
    let now = status()
    calls.forEach { $0(now) }
  }

  func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
    if status() != "not_determined" { finish() }
  }

  /// Starts or stops visit monitoring; stopping also clears every watched place.
  func setEnabled(_ on: Bool) {
    DispatchQueue.main.async {
      if on {
        self.manager.startMonitoringVisits()
      } else {
        self.manager.stopMonitoringVisits()
        self.watch(LocationPlan())
      }
    }
  }

  func locationManager(_ manager: CLLocationManager, didVisit visit: CLVisit) {
    let arrived = visit.arrivalDate == Date.distantPast ? nil : visit.arrivalDate
    guard let arrived = arrived else { return }
    let left = visit.departureDate == Date.distantFuture ? nil : LocationBridge.iso(visit.departureDate)
    let out = VisitOut(lat: visit.coordinate.latitude, lon: visit.coordinate.longitude, accuracy_m: visit.horizontalAccuracy, arrived_at: LocationBridge.iso(arrived), left_at: left)
    queue.async {
      var buffer = self.load("buffer.json", LocationBuffer())
      if buffer.visits.count < 2000 { buffer.visits.append(out) }
      self.save("buffer.json", buffer)
    }
  }

  /// Hands over and empties everything buffered, after adding the nudges whose time passed.
  func drain() -> LocationBuffer {
    queue.sync {
      var buffer = load("buffer.json", LocationBuffer())
      var scheduled: [String: ScheduledNudge] = load("scheduled.json", [:])
      let now = Date().timeIntervalSince1970
      for (id, s) in scheduled where s.fire <= now {
        let shown = NudgedOut(place_id: s.nudge.place_id, date: s.nudge.date)
        if !buffer.nudged.contains(shown) { buffer.nudged.append(shown) }
        scheduled.removeValue(forKey: id)
      }
      save("scheduled.json", scheduled)
      save("buffer.json", LocationBuffer())
      return buffer
    }
  }

  /// Stores the watched set and nudge plan and monitors exactly those places.
  func watch(_ plan: LocationPlan) {
    var plan = plan
    plan.places = Array(plan.places.prefix(LocationBridge.maxWatched))
    assert(plan.places.count <= LocationBridge.maxWatched)
    queue.sync { save("plan.json", plan) }
    DispatchQueue.main.async { self.apply(plan) }
  }

  private func apply(_ plan: LocationPlan) {
    if #available(iOS 17, *) {
      Task { @MainActor in await self.monitor(plan.places) }
    } else {
      let wanted = Set(plan.places.map { $0.id })
      for region in manager.monitoredRegions where !wanted.contains(region.identifier) {
        manager.stopMonitoring(for: region)
      }
      let have = Set(manager.monitoredRegions.map { $0.identifier })
      for p in plan.places where !have.contains(p.id) {
        let region = CLCircularRegion(center: CLLocationCoordinate2D(latitude: p.lat, longitude: p.lon), radius: CLLocationDistance(p.radius_m), identifier: p.id)
        region.notifyOnEntry = true
        region.notifyOnExit = true
        manager.startMonitoring(for: region)
      }
    }
  }

  @available(iOS 17, *)
  @MainActor private func monitor(_ places: [WatchedPlace]) async {
    let opening: Task<CLMonitor, Never>
    if let box = monitorBox as? Task<CLMonitor, Never> {
      opening = box
    } else {
      opening = Task { @MainActor in
        let created = await CLMonitor("ArohPlaces")
        Task {
          do {
            for try await event in await created.events {
              switch event.state {
              case .satisfied: self.entered(event.identifier, at: event.date)
              case .unsatisfied: self.left(event.identifier)
              default: break
              }
            }
          } catch {
            NSLog("aroh-ios: place monitor stopped")
          }
        }
        return created
      }
      monitorBox = opening
    }
    let monitor = await opening.value
    let wanted = Set(places.map { $0.id })
    for id in await monitor.identifiers where !wanted.contains(id) {
      await monitor.remove(id)
    }
    let have = Set(await monitor.identifiers)
    for p in places where !have.contains(p.id) {
      let condition = CLMonitor.CircularGeographicCondition(center: CLLocationCoordinate2D(latitude: p.lat, longitude: p.lon), radius: CLLocationDistance(p.radius_m))
      await monitor.add(condition, identifier: p.id, assuming: .unsatisfied)
    }
  }

  func locationManager(_ manager: CLLocationManager, didEnterRegion region: CLRegion) {
    entered(region.identifier, at: Date())
  }

  func locationManager(_ manager: CLLocationManager, didExitRegion region: CLRegion) {
    left(region.identifier)
  }

  private func entered(_ placeId: String, at: Date) {
    queue.async {
      let plan = self.load("plan.json", LocationPlan())
      var scheduled: [String: ScheduledNudge] = self.load("scheduled.json", [:])
      let fire = max(at, Date()).addingTimeInterval(LocationBridge.dwell)
      let day = LocationBridge.localDay(fire)
      guard let nudge = plan.nudges.first(where: { $0.place_id == placeId && $0.date == day }),
        scheduled[nudge.id] == nil,
        let from = LocationBridge.parse(nudge.from), let until = LocationBridge.parse(nudge.until),
        fire >= from, fire < until
      else { return }
      scheduled[nudge.id] = ScheduledNudge(nudge: nudge, fire: fire.timeIntervalSince1970)
      self.save("scheduled.json", scheduled)
      self.post(nudge, id: nudge.id, after: fire.timeIntervalSinceNow)
    }
  }

  private func left(_ placeId: String) {
    queue.async {
      var scheduled: [String: ScheduledNudge] = self.load("scheduled.json", [:])
      let now = Date().timeIntervalSince1970
      let early = scheduled.filter { !$0.key.hasPrefix(LocationBridge.later) && $0.value.nudge.place_id == placeId && $0.value.fire > now }.map { $0.key }
      guard !early.isEmpty else { return }
      early.forEach { scheduled.removeValue(forKey: $0) }
      self.save("scheduled.json", scheduled)
      UNUserNotificationCenter.current().removePendingNotificationRequests(withIdentifiers: early)
    }
  }

  private func post(_ nudge: PlannedNudge, id: String, after: TimeInterval) {
    let content = UNMutableNotificationContent()
    content.title = nudge.title
    content.body = nudge.body
    content.sound = .default
    content.categoryIdentifier = LocationBridge.category
    content.userInfo = ["action_id": nudge.action_id, "place_id": nudge.place_id, "date": nudge.date]
    let trigger = UNTimeIntervalNotificationTrigger(timeInterval: max(1, after), repeats: false)
    UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: id, content: content, trigger: trigger))
  }

  /// Puts back the nudges still waiting to show, after every pending request was removed
  /// to reschedule reminders.
  func restorePending() {
    queue.async {
      let scheduled: [String: ScheduledNudge] = self.load("scheduled.json", [:])
      let now = Date().timeIntervalSince1970
      for (id, s) in scheduled where s.fire > now {
        self.post(s.nudge, id: id, after: s.fire - now)
      }
    }
  }

  /// Handles a nudge answer: Mark done and Skip today are buffered for the drain, Remind me
  /// in 1 hour shows the same nudge again an hour later. Output: whether it was a nudge.
  func respond(_ response: UNNotificationResponse) -> Bool {
    let request = response.notification.request
    guard request.content.categoryIdentifier == LocationBridge.category,
      let action = request.content.userInfo["action_id"] as? String,
      let place = request.content.userInfo["place_id"] as? String,
      let date = request.content.userInfo["date"] as? String
    else { return false }
    let choice = response.actionIdentifier
    queue.async {
      var buffer = self.load("buffer.json", LocationBuffer())
      let shown = NudgedOut(place_id: place, date: date)
      if !buffer.nudged.contains(shown) { buffer.nudged.append(shown) }
      if choice == "done" || choice == "skip" {
        buffer.responses.append(ResponseOut(action_id: action, choice: choice))
      }
      self.save("buffer.json", buffer)
      if choice == "later" {
        let content = request.content
        let nudge = PlannedNudge(id: request.identifier, place_id: place, date: date, action_id: action, title: content.title, body: content.body, from: "", until: "")
        let id = LocationBridge.later + place + "." + date
        var scheduled: [String: ScheduledNudge] = self.load("scheduled.json", [:])
        scheduled[id] = ScheduledNudge(nudge: nudge, fire: Date().addingTimeInterval(3600).timeIntervalSince1970)
        self.save("scheduled.json", scheduled)
        self.post(nudge, id: id, after: 3600)
      }
    }
    return true
  }

  /// Finds places for `query` near the current region of the device. `done` gets up to 10 hits.
  func search(_ query: String, done: @escaping ([PlaceHit]) -> Void) {
    let request = MKLocalSearch.Request()
    request.naturalLanguageQuery = query
    if let here = manager.location {
      request.region = MKCoordinateRegion(center: here.coordinate, latitudinalMeters: 20000, longitudinalMeters: 20000)
    }
    MKLocalSearch(request: request).start { response, _ in
      let hits = (response?.mapItems ?? []).prefix(10).map { item in
        PlaceHit(name: item.name, kind: item.pointOfInterestCategory?.rawValue, address: LocationBridge.address(item.placemark), lat: item.placemark.coordinate.latitude, lon: item.placemark.coordinate.longitude)
      }
      DispatchQueue.main.async { done(Array(hits)) }
    }
  }

  private static func address(_ mark: CLPlacemark) -> String? {
    let parts = [[mark.subThoroughfare, mark.thoroughfare].compactMap { $0 }.joined(separator: " "), mark.locality ?? ""].filter { !$0.isEmpty }
    return parts.isEmpty ? nil : parts.joined(separator: ", ")
  }

  /// The nearest point of interest within 100 m of a spot and its address. `done` gets one hit.
  func lookup(lat: Double, lon: Double, done: @escaping (PlaceHit) -> Void) {
    let center = CLLocationCoordinate2D(latitude: lat, longitude: lon)
    let request = MKLocalPointsOfInterestRequest(center: center, radius: 100)
    MKLocalSearch(request: request).start { response, _ in
      let spot = CLLocation(latitude: lat, longitude: lon)
      let item = (response?.mapItems ?? []).min { a, b in
        (a.placemark.location?.distance(from: spot) ?? .infinity) < (b.placemark.location?.distance(from: spot) ?? .infinity)
      }
      CLGeocoder().reverseGeocodeLocation(spot) { marks, _ in
        let address = marks?.first.flatMap(LocationBridge.address) ?? item.flatMap { LocationBridge.address($0.placemark) }
        let hit = PlaceHit(name: item?.name, kind: item?.pointOfInterestCategory?.rawValue, address: address, lat: lat, lon: lon)
        DispatchQueue.main.async { done(hit) }
      }
    }
  }

  /// A map image of a spot with a pin, as a PNG data URL, `width` by `height` points.
  func snapshot(lat: Double, lon: Double, width: Double, height: Double, done: @escaping (String?) -> Void) {
    let options = MKMapSnapshotter.Options()
    let center = CLLocationCoordinate2D(latitude: lat, longitude: lon)
    options.region = MKCoordinateRegion(center: center, latitudinalMeters: 600, longitudinalMeters: 600)
    options.size = CGSize(width: min(max(width, 40), 1200), height: min(max(height, 40), 1200))
    MKMapSnapshotter(options: options).start { shot, _ in
      guard let shot = shot else {
        DispatchQueue.main.async { done(nil) }
        return
      }
      let image = UIGraphicsImageRenderer(size: options.size).image { _ in
        shot.image.draw(at: .zero)
        let p = shot.point(for: center)
        UIColor.systemRed.setFill()
        UIBezierPath(ovalIn: CGRect(x: p.x - 7, y: p.y - 7, width: 14, height: 14)).fill()
      }
      let url = image.pngData().map { "data:image/png;base64," + $0.base64EncodedString() }
      DispatchQueue.main.async { done(url) }
    }
  }
}
