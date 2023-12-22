use defmt::info;
use embassy_rp::{
    gpio::{AnyPin, Level, Output},
    peripherals::PIN_16,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, channel::Receiver};
use embassy_time::{with_timeout, Duration};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
pub enum IndicatorStatus {
    PowerOn,
    WifiConnecting,
    WifiConnected,
    ServerConnecting,
    ServerConnected,
    EnterScreen,
    LeaveScreen,
    ServerDisconnected,
}

fn get_duty_cycle(status: IndicatorStatus) -> (u64, u64) {
    match status {
        IndicatorStatus::PowerOn => (50, 50),
        IndicatorStatus::WifiConnecting => (50, 50),
        IndicatorStatus::WifiConnected => (100, 100),
        IndicatorStatus::ServerConnecting => (100, 100),
        IndicatorStatus::ServerConnected => (500, 500),
        IndicatorStatus::EnterScreen => (1000, 0),
        IndicatorStatus::LeaveScreen => (500, 500),
        IndicatorStatus::ServerDisconnected => (100, 100),
    }
}

#[embassy_executor::task]
pub async fn indicator_task(
    receiver: Receiver<'static, NoopRawMutex, IndicatorStatus, 4>,
    led: AnyPin,
) {
    let mut led = Output::new(led, Level::Low);
    let mut current_status = IndicatorStatus::PowerOn;
    let mut led_on = true;
    loop {
        let (on, off) = get_duty_cycle(current_status);
        match current_status {
            IndicatorStatus::EnterScreen => {
                info!("Enable power saving");
            }
            IndicatorStatus::LeaveScreen => {
                info!("Disable power saving");
            }
            _ => {}
        }
        let next_period = Duration::from_millis(if led_on { on } else { off });
        if next_period == Duration::from_millis(0) {
            led_on = !led_on;
            continue;
        }

        if led_on {
            //info!("LED on");
            led.set_low();
        } else {
            //info!("LED off");
            led.set_high();
        }
        match with_timeout(next_period, receiver.receive()).await {
            Ok(status) => {
                info!("Got status: {:?}", status);
                current_status = status;
            }
            Err(_) => {
                led_on = !led_on;
            }
        }
    }
}
