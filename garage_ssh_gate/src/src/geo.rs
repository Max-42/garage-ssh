/// Haversine formula to calculate distance between two points on Earth
pub fn haversine_distance_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0;
    
    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();
    let delta_lat = (lat2 - lat1).to_radians();
    let delta_lon = (lon2 - lon1).to_radians();
    
    let a = (delta_lat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (delta_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    
    EARTH_RADIUS_KM * c
}

/// Check if a position is within the geofence radius
pub fn is_within_geofence(
    home_lat: f64,
    home_lon: f64,
    client_lat: f64,
    client_lon: f64,
    radius_km: f64,
) -> (bool, f64) {
    let distance = haversine_distance_km(home_lat, home_lon, client_lat, client_lon);
    (distance <= radius_km, distance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_same_location() {
        let (within, dist) = is_within_geofence(52.52, 13.405, 52.52, 13.405, 15.0);
        assert!(within);
        assert!(dist < 0.001);
    }

    #[test]
    fn test_nearby_location() {
        // Berlin to Potsdam ~= 27km
        let (within, _dist) = is_within_geofence(52.52, 13.405, 52.3906, 13.0645, 15.0);
        assert!(!within);
    }

    #[test]
    fn test_within_radius() {
        // Two points ~5km apart
        let (within, dist) = is_within_geofence(52.52, 13.405, 52.55, 13.40, 15.0);
        assert!(within);
        assert!(dist < 15.0);
    }
}
