#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![allow(async_fn_in_trait)]

use const_env::from_env;
use core::sync::atomic::{AtomicBool, Ordering};
use cyw43::Control;
use cyw43_pio::PioSpi;
use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::join::join5;
use embassy_net::tcp::TcpSocket;
use embassy_net::{Config, IpEndpoint, Ipv4Address, Stack, StackResources};
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH0, PIN_23, PIN_25, PIO0, USB};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::watchdog::Watchdog;
use embassy_sync::channel::Sender;
use embassy_sync::mutex::Mutex;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, channel};
use embassy_time::{with_timeout, Duration, Timer};
use embassy_usb::class::hid::{HidReaderWriter, ReportId, RequestHandler};
use embassy_usb::control::OutResponse;
use indicator::IndicatorStatus;
use static_cell::make_static;

use crate::synergy_hid::SynergyHid;
use {defmt_rtt as _, panic_probe as _};

mod barrier;
mod indicator;
mod synergy_hid;
mod usb_actuator;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<USB>;
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

const WIFI_NETWORK: &str = env!("WIFI_NETWORK");
const WIFI_PASSWORD: &str = env!("WIFI_PASSWORD");
const SCREEN_NAME: &str = env!("SCREEN_NAME");
const SERVER_ENDPOINT: &str = env!("SERVER_ENDPOINT");
#[from_env]
const SCREEN_WIDTH: u16 = 1920;
#[from_env]
const SCREEN_HEIGHT: u16 = 1080;
#[from_env]
const FLIP_MOUSE_WHEEL: bool = false;
#[from_env]
const WATCHDOG_INTERVAL: u64 = 8;

fn parse_addr(s: &str) -> Ipv4Address {
    let mut parts = s.split('.');
    let a = parts.next().expect("invalid ip address");
    let b = parts.next().expect("invalid ip address");
    let c = parts.next().expect("invalid ip address");
    let d = parts.next().expect("invalid ip address");
    let a = a.parse().expect("invalid ip address");
    let b = b.parse().expect("invalid ip address");
    let c = c.parse().expect("invalid ip address");
    let d = d.parse().expect("invalid ip address");
    Ipv4Address::new(a, b, c, d)
}

fn parse_endpoint(s: &str) -> IpEndpoint {
    let mut parts = s.split(':');
    let ip = parts.next().expect("invalid ip endpoint");
    let port = parts.next().expect("invalid ip endpoint");
    let ip = parse_addr(ip);
    let port = port.parse().expect("invalid port");
    IpEndpoint::from((ip, port))
}

#[embassy_executor::task]
async fn wifi_task(
    runner: cyw43::Runner<
        'static,
        Output<'static, PIN_23>,
        PioSpi<'static, PIN_25, PIO0, 0, DMA_CH0>,
    >,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(stack: &'static Stack<cyw43::NetDriver<'static>>) -> ! {
    stack.run().await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let driver = embassy_rp::usb::Driver::new(p.USB, Irqs);
    let mut watchdog = Watchdog::new(p.WATCHDOG);
    watchdog.start(Duration::from_secs(WATCHDOG_INTERVAL));

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
        poll_ms: 10,
        max_packet_size: 64,
    };
    let keyboard = HidReaderWriter::<_, 1, 8>::new(&mut builder, &mut keyboard_state, config);
    let config = embassy_usb::class::hid::Config {
        report_descriptor: SynergyHid::get_report_descriptor(synergy_hid::ReportType::Mouse).1,
        request_handler: Some(&request_handler),
        poll_ms: 5,
        max_packet_size: 64,
    };
    let mouse = HidReaderWriter::<_, 1, 8>::new(&mut builder, &mut mouse_state, config);
    let config = embassy_usb::class::hid::Config {
        report_descriptor: SynergyHid::get_report_descriptor(synergy_hid::ReportType::Consumer).1,
        request_handler: Some(&request_handler),
        poll_ms: 10,
        max_packet_size: 64,
    };
    let consumer = HidReaderWriter::<_, 1, 8>::new(&mut builder, &mut consumer_state, config);

    // Build the builder.
    let mut usb = builder.build();

    // Run the USB device.
    let usb_fut = usb.run();

    // To make flashing faster for development, you may want to flash the firmwares independently
    // at hardcoded addresses, instead of baking them into the program with `include_bytes!`:
    //     probe-rs download 43439A0.bin --format bin --chip RP2040 --base-address 0x10100000
    //     probe-rs download 43439A0_clm.bin --format bin --chip RP2040 --base-address 0x10140000
    let fw = if cfg!(debug_assertions) {
        unsafe { core::slice::from_raw_parts(0x10100000 as *const u8, 230321) }
    } else {
        include_bytes!("../firmware/43439A0.bin")
    };
    let clm = if cfg!(debug_assertions) {
        unsafe { core::slice::from_raw_parts(0x10140000 as *const u8, 4752) }
    } else {
        include_bytes!("../firmware/43439A0_clm.bin")
    };

    let pwr = Output::new(p.PIN_23, Level::Low);
    let cs = Output::new(p.PIN_25, Level::High);
    let mut pio = Pio::new(p.PIO0, Irqs);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        pio.irq0,
        cs,
        p.PIN_24,
        p.PIN_29,
        p.DMA_CH0,
    );

    let state: &'static mut cyw43::State = make_static!(cyw43::State::new());
    let (net_device, control, runner) = cyw43::new(state, pwr, spi, fw).await;
    unwrap!(spawner.spawn(wifi_task(runner)));

    let control: &'static Mutex<NoopRawMutex, Control<'static>> =
        make_static!(Mutex::<NoopRawMutex, Control<'static>>::new(control));

    control.lock().await.init(clm).await;

    unwrap!(spawner.spawn(indicator::indicator_task(
        indicator_channel.receiver(),
        control
    )));

    control
        .lock()
        .await
        .set_power_management(cyw43::PowerManagementMode::None)
        .await;

    let config = Config::dhcpv4(Default::default());

    // Generate random seed
    let seed = 0x0123_4567_89ab_cafe; // chosen by fair dice roll. guaranteed to be random.

    // Init network stack
    let stack = &*make_static!(Stack::new(
        net_device,
        config,
        make_static!(StackResources::<2>::new()),
        seed
    ));

    unwrap!(spawner.spawn(net_task(stack)));

    loop {
        match with_timeout(
            Duration::from_secs(10),
            control.lock().await.join_wpa2(WIFI_NETWORK, WIFI_PASSWORD),
        )
        .await
        {
            Ok(Ok(())) => {
                info!("join successful");
                // In case the join was successful, feed the watchdog
                watchdog.feed();
                break;
            }
            Ok(Err(err)) => {
                info!("join failed with status={}", err.status);
            }
            Err(_) => {
                info!("join timed out");
            }
        }
        info!("retrying in 1s...");
        Timer::after(Duration::from_secs(1)).await;
    }

    // Wait for DHCP, not necessary when using static IP
    info!("waiting for DHCP...");
    while !stack.is_config_up() {
        Timer::after_millis(100).await;
    }
    info!("DHCP is now up!");

    // DHCP could be slow in some cases, so we feed the watchdog
    watchdog.feed();

    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];

    let (keyboard_reader, keyboard_writer) = keyboard.split();

    let (mouse_reader, mouse_writer) = mouse.split();

    let (consumer_reader, consumer_writer) = consumer.split();

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
        let mut actuator = usb_actuator::UsbActuator::new(
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
            FLIP_MOUSE_WHEEL,
            sender,
            keyboard_writer,
            mouse_writer,
            consumer_writer,
        );
        loop {
            let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
            socket.set_timeout(Some(Duration::from_secs(10)));

            let remote_endpoint = parse_endpoint(SERVER_ENDPOINT);
            info!("Connecting...");
            let r = socket.connect(remote_endpoint).await;
            if let Err(e) = r {
                warn!("connect error: {:?}", e);
                continue;
            }
            info!("Connected!");
            barrier::start(socket, SCREEN_NAME, &mut actuator, &mut watchdog)
                .await
                .ok();
            Timer::after(Duration::from_secs(1)).await;
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
