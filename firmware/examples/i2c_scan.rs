
#![no_std]
#![no_main]

use core::cell::RefCell;
use cortex_m_rt::entry;
use critical_section::Mutex;
use embedded_hal::i2c::I2c;
use panic_halt as _;
use rp2040_hal as hal;
use static_cell::StaticCell;

use hal::{
    clocks::init_clocks_and_plls, fugit::RateExtU32, gpio::Pins, pac, sio::Sio, timer::Timer,
    usb::UsbBus, watchdog::Watchdog, I2C,
};
use usb_device::{class_prelude::*, prelude::*};
use usbd_serial::SerialPort;

type UsbState = (UsbDevice<'static, UsbBus>, SerialPort<'static, UsbBus>);
static USB_STATE: Mutex<RefCell<Option<UsbState>>> = Mutex::new(RefCell::new(None));

struct DefmtUsbWriter;

impl embedded_io::ErrorType for DefmtUsbWriter {
    type Error = core::convert::Infallible;
}

impl embedded_io::Write for DefmtUsbWriter {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        let mut written = 0;
        let mut idle_polls = 0u32;
        const MAX_IDLE_POLLS: u32 = 200;

        while written < buf.len() && idle_polls < MAX_IDLE_POLLS {
            critical_section::with(|cs| {
                if let Some((usb_dev, serial)) = USB_STATE.borrow_ref_mut(cs).as_mut() {
                    usb_dev.poll(&mut [serial]);
                    match serial.write(&buf[written..]) {
                        Ok(n) if n > 0 => {
                            written += n;
                            idle_polls = 0;
                        }
                        Ok(_) | Err(UsbError::WouldBlock) => idle_polls += 1,
                        Err(_) => idle_polls = MAX_IDLE_POLLS,
                    }
                }
            });
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn poll_usb() {
    critical_section::with(|cs| {
        if let Some((usb_dev, serial)) = USB_STATE.borrow_ref_mut(cs).as_mut() {
            usb_dev.poll(&mut [serial]);
        }
    });
}

fn wait_for_usb_settle(timer: &Timer, settle_ticks: u64) {
    let start = timer.get_counter().ticks();
    while timer.get_counter().ticks() - start < settle_ticks {
        poll_usb();
    }
}

static WRITER: StaticCell<DefmtUsbWriter> = StaticCell::new();
static USB_BUS: StaticCell<UsbBusAllocator<UsbBus>> = StaticCell::new();

#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_GENERIC_03H;

#[entry]
fn main() -> ! {
    let mut pac = pac::Peripherals::take().unwrap();
    let mut watchdog = Watchdog::new(pac.WATCHDOG);

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

    let sio = Sio::new(pac.SIO);
    let pins = Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );
    let timer = Timer::new(pac.TIMER, &mut pac.RESETS, &clocks);

    let usb_bus = USB_BUS.init(UsbBusAllocator::new(UsbBus::new(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    )));
    let serial = SerialPort::new(usb_bus);
    let usb_dev = UsbDeviceBuilder::new(usb_bus, UsbVidPid(0x16c0, 0x27dd))
        .strings(&[StringDescriptors::new(LangID::EN).product("i2c-scan")])
        .unwrap()
        .device_class(usbd_serial::USB_CLASS_CDC)
        .build();

    critical_section::with(|cs| {
        *USB_STATE.borrow_ref_mut(cs) = Some((usb_dev, serial));
    });
    defmt_serial::defmt_serial(WRITER.init(DefmtUsbWriter));
    wait_for_usb_settle(&timer, 3_000_000);
    defmt::info!("up");

    let sda_pin = pins
        .gpio4
        .reconfigure::<hal::gpio::FunctionI2C, hal::gpio::PullUp>();
    let scl_pin = pins
        .gpio5
        .reconfigure::<hal::gpio::FunctionI2C, hal::gpio::PullUp>();

    let mut i2c = I2C::i2c0(
        pac.I2C0,
        sda_pin,
        scl_pin,
        100.kHz(),
        &mut pac.RESETS,
        &clocks.peripheral_clock,
    );

    defmt::info!("starting i2c scan on I2C0 (GP4=SDA, GP5=SCL)");

    let mut found = 0u8;
    let mut dummy = [0u8; 1];

    for addr in 0x08u8..=0x77u8 {
        match i2c.read(addr, &mut dummy) {
            Ok(_) => {
                found += 1;
                defmt::info!("found device at 0x{:02X}", addr);
            }
            Err(_) => {}
        }
    }

    if found == 0 {
        defmt::warn!("no i2c devices found");
    } else {
        defmt::info!("scan complete, {} device(s) found", found);
    }

    loop {
        poll_usb();
    }
}
