# pico-weather-station

BME280 environmental sensor + OLED, with a multipage UI navigated by the same two pots from `pico-pot-meter` — no debug probe required.

![breadboard](docs/images/breadboard)

<!-- TODO: demo GIF — cycling through pages, then unplugging the sensor mid-run to show the error page and auto-recovery -->
`docs/images/demo.gif`

## Features

- Four pages, cycled with pot #1: **Now** (live temp/humidity/pressure), **Min/Max** (session extremes), **Trend** (60-sample rolling graph), **About** (firmware version + git hash).
- Pot #2 sets the sample interval; button freezes the current page (handy for actually reading the trend graph instead of watching it scroll).
- Dew point and sea-level-adjusted pressure computed in [`crates/station-core`](crates/station-core), unit-tested against known reference values on the host.
- **Graceful sensor-fault recovery**: unplug the BME280 mid-run and the display switches to an error page instead of hanging or panicking — replug it, and it recovers on its own, no reboot needed. This is the actual point of the project; most hobby firmware just `unwrap()`s on the first bad I²C read and dies.
- Two I²C devices sharing one bus (BME280 + OLED) — the first project in this series that needs the shared-bus pattern instead of owning a peripheral outright.
- Same USB-only dev loop as every project before this one: `defmt-serial` logging, `panic-persist` crash capture, no SWD probe.


## To-do list

- [x] BME280 reads over defmt (raw temp/humidity/pressure, unfiltered)
- [x] shared-bus refactor — OLED and BME280 coexisting on I2C0
- [x] page state machine: pot #1 → page index, button → freeze/unfreeze
- [x] Now / Min-Max / Trend / About pages rendered
- [x] `heapless::HistoryBuffer` ring buffer feeding the trend graph
- [x] dew point + sea-level pressure formulas in `station-core`, tested against reference tables
- [x] fault injection: unplug the sensor mid-run, confirm error page + auto-recovery (no reboot)
- [ ] Finalize cleanup, polish, and any missing documentation before release.

## Hardware BOM

| Part                              |   Qty | Notes                                    |
|-----------------------------------|------:|------------------------------------------|
| Raspberry Pi Pico W               |     1 | RP2040 + CYW43439                        |
| GY-BME280                         |     1 | 3.3 V I²C sensor                         |
| SSD1306/SSD1309 OLED 0.96" 128×64 |     1 | I²C display                              |
| 10 kΩ potentiometer               |     2 | One per menu/control axis                |
| Breadboard + jumper wires         | 1 set | Prototyping hardware                     |
| 3.3 V rail and common ground      |     1 | Required for stable I²C and sensor power |

## Wiring

| Pico W pin    | Signal                           | Notes                     |
|---------------|----------------------------------|---------------------------|
| GP4 (pin 6)   | I²C0 SDA                         | shared by BME280 and OLED |
| GP5 (pin 7)   | I²C0 SCL                         | shared by BME280 and OLED |
| GP26 (pin 31) | Pot #1 wiper (ADC0)              | page selector             |
| GP27 (pin 32) | Pot #2 wiper (ADC1)              | sample interval           |
| GP12 (pin 16) | Button, other leg to GND         | internal pull-up          |
| 3V3 (pin 36)  | BME280 VCC + OLED VCC + pot ends | **BME280 is 3.3 V only**  |
| GND (pin 38)  | Common ground                    | shared by every module    |

```text
Pico W pin 36 (3V3 OUT) ──┬── BME280 VCC
                          ├── OLED VCC
                          ├── pot #1 high side
                          └── pot #2 high side

Pico W pin 38 (GND)     ──┴── all grounds

GP4  ── SDA ── BME280 / OLED
GP5  ── SCL ── BME280 / OLED
GP26 ── pot #1 wiper
GP27 ── pot #2 wiper
GP12 ── reboot-to-bootloader button
```

BME280 is at I²C address `0x76` (some boards ship at `0x77` — run the bus scanner from `firmware/examples/i2c_scan.rs` 
if it isn't found). Each breakout carries its own pull-ups; 
two devices in parallel is still comfortably within range at 3.3 V.
## Quickstart

```bash
cargo run --release   # from inside firmware/, per the workspace's .cargo/config.toml
```

Watch logs the same way as the earlier projects:
```bash
./watch-defmt.sh
```

Turn pot #1 to switch pages, pot #2 to change the sample interval,
and press the button once to freeze whatever page is showing.
To see the fault-recovery behavior, just unplug the BME280's SDA or VCC wire 
while it's running — the display should drop to an error page within a couple of sample intervals,
and pick back up on its own once you reconnect it.



## Architecture

```
pico-weather-station/
├── crates/station-core/    # no_std-compatible, zero HAL deps
│   └── src/lib.rs          #   dew_point_c(), sea_level_pressure() — host-tested
└── firmware/
    └── src/
    │   ├── main.rs         # wiring: shared I2C bus, page state machine, error handling
    └── examples/
        └── i2c_scan.rs # bus scanner — your diagnostic tool if a device goes missing
```

`station-core` holds the two pieces of real math this project needs (dew point via the Magnus formula, sea-level-adjusted pressure) — both pure functions, both tested against known reference values on the host, same pattern as `pot-core`'s `raw_to_percent`. Everything involving the shared I²C bus, sensor error handling, and page rendering stays in `firmware`, since none of it can be meaningfully tested without real hardware.


## Testing

```bash
cargo test -p station-core --target x86_64-unknown-linux-gnu
```

`firmware/` has no host tests — the shared-bus behavior and fault recovery
specifically need real hardware (and a real fault, i.e. an unplugged wire) to 
verify.

But if you want to test and scan the i2c bus:

Run the i2c shell script in another terminal to catch then:
```bash
cd firmware
cargo run --example i2c_scan
```

## Known limitations

- The BME280 must be powered from 3.3 V only; never feed it from 5 V.
- The I2C bus is shared and can become flaky with long leads or poor grounding.
- The project intentionally avoids SWD probes; serial logs and the bootloader flow are the debugging path.
- The OLED is best updated at a moderate refresh rate so the bus and CPU remain responsive.
- Sensor faults are handled gracefully, but the application is still a prototype and not production-hardened.

## License

This project uses the standard Rust dual-license approach:

- MIT
- Apache 2.0

## Acknowledgements

- Embedded Rust on the RP2040 roadmap (USB-only workflow)
- Raspberry Pi Pico W documentation and RP2040 datasheet
- BME280 datasheet and reference calculations
- `embedded-graphics` and `rp2040-hal` examples for OLED and GPIO setup
