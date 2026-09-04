use core::fmt::Write as _;

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};
use ssd1306::{
    mode::BufferedGraphicsMode,
    prelude::{DisplayConfig, WriteOnlyDataCommand},
    size::DisplaySize128x64,
    Ssd1306,
};

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
pub fn render_placeholder<D>(display: &mut D, label: &str) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    display.clear(BinaryColor::Off)?;
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    Text::new(label, Point::new(10, 30), style).draw(display)?;
    Ok(())
}
