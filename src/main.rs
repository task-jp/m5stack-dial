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

    let style = PrimitiveStyleBuilder::new()
        .stroke_width(4)
        .stroke_color(embedded_graphics::prelude::RgbColor::BLACK)
        .fill_color(embedded_graphics::prelude::RgbColor::RED)
        .build();

    let mut last_value = 0;

    loop {
        #[cfg(feature = "dial")]
        critical_section::with(|cs| {
            let mut u0 = UNIT0.borrow_ref_mut(cs);
            let u0 = u0.as_mut().unwrap();
            let mut value: i32 = u0.get_value() as i32 + VALUE.load(Ordering::SeqCst);
            while (value < 0) {
                value += 360;
            }
            if value != last_value {
                println!("value: {value}");
                last_value = value;
            }
        });
        display.clear(Rgb565::BLUE).unwrap();
        let diameter: i32 = 40;
        let angle = <i32 as Into<f64>>::into(last_value % 360) * PI / 180.0 * 2.0;
        let x = 120 as f64 + angle.cos() * 100 as f64 - diameter as f64 / 2.0;
        let y = 120 as f64 + angle.sin() * 100 as f64 - diameter as f64 / 2.0;
        println!("angle: {}, x: {}, y: {}", last_value % 360, x, y);

        Circle::new(Point::new(x as i32, y as i32), 10)
            .into_styled(style)
            .draw(&mut display)
            .unwrap();
        delay.delay_ms(500u32);
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
