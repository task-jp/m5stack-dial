#![no_std]
#![no_main]

use esp32s3_hal::{
    clock::{ClockControl, CpuClock},
    peripherals::Peripherals,
    prelude::*,
    spi::{Spi, SpiMode},
    Delay, IO,
};
use esp_backtrace as _;
use esp_println::println;

use embedded_graphics::{
    prelude::*,
    primitives::{Primitive, PrimitiveStyleBuilder, Rectangle},
    Drawable,
};

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
    #[cfg(feature = "samjkent-gc9a01")]
    display.clear(Rgb565::BLUE).unwrap();

    let style = PrimitiveStyleBuilder::new()
        .stroke_width(4)
        .stroke_color(embedded_graphics::prelude::RgbColor::BLACK)
        .fill_color(embedded_graphics::prelude::RgbColor::RED)
        .build();
    Rectangle::new(Point::new(100, 100), Size::new(40u32, 40u32))
        .into_styled(style)
        .draw(&mut display)
        .unwrap();
    loop {
        println!("loop");
        delay.delay_ms(500u32);
    }
}
