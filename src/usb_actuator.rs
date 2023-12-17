use defmt::info;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, channel::Sender};
use embassy_usb::class::hid::HidWriter;

use crate::{
    barrier::Actuator,
    indicator::IndicatorStatus,
    synergy_hid::{ReportType, SynergyHid},
};

pub struct UsbActuator<'a, 'b, 'c, 'd> {
    width: u16,
    height: u16,
    x: u16,
    y: u16,
    hid: SynergyHid,
    sender: Sender<'a, NoopRawMutex, IndicatorStatus, 4>,
    keyboard_writer: HidWriter<'b, embassy_rp::usb::Driver<'b, embassy_rp::peripherals::USB>, 8>,
    mouse_writer: HidWriter<'c, embassy_rp::usb::Driver<'c, embassy_rp::peripherals::USB>, 7>,
    consumer_writer: HidWriter<'d, embassy_rp::usb::Driver<'d, embassy_rp::peripherals::USB>, 2>,
}

impl<'a, 'b, 'c, 'd> UsbActuator<'a, 'b, 'c, 'd> {
    pub fn new(
        width: u16,
        height: u16,
        flip_mouse_wheel: bool,
        sender: Sender<'a, NoopRawMutex, IndicatorStatus, 4>,
        keyboard_writer: HidWriter<
            'b,
            embassy_rp::usb::Driver<'b, embassy_rp::peripherals::USB>,
            8,
        >,
        mouse_writer: HidWriter<'c, embassy_rp::usb::Driver<'c, embassy_rp::peripherals::USB>, 7>,
        consumer_writer: HidWriter<
            'd,
            embassy_rp::usb::Driver<'d, embassy_rp::peripherals::USB>,
            2,
        >,
    ) -> Self {
        Self {
            width,
            height,
            x: 0,
            y: 0,
            hid: SynergyHid::new(flip_mouse_wheel),
            sender,
            keyboard_writer,
            mouse_writer,
            consumer_writer,
        }
    }

    pub async fn send_report(&mut self, report: (ReportType, &[u8])) {
        info!("Sending report: {}, {}", report.0 as u8, report.1);
        match report.0 {
            ReportType::Keyboard => {
                self.keyboard_writer.write(report.1).await.ok();
            }
            ReportType::Mouse => {
                self.mouse_writer.write(report.1).await.ok();
            }
            ReportType::Consumer => {
                self.consumer_writer.write(report.1).await.ok();
            }
        }
    }

    pub(crate) fn scale_position(&self, x: u16, y: u16) -> (u16, u16) {
        // Scale screen position to HID position
        (
            ((x as f32) * (0x7fff as f32) / (self.width as f32)) as u16,
            ((y as f32) * (0x7fff as f32) / (self.height as f32)) as u16,
        )
    }
}

impl<'a, 'b, 'c, 'd> Actuator for UsbActuator<'a, 'b, 'c, 'd> {
    async fn connected(&mut self) {
        info!("Connected to Barrier");
        self.sender.send(IndicatorStatus::ServerConnected).await;
    }

    async fn disconnected(&mut self) {
        info!("Disconnected from Barrier");
        self.sender.send(IndicatorStatus::ServerDisconnected).await;
    }

    async fn get_screen_size(&self) -> (u16, u16) {
        // TODO:
        (self.width, self.height)
    }

    async fn get_cursor_position(&self) -> (u16, u16) {
        (self.x, self.y)
    }

    async fn set_cursor_position(&mut self, x: u16, y: u16) {
        self.x = x;
        self.y = y;
        let (phy_x, phy_y) = self.scale_position(x, y);
        let mut report = [0; 9];
        let ret = self.hid.set_cursor_position(phy_x, phy_y, &mut report);
        self.send_report(ret).await;
    }

    async fn mouse_down(&mut self, button: i8) {
        let mut report = [0; 9];
        let ret = self.hid.mouse_down(button, &mut report);
        self.send_report(ret).await;
    }

    async fn mouse_up(&mut self, button: i8) {
        let mut report = [0; 9];
        let ret = self.hid.mouse_up(button, &mut report);
        self.send_report(ret).await;
    }

    async fn mouse_wheel(&mut self, x: i16, y: i16) {
        let mut report = [0; 9];
        let ret = self.hid.mouse_scroll(x, y, &mut report);
        self.send_report(ret).await;
    }

    async fn key_down(&mut self, key: u16, mask: u16, button: u16) {
        let mut report = [0; 9];
        let ret = self.hid.key_down(key, mask, button, &mut report);
        self.send_report(ret).await;
    }

    async fn key_repeat(&mut self, key: u16, mask: u16, button: u16, count: u16) {
        info!(
            "Key repeat on key: {}, mask: {}, button: {}, count: {}",
            key, mask, button, count
        )
    }

    async fn key_up(&mut self, key: u16, mask: u16, button: u16) {
        let mut report = [0; 9];
        let ret = self.hid.key_up(key, mask, button, &mut report);
        self.send_report(ret).await;
    }

    async fn reset_options(&mut self) {
        info!("Resetting options")
    }

    async fn enter(&mut self) {
        info!("Entering");
        self.sender.send(IndicatorStatus::EnterScreen).await;
    }

    async fn leave(&mut self) {
        info!("Leaving");
        let mut report = [0; 9];
        let ret = self.hid.clear(ReportType::Keyboard, &mut report);
        self.send_report(ret).await;
        let ret = self.hid.clear(ReportType::Mouse, &mut report);
        self.send_report(ret).await;
        let ret = self.hid.clear(ReportType::Consumer, &mut report);
        self.send_report(ret).await;
        self.sender.send(IndicatorStatus::LeaveScreen).await;
    }
}
