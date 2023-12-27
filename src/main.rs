#![no_std]
#![no_main]

use core::{
    cell::RefCell,
    cmp::min,
    f64::consts::PI,
    sync::atomic::{AtomicI32, Ordering},
};

use esp32s3_hal::{
    clock::{ClockControl, CpuClock},
    interrupt,
    pcnt::{
        channel::{self, PcntSource},
        unit::{self, Unit},
    },
    peripherals::{self, Peripherals},
    prelude::*,
    spi::{Spi, SpiMode},
    Delay, IO,
};
use esp_backtrace as _;
use esp_println::println;

use embedded_graphics::{
    prelude::*,
    primitives::{Circle, Primitive, PrimitiveStyleBuilder, Rectangle},
    Drawable,
};

#[cfg(feature = "dial")]
use critical_section::Mutex;

use display_interface_spi::SPIInterface;
use embedded_graphics_core::{draw_target::DrawTarget, pixelcolor::Rgb565, pixelcolor::RgbColor};

#[cfg(feature = "kaizensparc-gc9a01-rs")]
mod kaizensparc_gc9a01_rs;
#[cfg(feature = "kaizensparc-gc9a01-rs")]
use kaizensparc_gc9a01_rs as gc9a01;

#[cfg(feature = "samjkent-gc9a01")]
mod samjkent_gc9a01;
#[cfg(feature = "samjkent-gc9a01")]
use samjkent_gc9a01 as gc9a01;

use gc9a01::*;

#[cfg(feature = "dial")]
use esp32s3_hal::pcnt::PCNT;

use num_traits::real::Real;

#[cfg(feature = "dial")]
static UNIT0: Mutex<RefCell<Option<Unit>>> = Mutex::new(RefCell::new(None));
#[cfg(feature = "dial")]
static VALUE: AtomicI32 = AtomicI32::new(0);

#[entry]
fn main() -> ! {
    let peripherals = Peripherals::take();
    let mut system = peripherals.SYSTEM.split();
    let clocks = ClockControl::configure(system.clock_control, CpuClock::Clock240MHz).freeze();

    let mut delay = Delay::new(&clocks);
    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);

    let sclk = io.pins.gpio6;
    let mosi = io.pins.gpio5;

    let spi = Spi::new_no_cs_no_miso(
        peripherals.SPI2,
        sclk,
        mosi,
        60u32.MHz(),
        SpiMode::Mode0,
        &mut system.peripheral_clock_control,
        &clocks,
    );

    let dc = io.pins.gpio4.into_push_pull_output();
    let cs = io.pins.gpio7.into_push_pull_output();
    let rst = io.pins.gpio8.into_push_pull_output();
    let mut bl = io.pins.gpio9.into_push_pull_output();
    let _ = bl.set_high().unwrap();

    #[cfg(feature = "kaizensparc-gc9a01-rs")]
    let iface = SPIInterface::new(spi, dc, cs);
    #[cfg(feature = "kaizensparc-gc9a01-rs")]
    let mut display = GC9A01::new(
        iface,
        rst,
        &mut delay,
        gc9a01::Orientation::Landscape,
        gc9a01::DisplaySize240x240,
    )
    .unwrap();
    #[cfg(feature = "kaizensparc-gc9a01-rs")]
    display.clear(Rgb565::BLUE).unwrap();

    #[cfg(feature = "IniterWorker-gc9a01-rs")]
    let iface = SPIInterface::new(spi, dc, cs);
    #[cfg(feature = "IniterWorker-gc9a01-rs")]
    let display = Gc9a01::new(
        iface,
        prelude::DisplayResolution240x240,
        prelude::DisplayRotation::Rotate0,
    );
    #[cfg(feature = "IniterWorker-gc9a01-rs")]
    let mut display = display.into_buffered_graphics(); // never returns

    #[cfg(feature = "samjkent-gc9a01")]
    let mut display = GC9A01::default(spi, cs, dc).unwrap();
    #[cfg(feature = "samjkent-gc9a01")]
    display.setup();

    #[cfg(feature = "dial")]
    {
        let mut mtdo = io.pins.gpio40.into_pull_up_input();
        let mut mtdi = io.pins.gpio41.into_pull_up_input();
        let pcnt = PCNT::new(peripherals.PCNT, &mut system.peripheral_clock_control);
        let mut u0: unit::Unit = pcnt.get_unit(unit::Number::Unit1);
        u0.configure(unit::Config {
            low_limit: -100,
            high_limit: 100,
            filter: Some(min(10u16 * 80, 1023u16)),
            ..Default::default()
        })
        .unwrap();
        let mut ch0 = u0.get_channel(channel::Number::Channel0);
        ch0.configure(
            PcntSource::from_pin(&mut mtdi),
            PcntSource::from_pin(&mut mtdo),
            channel::Config {
                lctrl_mode: channel::CtrlMode::Reverse,
                hctrl_mode: channel::CtrlMode::Keep,
                pos_edge: channel::EdgeMode::Decrement,
                neg_edge: channel::EdgeMode::Increment,
                invert_ctrl: false,
                invert_sig: false,
            },
        );
        let mut ch1 = u0.get_channel(channel::Number::Channel1);
        ch1.configure(
            PcntSource::from_pin(&mut mtdo),
            PcntSource::from_pin(&mut mtdi),
            channel::Config {
                lctrl_mode: channel::CtrlMode::Reverse,
                hctrl_mode: channel::CtrlMode::Keep,
                pos_edge: channel::EdgeMode::Increment,
                neg_edge: channel::EdgeMode::Decrement,
                invert_ctrl: false,
                invert_sig: false,
            },
        );
        u0.events(unit::Events {
            low_limit: true,
            high_limit: true,
            thresh0: false,
            thresh1: false,
            zero: false,
        });
        u0.listen();
        u0.resume();

        critical_section::with(|cs| UNIT0.borrow_ref_mut(cs).replace(u0));

        interrupt::enable(peripherals::Interrupt::PCNT, interrupt::Priority::Priority2).unwrap();
    }

    #[cfg(feature = "button")]
    let mtms = io.pins.gpio42.into_pull_up_input();

    #[cfg(feature = "touch")]
    let i2c = esp32s3_hal::i2c::I2C::new(
        peripherals.I2C0,
        io.pins.gpio11, // tp sda
        io.pins.gpio12, // tp scl
        esp32s3_hal::prelude::_fugit_RateExtU32::kHz(400),
        &mut system.peripheral_clock_control,
        &clocks,
    );

    #[cfg(feature = "touch")]
    let mut touch = ft3267::FT3267::new(i2c);

    let dial_button_style = PrimitiveStyleBuilder::new()
        .stroke_width(4)
        .stroke_color(embedded_graphics::prelude::RgbColor::YELLOW)
        .fill_color(embedded_graphics::prelude::RgbColor::BLACK)
        .build();
    let touch_style = PrimitiveStyleBuilder::new()
        .stroke_width(2)
        .stroke_color(embedded_graphics::prelude::RgbColor::WHITE)
        .fill_color(embedded_graphics::prelude::RgbColor::RED)
        .build();

    let mut last_value = 0;
    let mut last_pressed = false;
    let mut last_touch: [Option<(u16, u16)>; 2] = [None, None];

    let mut first = true;
    loop {
        let mut changed = first;
        first = false;

        #[cfg(feature = "touch")]
        {
            let t = touch.touch();
            if t != last_touch {
                last_touch = t;
                changed = true;
            }
        }
        #[cfg(feature = "dial")]
        critical_section::with(|cs| {
            let mut u0 = UNIT0.borrow_ref_mut(cs);
            let u0 = u0.as_mut().unwrap();
            let mut value: i32 = u0.get_value() as i32 + VALUE.load(Ordering::SeqCst);
            // while value < 0 {
            //     value += 64;
            // }
            value %= 128;
            value = value * 360 / 128;
            if value != last_value {
                println!("value: {value}");
                last_value = value;
                changed = true;
            }
        });
        #[cfg(feature = "button")]
        {
            let pressed = mtms.is_low().unwrap();
            if pressed != last_pressed {
                last_pressed = pressed;
                changed = true;
            }
            // println!("button: {}", mtms.is_low().unwrap());
        }

        if changed {
            Rectangle::new(Point::new(0, 0), Size::new(240, 240))
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(Rgb565::BLUE)
                        .build(),
                )
                .draw(&mut display)
                .unwrap();
            let diameter: i32 = match last_pressed {
                true => 20,
                false => 40,
            };
            let angle = <i32 as Into<f64>>::into(last_value) * PI / 180.0 * 2.0;
            let x = 120 as f64 + angle.cos() * 100 as f64 - diameter as f64 / 2.0;
            let y = 120 as f64 + angle.sin() * 100 as f64 - diameter as f64 / 2.0;
            Circle::new(Point::new(x as i32, y as i32), diameter as u32)
                .into_styled(dial_button_style)
                .draw(&mut display)
                .unwrap();

            match last_touch {
                [Some((x1, y1)), None] => {
                    Circle::with_center(Point::new(y1 as i32, 240 - x1 as i32), 60 as u32)
                        .into_styled(touch_style)
                        .draw(&mut display)
                        .unwrap();
                }
                [Some((x1, y1)), Some((x2, y2))] => {
                    Circle::with_center(Point::new(y1 as i32, 240 - x1 as i32), 60 as u32)
                        .into_styled(touch_style)
                        .draw(&mut display)
                        .unwrap();
                    Circle::with_center(Point::new(y2 as i32, 240 - x2 as i32), 60 as u32)
                        .into_styled(touch_style)
                        .draw(&mut display)
                        .unwrap();
                }
                _ => {}
            }
        }

        // delay.delay_ms(500u32);
        delay.delay_ms(32u32); // 30fps
    }
}

#[cfg(feature = "dial")]
#[interrupt]
fn PCNT() {
    critical_section::with(|cs| {
        let mut u0 = UNIT0.borrow_ref_mut(cs);
        let u0 = u0.as_mut().unwrap();
        if u0.interrupt_set() {
            let events = u0.get_events();
            if events.high_limit {
                VALUE.fetch_add(100, Ordering::SeqCst);
            } else if events.low_limit {
                VALUE.fetch_add(-100, Ordering::SeqCst);
            }
            println!("VALUE: {}", VALUE.load(Ordering::SeqCst));
            u0.reset_interrupt();
        }
    });
}

#[cfg(feature = "touch")]
mod ft3267 {
    use embedded_hal::blocking::i2c::{Write, WriteRead};

    pub struct FT3267<I2C> {
        i2c: I2C,
        address: u8,
    }

    impl<I2C, E> FT3267<I2C>
    where
        I2C: WriteRead<Error = E> + Write<Error = E>,
    {
        pub fn new(i2c: I2C) -> Self {
            Self { i2c, address: 0x38 }
        }

        pub fn touch(&mut self) -> [Option<(u16, u16)>; 2] {
            const FT_TP_STATUS: usize = 0x02;
            const FT_TP1_XH: usize = 0x03;
            const FT_TP1_XL: usize = 0x04;
            const FT_TP1_YH: usize = 0x05;
            const FT_TP1_YL: usize = 0x06;
            const FT_TP2_XH: usize = 0x09;
            const FT_TP2_XL: usize = 0x0a;
            const FT_TP2_YH: usize = 0x0b;
            const FT_TP2_YL: usize = 0x0c;

            let mut data: [u8; 13] = [0; 13];
            for i in 0..13 {
                if let Ok(d) = self.read(i) {
                    data[i as usize] = d;
                }
            }
            let count = data[FT_TP_STATUS];
            let mut points: [Option<(u16, u16)>; 2] = [None, None];
            if count > 0 {
                let x1 = ((data[FT_TP1_XH] as u16 & 0x0F) << 8) | (data[FT_TP1_XL] as u16);
                let y1 = ((data[FT_TP1_YH] as u16 & 0x0F) << 8) | (data[FT_TP1_YL] as u16);
                points[0] = Some((x1, y1));
            }
            if count > 1 {
                let x2 = ((data[FT_TP2_XH] as u16 & 0x0F) << 8) | (data[FT_TP2_XL] as u16);
                let y2 = ((data[FT_TP2_YH] as u16 & 0x0F) << 8) | (data[FT_TP2_YL] as u16);
                points[1] = Some((x2, y2));
            }
            points
        }

        fn read(&mut self, register: u8) -> Result<u8, E> {
            let mut data = [0];
            self.i2c
                .write_read(self.address, &[register], &mut data)
                .map(|_| data[0])
        }
    }
}
