use alloc::boxed::Box;
use defmt::info;
use embedded_graphics::Drawable;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::iso_8859_1::FONT_8X13;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::{Point, RgbColor};
use embedded_graphics::text::Text;
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::delay::Delay;
use esp_hal::gpio::DriveMode;
use esp_hal::peripherals;
use esp_hal::{
    Blocking,
    dma::{DmaRxBuf, DmaTxBuf},
    dma_buffers,
    gpio::{Level, Output, OutputConfig},
    spi::master::{Spi, SpiDmaBus},
    time::Rate,
};
use mipidsi::Builder;
use mipidsi::interface::SpiInterface;
use mipidsi::models::ILI9342CRgb565;
use mipidsi::options::ColorOrder;

// Use SPI with direct memory access, based on https://github.com/georgik/esp32-conways-game-of-life-rs/blob/main/esp32-s3-box-3/src/main.rs
type MyDisplay = mipidsi::Display<
    SpiInterface<
        'static,
        ExclusiveDevice<SpiDmaBus<'static, Blocking>, Output<'static>, Delay>,
        Output<'static>,
    >,
    ILI9342CRgb565,
    Output<'static>,
>;

const CONSOLE_X: i32 = 10;
const CONSOLE_LINE_HEIGHT: i32 = 15;
const CONSOLE_INITIAL_Y: i32 = 20;

pub struct Display {
    display: MyDisplay,
    console_y: i32,
}

impl Display {
    #[allow(clippy::too_many_arguments)]
    /// Initialize the display, needs all peripherals as seperate arguements since [`Storage`](crate::storage::Storage) and [`Wifi`](crate::wifi::Wifi) also rely on peripherals.
    pub fn new(
        spi2: peripherals::SPI2<'static>,
        dma_ch0: peripherals::DMA_CH0<'static>,
        gpio4: peripherals::GPIO4<'static>,
        gpio5: peripherals::GPIO5<'static>,
        gpio6: peripherals::GPIO6<'static>,
        gpio7: peripherals::GPIO7<'static>,
        gpio47: peripherals::GPIO47<'static>,
        gpio48: peripherals::GPIO48<'static>,
    ) -> Self {
        // direct memory access (dma) for SPI transfers
        let (rx_buffer, rx_descriptors, tx_buffer, tx_descriptors) = dma_buffers!(8912);
        let dma_rx_buf = DmaRxBuf::new(rx_descriptors, rx_buffer).unwrap();
        let dma_tx_buf = DmaTxBuf::new(tx_descriptors, tx_buffer).unwrap();

        // create SPI interface with DMA
        let spi = Spi::<Blocking>::new(
            spi2,
            esp_hal::spi::master::Config::default()
                .with_frequency(Rate::from_mhz(40))
                .with_mode(esp_hal::spi::Mode::_0),
        )
        .unwrap()
        .with_sck(gpio7)
        .with_mosi(gpio6)
        .with_dma(dma_ch0)
        .with_buffers(dma_rx_buf, dma_tx_buf);

        let cs_output = Output::new(gpio5, Level::High, OutputConfig::default());
        let spi_delay = Delay::new();
        let spi_device = ExclusiveDevice::new(spi, cs_output, spi_delay).unwrap();

        let lcd_dc = Output::new(gpio4, Level::Low, OutputConfig::default());
        let buffer: &'static mut [u8; 512] = Box::leak(Box::new([0_u8; 512]));
        let di = SpiInterface::new(spi_device, lcd_dc, buffer);

        // from https://github.com/georgik/esp32-conways-game-of-life-rs/blob/main/esp32-s3-box-3/src/main.rs
        // needs .with_drive_mode(DriveMode::OpenDrain) for some reason
        let reset = Output::new(
            gpio48,
            Level::High,
            OutputConfig::default().with_drive_mode(DriveMode::OpenDrain),
        );

        let mut display_delay = Delay::new();

        // create display driver
        let mut display: MyDisplay = Builder::new(ILI9342CRgb565, di)
            .reset_pin(reset)
            .display_size(320, 240)
            .color_order(ColorOrder::Bgr)
            .orientation(
                mipidsi::options::Orientation::new()
                    .flip_vertical()
                    .flip_horizontal(),
            )
            // .invert_colors(ColorInversion::Inverted)
            .init(&mut display_delay)
            .unwrap();

        // clear to black
        display.clear(Rgb565::BLACK).unwrap();

        // enable backlight
        let mut backlight = Output::new(gpio47, Level::High, OutputConfig::default());
        backlight.set_high();

        info!("Display initialized successfully");

        Self {
            display,
            console_y: CONSOLE_INITIAL_Y,
        }
    }

    pub fn display_mut(&mut self) -> &mut MyDisplay {
        &mut self.display
    }

    pub fn console_println(&mut self, text: &str) {
        Text::new(
            text,
            Point::new(CONSOLE_X, self.console_y),
            MonoTextStyle::new(&FONT_8X13, Rgb565::WHITE),
        )
        .draw(&mut self.display)
        .unwrap();
        self.console_y += CONSOLE_LINE_HEIGHT;
    }
}
