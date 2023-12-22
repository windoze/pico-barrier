#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![allow(async_fn_in_trait)]

use core::sync::atomic::{AtomicBool, Ordering};
use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::join::join5;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::Pin;
use embassy_rp::peripherals::{PIO0, USB};
use embassy_rp::pio::InterruptHandler;
use embassy_sync::channel::Sender;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, channel};
use embassy_usb::class::cdc_acm::CdcAcmClass;
use embassy_usb::class::hid::{HidReaderWriter, HidWriter, ReportId, RequestHandler};
use embassy_usb::control::OutResponse;
use embassy_usb::driver::EndpointError;
use indicator::IndicatorStatus;
use static_cell::make_static;

use crate::synergy_hid::SynergyHid;
use {defmt_rtt as _, panic_probe as _};

mod indicator;
mod synergy_hid;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<USB>;
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let driver = embassy_rp::usb::Driver::new(p.USB, Irqs);
    // let mut watchdog = Watchdog::new(p.WATCHDOG);
    // watchdog.start(Duration::from_secs(WATCHDOG_INTERVAL));

    let led = p.PIN_16.degrade();

    let indicator_channel = make_static!(channel::Channel::new());
    let sender: Sender<'_, NoopRawMutex, IndicatorStatus, 4> = indicator_channel.sender();

    // Create embassy-usb Config
    let mut config = embassy_usb::Config::new(0x0d0a, 0xc0de);
    config.manufacturer = Some("0d0a.com");
    config.product = Some("Pico-W Barrier KVM");
    config.serial_number = Some("12345678");
    config.max_power = 150;
    config.max_packet_size_0 = 64;

    // Create embassy-usb DeviceBuilder using the driver and config.
    // It needs some buffers for building the descriptors.
    let mut device_descriptor = [0; 256];
    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    // You can also add a Microsoft OS descriptor.
    let mut msos_descriptor = [0; 256];
    let mut control_buf = [0; 64];
    let request_handler = MyRequestHandler {};
    let mut device_handler = MyDeviceHandler::new();

    let mut keyboard_state = embassy_usb::class::hid::State::new();
    let mut mouse_state = embassy_usb::class::hid::State::new();
    let mut consumer_state = embassy_usb::class::hid::State::new();
    let mut serial_state = embassy_usb::class::cdc_acm::State::new();

    let mut builder = embassy_usb::Builder::new(
        driver,
        config,
        &mut device_descriptor,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut msos_descriptor,
        &mut control_buf,
    );

    builder.handler(&mut device_handler);

    // Create classes on the builder.
    let config = embassy_usb::class::hid::Config {
        report_descriptor: SynergyHid::get_report_descriptor(synergy_hid::ReportType::Keyboard).1,
        request_handler: Some(&request_handler),
        poll_ms: 1,
        max_packet_size: 64,
    };
    let keyboard = HidReaderWriter::<_, 1, 8>::new(&mut builder, &mut keyboard_state, config);
    let config = embassy_usb::class::hid::Config {
        report_descriptor: SynergyHid::get_report_descriptor(synergy_hid::ReportType::Mouse).1,
        request_handler: Some(&request_handler),
        poll_ms: 1,
        max_packet_size: 64,
    };
    let mouse = HidReaderWriter::<_, 1, 7>::new(&mut builder, &mut mouse_state, config);
    let config = embassy_usb::class::hid::Config {
        report_descriptor: SynergyHid::get_report_descriptor(synergy_hid::ReportType::Consumer).1,
        request_handler: Some(&request_handler),
        poll_ms: 1,
        max_packet_size: 64,
    };
    let consumer = HidReaderWriter::<_, 1, 2>::new(&mut builder, &mut consumer_state, config);
    // Create classes on the builder.
    let mut serial = CdcAcmClass::new(&mut builder, &mut serial_state, 64);

    // Build the builder.
    let mut usb = builder.build();

    // Run the USB device.
    let usb_fut = usb.run();

    unwrap!(spawner.spawn(indicator::indicator_task(indicator_channel.receiver(), led)));

    sender.send(IndicatorStatus::WifiConnecting).await;

    let (keyboard_reader, mut keyboard_writer) = keyboard.split();

    let (mouse_reader, mut mouse_writer) = mouse.split();

    let (consumer_reader, mut consumer_writer) = consumer.split();

    let keyboard_out_fut = async {
        keyboard_reader.run(false, &request_handler).await;
    };

    let mouse_out_fut = async {
        mouse_reader.run(false, &request_handler).await;
    };

    let consumer_out_fut = async {
        consumer_reader.run(false, &request_handler).await;
    };

    let in_fut = async {
        loop {
            serial.wait_connection().await;
            info!("Connected");
            indicator_channel
                .send(IndicatorStatus::ServerConnected)
                .await;
            let _ = start(
                &mut serial,
                &mut keyboard_writer,
                &mut mouse_writer,
                &mut consumer_writer,
                sender,
            )
            .await;
            info!("Disconnected");
            indicator_channel
                .send(IndicatorStatus::ServerDisconnected)
                .await;
        }
    };

    join5(
        usb_fut,
        in_fut,
        keyboard_out_fut,
        mouse_out_fut,
        consumer_out_fut,
    )
    .await;
}

struct Disconnected {}

impl From<EndpointError> for Disconnected {
    fn from(val: EndpointError) -> Self {
        warn!("EndpointError: {:?}", val);
        Disconnected {}
        // match val {
        //     EndpointError::BufferOverflow => crate::panic!("Buffer overflow"),
        //     EndpointError::Disabled => Disconnected {},
        // }
    }
}

pub trait ReadMsg {
    async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), EndpointError>;
    async fn read_u8(&mut self) -> Result<u8, EndpointError>;
    async fn read_i8(&mut self) -> Result<i8, EndpointError>;
    async fn read_u16(&mut self) -> Result<u16, EndpointError>;
    async fn read_i16(&mut self) -> Result<i16, EndpointError>;
}

impl<'d, D: embassy_usb::driver::Driver<'d>> ReadMsg for CdcAcmClass<'d, D> {
    async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), EndpointError> {
        let mut n = 0;
        while n < buf.len() {
            let n2 = self.read_packet(&mut buf[n..]).await?;
            n += n2;
        }
        Ok(())
    }

    async fn read_u8(&mut self) -> Result<u8, EndpointError> {
        let mut buf = [0; 1];
        self.read_exact(&mut buf).await?;
        Ok(buf[0])
    }

    async fn read_i8(&mut self) -> Result<i8, EndpointError> {
        let mut buf = [0; 1];
        self.read_exact(&mut buf).await?;
        Ok(buf[0] as i8)
    }

    async fn read_u16(&mut self) -> Result<u16, EndpointError> {
        let mut buf = [0; 2];
        self.read_exact(&mut buf).await?;
        Ok(u16::from_le_bytes(buf))
    }

    async fn read_i16(&mut self) -> Result<i16, EndpointError> {
        let mut buf = [0; 2];
        self.read_exact(&mut buf).await?;
        Ok(i16::from_le_bytes(buf))
    }
}

async fn start<'a, 'b, 'c, 'd, T: embassy_rp::usb::Instance + 'a>(
    class: &mut CdcAcmClass<'a, embassy_rp::usb::Driver<'a, T>>,
    keyboard_writer: &mut HidWriter<
        'b,
        embassy_rp::usb::Driver<'b, embassy_rp::peripherals::USB>,
        8,
    >,
    mouse_writer: &mut HidWriter<'c, embassy_rp::usb::Driver<'c, embassy_rp::peripherals::USB>, 7>,
    consumer_writer: &mut HidWriter<
        'd,
        embassy_rp::usb::Driver<'d, embassy_rp::peripherals::USB>,
        2,
    >,
    sender: Sender<'_, NoopRawMutex, IndicatorStatus, 4>,
) -> Result<(), Disconnected> {
    let mut buf = [0; 9];
    loop {
        class.read_exact(&mut buf).await?;
        match buf[0] {
            0 => {
                let status = match buf[1] {
                    0 => IndicatorStatus::WifiConnecting,
                    1 => IndicatorStatus::WifiConnected,
                    2 => IndicatorStatus::ServerConnecting,
                    3 => IndicatorStatus::ServerConnected,
                    4 => IndicatorStatus::EnterScreen,
                    5 => IndicatorStatus::LeaveScreen,
                    6 => IndicatorStatus::ServerDisconnected,
                    _ => IndicatorStatus::PowerOn,
                };
                sender.send(status).await;
            }
            1 => {
                keyboard_writer.write(&buf[1..9]).await?;
            }
            2 => {
                mouse_writer.write(&buf[1..8]).await?;
            }
            3 => {
                consumer_writer.write(&buf[1..3]).await?;
            }
            _ => {
                warn!("Unknown report id: {}", buf[0]);
            }
        }
    }
}

struct MyRequestHandler {}

impl RequestHandler for MyRequestHandler {
    fn get_report(&self, id: ReportId, _buf: &mut [u8]) -> Option<usize> {
        info!("Get report for {:?}", id);
        None
    }

    fn set_report(&self, id: ReportId, data: &[u8]) -> OutResponse {
        info!("Set report for {:?}: {=[u8]}", id, data);
        OutResponse::Accepted
    }

    fn set_idle_ms(&self, id: Option<ReportId>, dur: u32) {
        info!("Set idle rate for {:?} to {:?}", id, dur);
    }

    fn get_idle_ms(&self, id: Option<ReportId>) -> Option<u32> {
        info!("Get idle rate for {:?}", id);
        None
    }
}

struct MyDeviceHandler {
    configured: AtomicBool,
}

impl MyDeviceHandler {
    fn new() -> Self {
        MyDeviceHandler {
            configured: AtomicBool::new(false),
        }
    }
}

impl embassy_usb::Handler for MyDeviceHandler {
    fn enabled(&mut self, enabled: bool) {
        self.configured.store(false, Ordering::Relaxed);
        info!("Device {}", if enabled { "enabled" } else { "disabled" });
    }

    fn reset(&mut self) {
        self.configured.store(false, Ordering::Relaxed);
        info!("Bus reset, the Vbus current limit is 100mA");
    }

    fn addressed(&mut self, addr: u8) {
        self.configured.store(false, Ordering::Relaxed);
        info!("USB address set to: {}", addr);
    }

    fn configured(&mut self, configured: bool) {
        self.configured.store(configured, Ordering::Relaxed);
        if configured {
            info!(
                "Device configured, it may now draw up to the configured current limit from Vbus."
            )
        } else {
            info!("Device is no longer configured, the Vbus current limit is 100mA.");
        }
    }
}
