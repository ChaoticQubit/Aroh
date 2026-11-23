//! Distances between points stored in microdegrees.

/// "Near" a place (Q56): within this many meters of its center.
pub const NEAR_M: f64 = 150.0;

const EARTH_M: f64 = 6_371_000.0;

/// Great-circle distance in meters between two `(lat_e6, lon_e6)` points (haversine).
/// Output is finite and never negative for points on the globe.
pub fn distance_m(a: (i64, i64), b: (i64, i64)) -> f64 {
    let rad = |e6: i64| (e6 as f64 / 1e6).to_radians();
    let (lat1, lat2) = (rad(a.0), rad(b.0));
    let dlat = lat2 - lat1;
    let dlon = rad(b.1) - rad(a.1);
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    let d = 2.0 * EARTH_M * h.sqrt().min(1.0).asin();
    assert!(d.is_finite() && d >= 0.0, "distance must be a finite length");
    d
}

/// Degrees as microdegrees, or `None` for a value that is not finite or off the globe
/// (latitude beyond 90 when `lat`, longitude beyond 180 otherwise).
pub fn to_e6(degrees: f64, lat: bool) -> Option<i64> {
    let limit = if lat { 90.0 } else { 180.0 };
    (degrees.is_finite() && degrees.abs() <= limit).then(|| (degrees * 1e6).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_microdegree_of_latitude_is_about_eleven_centimeters() {
        let d = distance_m((37_776_000, -122_417_000), (37_776_000 + 1350, -122_417_000));
        assert!((d - 150.0).abs() < 1.0, "{d}");
        assert_eq!(to_e6(37.776, true), Some(37_776_000));
        assert_eq!(to_e6(91.0, true), None);
        assert_eq!(to_e6(f64::NAN, false), None);
    }
}
