use station_core::{raw_to_percent, Calibration, Ema};

pub const OVERSAMPLE_COUNT: u32 = 32;
pub const ALPHA: f32 = 0.2;
pub const PRINT_RATE: u64 = 500_000;

pub struct AppState {
    pub ema1: Ema,
    pub ema2: Ema,
    pub cal1: Calibration,
    pub cal2: Calibration,
    pub sweep1: Calibration,
    pub sweep2: Calibration,
    pub calibrating: bool,
    pub info_last_time: u64,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            ema1: Ema::new(ALPHA),
            ema2: Ema::new(ALPHA),
            cal1: Calibration::full_range(),
            cal2: Calibration::full_range(),
            sweep1: Calibration::start_sweep(),
            sweep2: Calibration::start_sweep(),
            calibrating: false,
            info_last_time: 0,
        }
    }

    pub fn handle_hold(&mut self) {
        if self.calibrating {
            if self.sweep1.is_valid() && self.sweep2.is_valid() {
                self.cal1 = self.sweep1;
                self.cal2 = self.sweep2;
                defmt::info!(
                    "Calibration saved: pot1 {}...{}  pot2 {}...{}",
                    self.cal1.min,
                    self.cal1.max,
                    self.cal2.min,
                    self.cal2.max
                );
            } else {
                defmt::warn!("Calibration incomplete - pot wasn't swept, keeping previous values");
            }
            self.calibrating = false;
        } else {
            self.sweep1 = Calibration::start_sweep();
            self.sweep2 = Calibration::start_sweep();
            self.calibrating = true;
            defmt::info!("calibration started - sweep both pots, hold again to finish");
        }
    }

    pub fn update_inputs(&mut self, pot1_raw: u16, pot2_raw: u16, now: u64) -> (u16, u16, u8, u8) {
        let pot1_smoothed = self.ema1.update(pot1_raw as f32);
        let pot2_smoothed = self.ema2.update(pot2_raw as f32);

        if self.calibrating {
            self.sweep1.observe(pot1_smoothed as u16);
            self.sweep2.observe(pot2_smoothed as u16);
        }

        let pct1 = raw_to_percent(pot1_smoothed as u16, self.cal1.min, self.cal1.max);
        let pct2 = raw_to_percent(pot2_smoothed as u16, self.cal2.min, self.cal2.max);

        if now - self.info_last_time >= PRINT_RATE {
            self.info_last_time = now;
            defmt::info!(
                "ema1={=u16} ema2={=u16}",
                pot1_smoothed as u16,
                pot2_smoothed as u16,
            );
        }

        (pot1_smoothed as u16, pot2_smoothed as u16, pct1, pct2)
    }
}
