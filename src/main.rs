#![no_std]
#![no_main]

use ch32_hal::gpio;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_time::Duration;
use embassy_usb::Builder;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::class::hid::{self, HidBootProtocol, HidReaderWriter, HidSubclass};
use embassy_usb::driver::EndpointError;
use embassy_usb_dfu::application;
use embassy_usb_dfu::consts::DfuAttributes;
use hal::bind_interrupts;
use hal::usbd;
use {ch32_hal as hal, panic_halt as _};

bind_interrupts!(struct Irqs {
    USB_LP_CAN1_RX0 => hal::usbd::InterruptHandler<hal::peripherals::USBD>;
});

#[embassy_executor::main(entry = "qingke_rt::entry")]
async fn main(_spawner: Spawner) {
    let p = hal::init(hal::Config {
        rcc: hal::rcc::Config::SYSCLK_FREQ_144MHZ_HSI,
        ..Default::default()
    });

    let led = gpio::Output::new(p.PC13, gpio::Level::Low, gpio::Speed::default());

    // USB device
    let driver = usbd::Driver::new(p.USBD, Irqs, p.PA12, p.PA11);

    let mut config = embassy_usb::Config::new(0xC0DE, 0xCAFE);
    config.manufacturer = Some("JussyDr");
    config.product = Some("HID Remapper");
    config.serial_number = Some("12345678");
    config.max_power = 100;
    config.max_packet_size_0 = 64;

    config.device_class = 0xEF;
    config.device_sub_class = 0x02;
    config.device_protocol = 0x01;
    config.composite_with_iads = true;

    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    let mut control_buf = [0; 64];

    let mut dfu_state = application::DfuState::new(
        ApplicationHandler { led },
        DfuAttributes::CAN_DOWNLOAD | DfuAttributes::WILL_DETACH,
        Duration::MAX,
    );

    let mut cdc_acm_state = State::new();

    let mut hid_state = hid::State::new();

    let mut builder = Builder::new(
        driver,
        config,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut [],
        &mut control_buf,
    );

    application::usb_dfu(&mut builder, &mut dfu_state, |_| {});

    let mut cdc_acm_class = CdcAcmClass::new(&mut builder, &mut cdc_acm_state, 64);

    let hid_class = HidReaderWriter::<_, 0, 0>::new(
        &mut builder,
        &mut hid_state,
        hid::Config {
            report_descriptor: &[],
            request_handler: None,
            poll_ms: 10,
            max_packet_size: 0,
            hid_subclass: HidSubclass::No,
            hid_boot_protocol: HidBootProtocol::None,
        },
    );

    let mut usb_device = builder.build();

    let usb_fut = usb_device.run();

    let echo_fut = async {
        loop {
            cdc_acm_class.wait_connection().await;
            let _ = echo(&mut cdc_acm_class).await;
        }
    };

    join(usb_fut, echo_fut).await;
}

struct ApplicationHandler<'d> {
    led: gpio::Output<'d>,
}

impl application::Handler for ApplicationHandler<'_> {
    fn enter_dfu(&mut self) {
        self.led.set_high();
    }
}

struct Disconnected {}

impl From<EndpointError> for Disconnected {
    fn from(val: EndpointError) -> Self {
        match val {
            EndpointError::BufferOverflow => panic!("Buffer overflow"),
            EndpointError::Disabled => Disconnected {},
        }
    }
}

async fn echo<'d, T: usbd::Instance + 'd>(
    class: &mut CdcAcmClass<'d, usbd::Driver<'d, T>>,
) -> Result<(), Disconnected> {
    let mut buf = [0; 64];
    loop {
        class.read_packet(&mut buf).await?;
        class.write_packet(b"I always say kaas :D\r\n").await?;
    }
}
