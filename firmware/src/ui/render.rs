use core::fmt::Write as _;

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};
use ssd1306::{
    mode::BufferedGraphicsMode, prelude::WriteOnlyDataCommand, size::DisplaySize128x64, Ssd1306,
};
use station_core::MinMaxTracker;
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

        let mut temp_buffer = heapless::String::<32>::new();
        let mut hum_buffer = heapless::String::<32>::new();
        let mut press_buffer = heapless::String::<32>::new();

        let _ = write!(temp_buffer, "Temp: {:.2} C", temperature);
        let _ = write!(hum_buffer, "Hum: {:.2} %", humidity);
        let _ = write!(press_buffer, "Pressure: {:.2} hPa", pressure_pa / 100.0);

        Text::new(&temp_buffer, Point::new(0, 15), style)
            .draw(display)
            .unwrap();
        Text::new(&hum_buffer, Point::new(0, 30), style)
            .draw(display)
            .unwrap();
        Text::new(&press_buffer, Point::new(0, 45), style)
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
//Quick stub, replace per-page later
pub fn render_placeholder<DI>(display: &mut DisplayType<DI>, label: &str)
where
    DI: WriteOnlyDataCommand,
{
    display.clear(BinaryColor::Off).unwrap();
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    Text::new(label, Point::new(10, 30), style)
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
