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
            self.humidity_min = humidity;
            self.pressure_min = pressure;
            self.pressure_min = pressure;
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
}
