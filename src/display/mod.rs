use alloc::boxed::Box;
use defmt::info;
use esp_hal::peripherals;
use esp_hal::delay::Delay;
use esp_hal::{
    dma_buffers,
    Blocking,
    gpio::{Level, Output, OutputConfig},
    dma::{DmaRxBuf, DmaTxBuf},
    spi::master::{Spi, SpiDmaBus},
    time::Rate,
};
use mipidsi::interface::SpiInterface;
use mipidsi::options::{ColorInversion, ColorOrder};
use mipidsi::{models::ILI9342CRgb565, Builder};
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::prelude::RgbColor;

// Use the DMA-enabled SPI bus type.
type MyDisplay = mipidsi::Display<
    SpiInterface<
        'static,
        ExclusiveDevice<SpiDmaBus<'static, Blocking>, Output<'static>, Delay>,
        Output<'static>,
    >,
    ILI9342CRgb565,
    Output<'static>,
>;

pub struct Display {
    display: MyDisplay
}


impl Display {
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
        // display_delay.delay_nanos(500_000u32);

        info!("Initializing buffer...");
        // direct memory access (dma) for SPI transfers
        let (rx_buffer, rx_descriptors, tx_buffer, tx_descriptors) = dma_buffers!(8912);
        let dma_rx_buf = DmaRxBuf::new(rx_descriptors, rx_buffer).unwrap();
        let dma_tx_buf = DmaTxBuf::new(tx_descriptors, tx_buffer).unwrap();

        info!("Initializing SPI...");
        // display setup, static since only the esp32s3 is supported
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

        let reset = Output::new(gpio48, Level::High, OutputConfig::default());

        let mut display_delay = Delay::new();

        info!("Initializing display...");
        let mut display: MyDisplay = Builder::new(ILI9342CRgb565, di)
            .reset_pin(reset)
            .display_size(320, 240)
            .color_order(ColorOrder::Bgr)
            .orientation(mipidsi::options::Orientation::new()
                .flip_vertical()
                .flip_horizontal()
            ,)
            // .invert_colors(ColorInversion::Inverted)
            .init(&mut display_delay)
            .unwrap();

        // Clear to black screen
        display.clear(Rgb565::BLACK).unwrap();

        // Enable backlight
        let mut backlight = Output::new(gpio47, Level::Low, OutputConfig::default());
        backlight.set_high();

        Self { display }
    }

    pub fn display_mut(&mut self) -> &mut MyDisplay {
        &mut self.display
    }
}