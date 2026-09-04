#![no_std]
#![no_main]

mod app;
mod debouncer;
mod ui;

use debouncer::{ButtonEvent, ButtonMonitor};

use bme280::i2c::BME280;
use ssd1306::{prelude::*, I2CDisplayInterface, Ssd1306};

use core::cell::RefCell;
use cortex_m_rt::entry;
use critical_section::Mutex;
use embedded_hal_bus::i2c::RefCellDevice;
use static_cell::StaticCell;

use panic_persist as _;

use crate::ui::render::render_placeholder;
use app::AppState;
use embedded_hal::delay::DelayNs;
use hal::{
    clocks::init_clocks_and_plls, gpio::Pins, pac, sio::Sio, timer::Timer, usb::UsbBus,
    watchdog::Watchdog,
};
use rp2040_hal::fugit::RateExtU32;
use rp2040_hal::{self as hal, adc::AdcPin, Adc};
use ui::render::{render_now, render_read_failed};
use usb_device::{class_prelude::*, prelude::*};
use usbd_serial::SerialPort;

type UsbState = (UsbDevice<'static, UsbBus>, SerialPort<'static, UsbBus>);
static USB_STATE: Mutex<RefCell<Option<UsbState>>> = Mutex::new(RefCell::new(None));

// --- Button-triggered behavior tuning (timer ticks = microseconds) ---
const DEBOUNCE_TICKS: u64 = 10_000;
const MULTI_CLICKS_WINDOW_TICKS: u64 = 400_000;
const HOLD_TICKS: u64 = 1_500_000;
// --- Other consts ---
const POLL_BUTTON_TICKS: u64 = 5_000;
const DISPLAY_PERIOD_TICKS: u64 = 50_000; //instead of delay_ms(50)
const BME_READ_TICKS: u64 = 1_000_000;
struct DefmtUsbWriter;

struct PollingDelay<'a> {
    timer: &'a Timer,
}

impl<'a> DelayNs for PollingDelay<'a> {
    fn delay_ns(&mut self, ns: u32) {
        // Timer ticks are microseconds (same convention as the other
        // *_TICKS constants) — round up so we never wait slightly less
        // than requested.
        let wait_ticks = (ns as u64 + 999) / 1000;
        let start = self.timer.get_counter().ticks();
        while self.timer.get_counter().ticks().wrapping_sub(start) < wait_ticks {
            poll_usb(); //keep usb alive
        }
    }
}

impl embedded_io::ErrorType for DefmtUsbWriter {
    type Error = core::convert::Infallible;
}

impl embedded_io::Write for DefmtUsbWriter {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        let mut written = 0;
        let mut idle_polls = 0u32;
        const MAX_IDLE_POLLS: u32 = 200; // bound the spin — drop the rest if host isn't draining

        while written < buf.len() && idle_polls < MAX_IDLE_POLLS {
            critical_section::with(|cs| {
                if let Some((usb_dev, serial)) = USB_STATE.borrow_ref_mut(cs).as_mut() {
                    usb_dev.poll(&mut [serial]);
                    match serial.write(&buf[written..]) {
                        Ok(n) if n > 0 => {
                            written += n;
                            idle_polls = 0; // reset — we're making progress
                        }
                        Ok(_) | Err(UsbError::WouldBlock) => {
                            idle_polls += 1;
                        }
                        Err(_) => idle_polls = MAX_IDLE_POLLS, // bail on real errors
                    }
                }
            });
        }
        // Whether we wrote everything or gave up, tell the caller it's done —
        // dropping unsent log bytes is fine; hanging the app is not.
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn wait_for_usb_settle(timer: &Timer, settle_ticks: u64) {
    let start = timer.get_counter().ticks();
    while timer.get_counter().ticks() - start < settle_ticks {
        poll_usb();
    }
}

fn poll_usb() {
    critical_section::with(|cs| {
        if let Some((usb_dev, serial)) = USB_STATE.borrow_ref_mut(cs).as_mut() {
            usb_dev.poll(&mut [serial]);
        }
    });
}

static WRITER: StaticCell<DefmtUsbWriter> = StaticCell::new();
static USB_BUS: StaticCell<UsbBusAllocator<UsbBus>> = StaticCell::new();

/// Second-stage bootloader. Required: the RP2040 boot ROM reads this first
/// to know how to talk to QSPI flash. Without it, the chip can't validate
/// the flashed image and falls back to bootloader/mass-storage mode on
/// every reset instead of running the program.
#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_GENERIC_03H;

#[entry]
fn main() -> ! {
    //take ownership of rp2040 peripherals (singleton)
    let mut pac = pac::Peripherals::take().unwrap();

    //Watchdog
    let mut watchdog = Watchdog::new(pac.WATCHDOG);

    //Initialize clocks
    let clocks = init_clocks_and_plls(
        12_000_000,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    //SIO gives access to GPIO
    let sio = Sio::new(pac.SIO);

    //Initialize GPIO Pins
    let pins = Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );
    //Enable I2C peripheral
    let sda = pins
        .gpio4
        .reconfigure::<hal::gpio::FunctionI2C, hal::gpio::PullUp>();
    let scl = pins
        .gpio5
        .reconfigure::<hal::gpio::FunctionI2C, hal::gpio::PullUp>();
    let i2c = rp2040_hal::I2C::i2c0(
        pac.I2C0,
        sda,
        scl,
        400.kHz(),
        &mut pac.RESETS,
        &clocks.peripheral_clock,
    );
    //shared i2c bus
    let shared_i2c = RefCell::new(i2c);

    let i2c_for_bme = RefCellDevice::new(&shared_i2c);
    let i2c_for_oled = RefCellDevice::new(&shared_i2c);

    //Enable ADC peripheral
    let mut adc = Adc::new(pac.ADC, &mut pac.RESETS);

    //enable Adc pin 0 and 1
    let mut adc_pin0 = AdcPin::new(pins.gpio26.into_floating_input()).unwrap();
    let mut adc_pin1 = AdcPin::new(pins.gpio27.into_floating_input()).unwrap();

    //GPIO12 as button (pin 16)
    let button_pin = pins.gpio12.into_pull_up_input();

    //Timer
    let timer = Timer::new(pac.TIMER, &mut pac.RESETS, &clocks);
    //USB comms
    let usb_bus = USB_BUS.init(UsbBusAllocator::new(UsbBus::new(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    )));
    let serial = SerialPort::new(usb_bus);
    let usb_dev = UsbDeviceBuilder::new(usb_bus, UsbVidPid(0x16c0, 0x27dd))
        .strings(&[StringDescriptors::new(LangID::EN).product("pico-pot-meter")])
        .unwrap()
        .device_class(usbd_serial::USB_CLASS_CDC)
        .build();

    critical_section::with(|cs| {
        *USB_STATE.borrow_ref_mut(cs) = Some((usb_dev, serial));
    });

    defmt_serial::defmt_serial(WRITER.init(DefmtUsbWriter));

    wait_for_usb_settle(&timer, 3_000_000);
    defmt::info!("pico-weather-station is up!");

    if let Some(msg) = panic_persist::get_panic_message_utf8() {
        defmt::error!("previous boot panicked: {}", msg);
    }

    let now0 = timer.get_counter().ticks();
    let mut button = ButtonMonitor::new(
        button_pin,
        true,
        DEBOUNCE_TICKS,
        MULTI_CLICKS_WINDOW_TICKS,
        HOLD_TICKS,
        now0,
    )
    .unwrap();

    let mut app = AppState::new();
    let mut button_last_time = 0u64;
    let mut display_last_time = 0u64;

    //Display init
    let interface = I2CDisplayInterface::new(i2c_for_oled);
    let mut display = Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();
    display.init().unwrap();
    let mut bme_delay = PollingDelay { timer: &timer };
    let mut bme = BME280::new_primary(i2c_for_bme);
    bme.init(&mut bme_delay).unwrap();
    let mut bme_last_time = 0u64;
    let mut last_temp = 0.0f32;
    let mut last_humidity = 0.0f32;
    let mut last_pressure = 0.0f32;

    loop {
        poll_usb();
        let now = timer.get_counter().ticks();
        if now.wrapping_sub(button_last_time) >= POLL_BUTTON_TICKS {
            button_last_time = now;
            match button.update(now).unwrap() {
                ButtonEvent::HoldTriggered => {
                    app.handle_hold();
                }
                ButtonEvent::Clicks(n) => app.handle_clicks(n),
                ButtonEvent::None => {}
            }
        }
        if now.wrapping_sub(bme_last_time) >= BME_READ_TICKS {
            bme_last_time = now;
            match bme.measure(&mut bme_delay) {
                Ok(m) => {
                    // m.temperature (°C), m.humidity (%RH), m.pressure (Pa)
                    defmt::info!(
                        "bme t={=f32}C rh={=f32}% p={=f32}Pa",
                        m.temperature,
                        m.humidity,
                        m.pressure
                    );
                    last_temp = m.temperature;
                    last_humidity = m.humidity;
                    last_pressure = m.pressure;
                }
                Err(_) => {
                    defmt::warn!("BME280 read failed");
                    render_read_failed(&mut display);
                }
            }
        }
        if now.wrapping_sub(display_last_time) >= DISPLAY_PERIOD_TICKS {
            display_last_time = now;
            //oversampling
            let mut pot1_sum: u32 = 0; //u32 because worst case scenario is 131_040 which surpasses u16::MAX
            let mut pot2_sum: u32 = 0;

            for _ in 0..app::OVERSAMPLE_COUNT {
                let raw_val1 = adc.read(&mut adc_pin0).unwrap();
                let raw_val2 = adc.read(&mut adc_pin1).unwrap();
                pot1_sum += raw_val1 as u32;
                pot2_sum += raw_val2 as u32;
            }
            let pot1_raw = (pot1_sum / app::OVERSAMPLE_COUNT) as u16;
            let pot2_raw = (pot2_sum / app::OVERSAMPLE_COUNT) as u16;
            let _ = app.update_inputs(pot1_raw, pot2_raw, now);
            match app.ui.page {
                ui::Page::Now => {
                    render_now(
                        &mut display,
                        last_temp,
                        last_humidity,
                        last_pressure,
                        app.ui.frozen,
                    );
                }
                ui::Page::MinMax => {
                    render_placeholder(&mut display, "Min/Max");
                }
                ui::Page::Trend => {
                    render_placeholder(&mut display, "Trend");
                }
                ui::Page::About => {
                    render_placeholder(&mut display, "About");
                }
            }
        }
    }
}
