//! Pure logic for pico-pot-meter: no HAL, no hardware, no_std-compatible.
//! Everything here takes plain numbers in and returns plain numbers out,
//! so it's testable with `cargo test` on the host — no board required.
#![cfg_attr(not(test), no_std)]
/// Map a raw ADC reading to a 0-100 percentage, given calibrated min/max
/// bounds. Handles both wiring directions:
/// - `min < max`: normal pot, raw counts increase as % increases.
/// - `min > max`: inverted pot (wired backwards, or CCW = 100%) — this is
///   just `min`/`max` swapped from the caller's point of view, and the
///   math below handles it without a separate code path.
///   Always clamps to 0..=100, so a raw reading outside the calibrated
///   range (noise, or calibration that didn't quite reach the physical
///   stop) never produces a nonsense percentage.
pub fn raw_to_percent(raw: u16, min: u16, max: u16) -> u8 {
    if min == max {
        return 0;
    }

    let (low, high, inv) = if min < max {
        (min, max, false)
    } else {
        (max, min, true)
    };
    let clamped = raw.clamp(low, high);
    let span = (high - low) as u32;
    let offset = (clamped - low) as u32;
    let res = (offset * 100) / span;
    let pct = if inv { 100 - res } else { res };

    pct as u8
}

pub fn page_index_from_percent(pct: u8, num_pages: usize) -> usize {
    if num_pages == 0 {
        return 0;
    }
    let pct = pct.min(100) as usize; //avoid degenerate values
    let idx = (pct * num_pages) / 101;
    idx.min(num_pages - 1)
}

/// Simple exponential moving average filter for smoothing ADC noise.
/// `alpha` is the weight given to each new sample (0.0..=1.0) — lower
/// values smooth harder but respond to real changes more slowly.
#[derive(Copy, Clone)]
pub struct Ema {
    alpha: f32,
    value: Option<f32>,
}

impl Ema {
    pub const fn new(alpha: f32) -> Self {
        Self { alpha, value: None }
    }

    pub fn update(&mut self, sample: f32) -> f32 {
        let filtered = match self.value {
            None => sample, //first value , nothing to average against
            Some(prev) => prev + self.alpha * (sample - prev),
        };
        self.value = Some(filtered);
        filtered
    }
}

#[derive(Copy, Clone)]
pub struct Calibration {
    pub min: u16,
    pub max: u16,
}

impl Calibration {
    pub fn full_range() -> Self {
        Self { min: 0, max: 4095 }
    }

    pub fn start_sweep() -> Self {
        Self {
            min: u16::MAX,
            max: 0,
        }
    }

    pub fn observe(&mut self, raw: u16) {
        self.min = self.min.min(raw);
        self.max = self.max.max(raw);
    }

    pub fn is_valid(&self) -> bool {
        self.min < self.max
    }
}
#[derive(Copy, Clone)]
pub struct MinMaxTracker {
    pub temp_min: f32,
    pub temp_max: f32,
    pub humidity_min: f32,
    pub humidity_max: f32,
    pub pressure_min: f32,
    pub pressure_max: f32,
    initialized: bool,
}
impl MinMaxTracker {
    pub const fn new() -> Self {
        Self {
            temp_min: 0.0,
            temp_max: 0.0,
            humidity_min: 0.0,
            humidity_max: 0.0,
            pressure_min: 0.0,
            pressure_max: 0.0,
            initialized: false,
        }
    }

    pub fn observe(&mut self, temp: f32, humidity: f32, pressure: f32) {
        if !self.initialized {
            self.temp_min = temp;
            self.temp_max = temp;
            self.humidity_min = humidity;
            self.humidity_max = humidity;
            self.pressure_min = pressure;
            self.pressure_max = pressure;
            self.initialized = true;
            return;
        }
        self.temp_min = self.temp_min.min(temp);
        self.temp_max = self.temp_max.max(temp);
        self.humidity_min = self.humidity_min.min(humidity);
        self.humidity_max = self.humidity_max.max(humidity);
        self.pressure_min = self.pressure_min.min(pressure);
        self.pressure_max = self.pressure_max.max(pressure);
    }
}

pub fn value_to_graph_y(value: f32, min: f32, max: f32, height: u32) -> u32 {
    if max <= min {
        return height.saturating_sub(1) / 2;
    }
    let clamped = value.clamp(min, max);
    let frac = (clamped - min) / (max - min); // 0.0 at min, 1.0 at max
    let from_top = (1.0 - frac) * height.saturating_sub(1) as f32;
    from_top as u32
}

/// Magnus-formula dew point from temperature and relative humidity.
/// Accurate to within ~0.4°C over the range a hobby weather station
/// actually sees (roughly 0-60°C, RH above a percent or so).
///
/// RH is clamped away from 0 before the log — RH=0% is physically
/// nonsensical anyway (dew point is undefined at zero humidity), and
/// `ln(0)` would otherwise produce `-inf` and poison the result.
pub fn dew_point_c(temp_c: f32, rh_pct: f32) -> f32 {
    const A: f32 = 17.62;
    const B: f32 = 243.12;
    let rh = rh_pct.clamp(0.1, 100.0) / 100.0;
    let gamma = libm::logf(rh) + (A * temp_c) / (B + temp_c);
    (B * gamma) / (A - gamma)
}

/// Adjust a station pressure reading to what it would read at sea level,
/// given the station's altitude. This is the standard barometric
/// formula behind the altitude functions in common BME280/BMP libraries
/// (Adafruit's included) — it assumes the International Standard
/// Atmosphere's temperature/pressure profile rather than today's actual
/// conditions, which is why the constants (44330, 5.255) are fixed
/// rather than derived from a live temperature reading. Good enough for
/// a hobby station's relative trends; not survey-grade absolute accuracy.
pub fn sea_level_pressure(station_pressure_pa: f32, altitude_m: f32) -> f32 {
    station_pressure_pa / libm::powf(1.0 - altitude_m / 44330.0, 5.255)
}
///Linearly maps a pct into a value between min_ticks and max_ticks
pub fn interval_from_percent(pct: u8, min_ticks: u64, max_ticks: u64) -> u64 {
    let pct = pct.min(100) as u64;
    min_ticks + ((max_ticks - min_ticks) * pct) / 100
}

#[cfg(test)]
mod tests {
    use super::*;
    //-----------pots tests-------------
    #[test]
    fn midpoint_maps_to_fifty_percent() {
        assert_eq!(raw_to_percent(2048, 0, 4095), 50);
    }

    #[test]
    fn clamps_below_calibrated_min() {
        // raw below `min` (e.g. pot settled slightly past where calibration
        // caught it) must still report 0, not wrap or underflow.
        assert_eq!(raw_to_percent(0, 100, 4095), 0);
    }

    #[test]
    fn clamps_above_calibrated_max() {
        assert_eq!(raw_to_percent(4095, 0, 4000), 100);
    }

    #[test]
    fn inverted_pot_at_raw_min_reads_100_percent() {
        // min > max signals the pot is wired/read backwards: the raw
        // minimum should now correspond to 100%, not 0%.
        assert_eq!(raw_to_percent(0, 4095, 0), 100);
    }

    #[test]
    fn inverted_pot_at_raw_max_reads_0_percent() {
        assert_eq!(raw_to_percent(4095, 4095, 0), 0);
    }

    #[test]
    fn degenerate_calibration_reports_zero_not_panic() {
        assert_eq!(raw_to_percent(2048, 100, 100), 0);
    }

    #[test]
    fn ema_converges_toward_a_constant_input() {
        let mut ema = Ema::new(0.2);
        let mut last = ema.update(0.0);
        for _ in 0..50 {
            last = ema.update(100.0);
        }
        assert!(
            (last - 100.0).abs() < 1.0,
            "expected convergence near 100, got {last}"
        );
    }
    //---------------------Calibration tests-------------------
    #[test]
    fn calibration_widens_as_samples_come_in() {
        let mut cal = Calibration::start_sweep();
        cal.observe(1500);
        cal.observe(300);
        cal.observe(3900);
        cal.observe(2000); // inside the current range — shouldn't change bounds
        assert_eq!(cal.min, 300);
        assert_eq!(cal.max, 3900);
    }

    #[test]
    fn calibration_single_sample_is_invalid() {
        // pot never actually moved during the sweep window
        let mut cal = Calibration::start_sweep();
        cal.observe(2048);
        assert!(!cal.is_valid());
    }

    #[test]
    fn calibration_after_real_sweep_is_valid() {
        let mut cal = Calibration::start_sweep();
        cal.observe(100);
        cal.observe(4000);
        assert!(cal.is_valid());
    }
    //---------------Page tests---------------
    #[test]
    fn zero_percent_is_first_page() {
        assert_eq!(page_index_from_percent(0, 4), 0);
    }

    #[test]
    fn hundred_percent_is_last_page() {
        assert_eq!(page_index_from_percent(100, 4), 3);
    }

    #[test]
    fn never_exceeds_bounds_even_with_a_bad_input() {
        assert_eq!(page_index_from_percent(255, 4), 3);
    }

    #[test]
    fn is_monotonic_non_decreasing_across_the_full_sweep() {
        let mut last = 0;
        for pct in 0..=100u8 {
            let idx = page_index_from_percent(pct, 4);
            assert!(idx >= last, "page index went backwards at {pct}%");
            last = idx;
        }
    }

    #[test]
    fn every_page_is_reachable_across_the_full_sweep() {
        let mut seen = [false; 4];
        for pct in 0..=100u8 {
            seen[page_index_from_percent(pct, 4)] = true;
        }
        assert!(
            seen.iter().all(|&s| s),
            "not every page was reachable: {seen:?}"
        );
    }

    #[test]
    fn single_page_is_always_index_zero() {
        assert_eq!(page_index_from_percent(50, 1), 0);
    }
    //----------------MinMax-----------------
    #[test]
    fn min_max_tracks_across_multiple_readings() {
        let mut mm = MinMaxTracker::new();
        mm.observe(20.0, 45.0, 101300.0);
        mm.observe(18.0, 50.0, 101250.0);
        mm.observe(22.0, 40.0, 101400.0);
        assert_eq!(mm.temp_min, 18.0);
        assert_eq!(mm.temp_max, 22.0);
        assert_eq!(mm.humidity_min, 40.0);
        assert_eq!(mm.humidity_max, 50.0);
        assert_eq!(mm.pressure_min, 101250.0);
        assert_eq!(mm.pressure_max, 101400.0);
    }
    //-----------Graph tests--------------
    #[test]
    fn value_at_min_maps_to_bottom_row() {
        assert_eq!(value_to_graph_y(0.0, 0.0, 10.0, 50), 49);
    }

    #[test]
    fn value_at_max_maps_to_top_row() {
        assert_eq!(value_to_graph_y(10.0, 0.0, 10.0, 50), 0);
    }

    #[test]
    fn degenerate_range_returns_middle_row() {
        assert_eq!(value_to_graph_y(5.0, 5.0, 5.0, 50), 24);
    }

    #[test]
    fn value_above_max_clamps_to_top_row() {
        assert_eq!(value_to_graph_y(999.0, 0.0, 10.0, 50), 0);
    }

    #[test]
    fn value_below_min_clamps_to_bottom_row() {
        assert_eq!(value_to_graph_y(-999.0, 0.0, 10.0, 50), 49);
    }

    #[test]
    fn dew_point_matches_reference_table_25c_60rh() {
        // Standard psychrometric reference value: 25°C / 60% RH -> ~16.7°C
        assert!((dew_point_c(25.0, 60.0) - 16.7).abs() < 0.3);
    }

    #[test]
    fn dew_point_matches_reference_table_20c_50rh() {
        // ~9.3°C per standard reference tables
        assert!((dew_point_c(20.0, 50.0) - 9.3).abs() < 0.3);
    }

    #[test]
    fn dew_point_equals_air_temp_at_100_percent_humidity() {
        // Not just a reference-table check — this is an exact identity
        // of the Magnus formula itself: at RH=100%, ln(1)=0, and the
        // algebra collapses to dew_point == temp_c regardless of value.
        for temp in [-10.0, 0.0, 15.0, 30.0, 45.0] {
            assert!((dew_point_c(temp, 100.0) - temp).abs() < 0.01);
        }
    }

    #[test]
    fn dew_point_never_exceeds_air_temperature() {
        // Physical invariant: dew point can't be warmer than the air
        // it's condensing out of, for any humidity below saturation.
        for rh in [10.0, 30.0, 50.0, 70.0, 90.0, 99.0] {
            assert!(dew_point_c(20.0, rh) <= 20.0 + 0.01);
        }
    }

    #[test]
    fn sea_level_pressure_at_zero_altitude_is_unchanged() {
        // altitude=0 -> the correction factor is exactly 1.0
        assert!((sea_level_pressure(95000.0, 0.0) - 95000.0).abs() < 0.01);
    }

    #[test]
    fn sea_level_pressure_reconstructs_isa_standard_at_500m() {
        // ICAO Standard Atmosphere: pressure at 500m altitude is
        // 95461 Pa. Feeding that back through this function at the same
        // altitude should reconstruct the ISA sea-level standard,
        // 101325 Pa — the 44330/5.255 constants are literally derived
        // from this reference atmosphere, so this checks the constants
        // are wired up correctly, not just that the formula "runs".
        let result = sea_level_pressure(95461.0, 500.0);
        assert!(
            (result - 101325.0).abs() < 50.0,
            "got {result}, expected ~101325"
        );
    }

    #[test]
    fn sea_level_pressure_reconstructs_isa_standard_at_1000m() {
        // ISA pressure at 1000m: 89876 Pa
        let result = sea_level_pressure(89876.0, 1000.0);
        assert!(
            (result - 101325.0).abs() < 50.0,
            "got {result}, expected ~101325"
        );
    }

    #[test]
    fn sea_level_pressure_is_always_at_least_station_pressure() {
        // Physical invariant for any non-negative altitude: sea-level-
        // equivalent pressure can't read lower than what the station
        // actually measured.
        for altitude in [0.0, 100.0, 500.0, 1500.0, 3000.0] {
            assert!(sea_level_pressure(90000.0, altitude) >= 90000.0 - 0.01);
        }
    }
    #[test]
    fn interval_at_zero_percent_is_the_minimum() {
        assert_eq!(interval_from_percent(0, 500_000, 5_000_000), 500_000);
    }

    #[test]
    fn interval_at_hundred_percent_is_the_maximum() {
        assert_eq!(interval_from_percent(100, 500_000, 5_000_000), 5_000_000);
    }

    #[test]
    fn interval_at_fifty_percent_is_the_midpoint() {
        assert_eq!(interval_from_percent(50, 500_000, 5_000_000), 2_750_000);
    }

    #[test]
    fn interval_above_100_percent_clamps_to_maximum() {
        assert_eq!(interval_from_percent(255, 500_000, 5_000_000), 5_000_000);
    }
}
