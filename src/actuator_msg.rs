use defmt::info;
use embassy_usb::{
    class::cdc_acm::CdcAcmClass,
    driver::{Driver, EndpointError},
};

pub enum ActuatorMsg {
    Unknown,
    Connected,
    Disconnected,
    GetScreenSize,
    GetCursorPosition,
    SetCursorPosition(u16, u16),
    MouseDown(i8),
    MouseUp(i8),
    MouseWheel(i16, i16),
    KeyDown(u16, u16, u16),
    KeyRepeat(u16, u16, u16, u16),
    KeyUp(u16, u16, u16),
    ResetOptions,
    Enter,
    Leave,
}

pub trait ReadMsg {
    async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), EndpointError>;
    async fn read_u8(&mut self) -> Result<u8, EndpointError>;
    async fn read_i8(&mut self) -> Result<i8, EndpointError>;
    async fn read_u16(&mut self) -> Result<u16, EndpointError>;
    async fn read_i16(&mut self) -> Result<i16, EndpointError>;
}

impl<'d, D: Driver<'d>> ReadMsg for CdcAcmClass<'d, D> {
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

impl ActuatorMsg {
    pub async fn read_msg<'d, D>(mut s: CdcAcmClass<'d, D>) -> Result<ActuatorMsg, EndpointError>
    where
        D: Driver<'d>,
    {
        let mut buf = [0; 1];
        s.read_exact(&mut buf).await?;
        match buf[0] {
            0x01 => {
                info!("Connected");
                Ok(ActuatorMsg::Connected)
            }
            0x02 => {
                info!("Disconnected");
                Ok(ActuatorMsg::Disconnected)
            }
            0x05 => {
                info!("SetCursorPosition");
                let x = s.read_u16().await?;
                let y = s.read_u16().await?;
                Ok(ActuatorMsg::SetCursorPosition(x, y))
            }
            0x06 => {
                info!("MouseDown");
                let button = s.read_i8().await?;
                Ok(ActuatorMsg::MouseDown(button))
            }
            0x07 => {
                info!("MouseUp");
                let button = s.read_i8().await?;
                Ok(ActuatorMsg::MouseUp(button))
            }
            0x08 => {
                info!("MouseWheel");
                let x = s.read_i16().await?;
                let y = s.read_i16().await?;
                Ok(ActuatorMsg::MouseWheel(x, y))
            }
            0x09 => {
                info!("KeyDown");
                let modifiers = s.read_u16().await?;
                let key = s.read_u16().await?;
                let key2 = s.read_u16().await?;
                Ok(ActuatorMsg::KeyDown(modifiers, key, key2))
            }
            0x0a => {
                info!("KeyRepeat");
                let modifiers = s.read_u16().await?;
                let key = s.read_u16().await?;
                let key2 = s.read_u16().await?;
                let delay = s.read_u16().await?;
                Ok(ActuatorMsg::KeyRepeat(modifiers, key, key2, delay))
            }
            0x0b => {
                info!("KeyUp");
                let modifiers = s.read_u16().await?;
                let key = s.read_u16().await?;
                let key2 = s.read_u16().await?;
                Ok(ActuatorMsg::KeyUp(modifiers, key, key2))
            }
            0x0c => {
                info!("ResetOptions");
                Ok(ActuatorMsg::ResetOptions)
            }
            0x0d => {
                info!("Enter");
                Ok(ActuatorMsg::Enter)
            }
            0x0e => {
                info!("Leave");
                Ok(ActuatorMsg::Leave)
            }
            _ => {
                info!("Unknown");
                Ok(ActuatorMsg::Unknown)
            }
        }
    }
}
