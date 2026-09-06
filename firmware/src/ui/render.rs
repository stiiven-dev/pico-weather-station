use core::fmt::Write as _;

use crate::app::HISTORY_LEN;
use embedded_graphics::primitives::PrimitiveStyle;
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::Line,
    text::Text,
};
use heapless::HistoryBuf;
use ssd1306::{
    mode::BufferedGraphicsMode, prelude::WriteOnlyDataCommand, size::DisplaySize128x64, Ssd1306,
};
use station_core::{value_to_graph_y, MinMaxTracker};

const GRAPH_X0: i32 = 4;
const GRAPH_WIDTH: u32 = 120;
const GRAPH_Y0: i32 = 2;
const GRAPH_HEIGHT: u32 = 50;
const STATION_ALTITUDE_M: f32 = 86.0;
pub type DisplayType<DI> = Ssd1306<DI, DisplaySize128x64, BufferedGraphicsMode<DisplaySize128x64>>;

pub fn render_now<DI>(
    display: &mut DisplayType<DI>,
    temperature: f32,
    humidity: f32,
    pressure_pa: f32,
    frozen: bool,
) where
    DI: WriteOnlyDataCommand,
{
    if !frozen {
        let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
        display.clear(BinaryColor::Off).unwrap();

        let sea_level_pa = station_core::sea_level_pressure(pressure_pa, STATION_ALTITUDE_M);
        let dew_pt = station_core::dew_point_c(temperature, humidity);

        let mut buf = heapless::String::<20>::new();

        let _ = write!(buf, "Temp: {:.1}C", temperature);
        Text::new(&buf, Point::new(4, 14), style)
            .draw(display)
            .unwrap();
        buf.clear();

        let _ = write!(buf, "Hum: {:.0}%", humidity);
        Text::new(&buf, Point::new(4, 28), style)
            .draw(display)
            .unwrap();
        buf.clear();

        let _ = write!(buf, "Pres: {:.0}hPa", sea_level_pa / 100.0);
        Text::new(&buf, Point::new(4, 42), style)
            .draw(display)
            .unwrap();
        buf.clear();

        let _ = write!(buf, "Dew: {:.1}C", dew_pt);
        Text::new(&buf, Point::new(4, 56), style)
            .draw(display)
            .unwrap();

        display.flush().unwrap();
    }
}

pub fn render_read_failed<DI>(display: &mut DisplayType<DI>)
where
    DI: WriteOnlyDataCommand,
{
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    display.clear(BinaryColor::Off).unwrap();
    Text::new("Read failed", Point::new(50, 30), style)
        .draw(display)
        .unwrap();
    display.flush().unwrap();
}

pub fn render_min_max<DI>(display: &mut DisplayType<DI>, mm: &MinMaxTracker)
where
    DI: WriteOnlyDataCommand,
{
    display.clear(BinaryColor::Off).unwrap();
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let mut buf = heapless::String::<32>::new();

    let _ = write!(buf, "T {:.1}/{:.1}C", mm.temp_min, mm.temp_max);
    Text::new(&buf, Point::new(4, 14), style)
        .draw(display)
        .unwrap();
    buf.clear();

    let _ = write!(buf, "H {:.0}/{:.0}%", mm.humidity_min, mm.humidity_max);
    Text::new(&buf, Point::new(4, 32), style)
        .draw(display)
        .unwrap();
    buf.clear();

    // Pa -> hPa for a shorter, more readable number (101325 Pa -> 1013 hPa)
    let _ = write!(
        buf,
        "P {:.0}/{:.0}hPa",
        mm.pressure_min / 100.0,
        mm.pressure_max / 100.0
    );
    Text::new(&buf, Point::new(4, 50), style)
        .draw(display)
        .unwrap();

    display.flush().unwrap();
}

pub fn render_trend<DI>(
    display: &mut DisplayType<DI>,
    history: &HistoryBuf<f32, HISTORY_LEN>,
    frozen: bool,
) where
    DI: WriteOnlyDataCommand,
{
    if !frozen {
        display.clear(BinaryColor::Off).unwrap();
        if history.len() < 2 {
            let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
            Text::new("Collecting...", Point::new(10, 32), style)
                .draw(display)
                .unwrap();
            return;
        }

        let mut min = f32::MAX;
        let mut max = f32::MIN;
        for &v in history.oldest_ordered() {
            min = min.min(v);
            max = max.max(v);
        }

        let len = history.len();
        let step_x = GRAPH_WIDTH as f32 / (HISTORY_LEN as f32 - 1.0);
        let start_offset = (HISTORY_LEN - len) as f32 * step_x;

        let mut prev: Option<Point> = None;
        for (i, &v) in history.oldest_ordered().enumerate() {
            let x = GRAPH_X0 + (start_offset + i as f32 * step_x) as i32;
            let y = GRAPH_Y0 + value_to_graph_y(v, min, max, GRAPH_HEIGHT) as i32;
            let p = Point::new(x, y);
            if let Some(prev_p) = prev {
                Line::new(prev_p, p)
                    .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
                    .draw(display)
                    .unwrap();
            }
            prev = Some(p);
        }

        let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
        let mut buf = heapless::String::<24>::new();
        let _ = write!(buf, "{:.1}-{:.1}C", min, max);
        Text::new(&buf, Point::new(4, 62), style)
            .draw(display)
            .unwrap();
        display.flush().unwrap();
    }
}

pub fn render_about<DI>(display: &mut DisplayType<DI>, uptime_secs: u32, panic_recovered: bool)
where
    DI: WriteOnlyDataCommand,
{
    display.clear(BinaryColor::Off).unwrap();
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let mut buf = heapless::String::<32>::new();

    let _ = write!(buf, "v{}", env!("CARGO_PKG_VERSION"));
    Text::new(&buf, Point::new(4, 12), style)
        .draw(display)
        .unwrap();
    buf.clear();

    let _ = write!(buf, "git {}", env!("GIT_HASH"));
    Text::new(&buf, Point::new(4, 26), style)
        .draw(display)
        .unwrap();
    buf.clear();

    let h = uptime_secs / 3600;
    let m = (uptime_secs % 3600) / 60;
    let s = uptime_secs % 60;
    let _ = write!(buf, "up {:02}:{:02}:{:02}", h, m, s);
    Text::new(&buf, Point::new(4, 40), style)
        .draw(display)
        .unwrap();

    let status = if panic_recovered {
        "last: recovered"
    } else {
        "last: clean boot"
    };
    Text::new(status, Point::new(4, 54), style)
        .draw(display)
        .unwrap();
    display.flush().unwrap();
}
