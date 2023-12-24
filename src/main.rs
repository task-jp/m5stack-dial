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
    primitives::{Rectangle, Primitive, PrimitiveStyleBuilder},
    Drawable,
};

use embedded_graphics_core::{
    pixelcolor::Rgb565,
    draw_target::DrawTarget,
    pixelcolor::RgbColor,
};
use display_interface_spi::SPIInterface;

#[cfg(feature = "kaizensparc-gc9a01-rs")]
mod gc9a01;
#[cfg(feature = "kaizensparc-gc9a01-rs")]
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
    bl.set_high().unwrap();

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
