use embedded_io_async::Write;

use super::{PacketError, PacketWriter};

#[allow(dead_code)]
#[derive(Debug)]
pub enum Packet {
    QueryInfo,
    DeviceInfo {
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        _dummy: u16,
        mx: u16, // x position of the mouse on the secondary screen
        my: u16, // y position of the mouse on the secondary screen
    },
    InfoAck,
    KeepAlive,
    ResetOptions,
    ClientNoOp,
    ErrorUnknownDevice,
    CursorEnter {
        x: u16,
        y: u16,
        seq_num: u32,
        mask: u16,
    },
    MouseUp {
        id: i8,
    },
    MouseDown {
        id: i8,
    },
    KeyUp {
        id: u16,
        mask: u16,
        button: u16,
    },
    KeyDown {
        id: u16,
        mask: u16,
        button: u16,
    },
    KeyRepeat {
        id: u16,
        mask: u16,
        button: u16,
        count: u16,
    },
    MouseWheel {
        x_delta: i16,
        y_delta: i16,
    },
    CursorLeave,
    MouseMoveAbs {
        x: u16,
        y: u16,
    },
    MouseMove {
        x: i16,
        y: i16,
    },
    Unknown([u8; 4]),
}

impl Packet {
    pub async fn write_wire<W: Write + Unpin>(self, mut out: W) -> Result<(), PacketError> {
        match self {
            Packet::QueryInfo => {
                out.write_str("QINF").await?;
                Ok(())
            }
            Packet::DeviceInfo {
                x,
                y,
                w,
                h,
                _dummy,
                mx,
                my,
            } => {
                out.write_u32(2 * 7 + 4).await?;
                out.write_all(b"DINF").await.map_err(|_| PacketError::IoError)?;
                out.write_u16(x).await?;
                out.write_u16(y).await?;
                out.write_u16(w).await?;
                out.write_u16(h).await?;
                out.write_u16(0).await?;
                out.write_u16(mx).await?;
                out.write_u16(my).await?;
                Ok(())
            }
            Packet::ClientNoOp => {
                out.write_str("CNOP").await?;
                Ok(())
            }
            Packet::Unknown(_) => {
                unimplemented!()
            }
            Packet::InfoAck => {
                out.write_str("CIAK").await?;
                Ok(())
            }
            Packet::KeepAlive => {
                out.write_str("CALV").await?;
                Ok(())
            }
            Packet::ErrorUnknownDevice => {
                out.write_str("EUNK").await?;
                Ok(())
            }
            Packet::MouseMoveAbs { x, y } => {
                out.write_u32(4 + 2 + 2).await?;
                out.write_all(b"DMMV").await.map_err(|_| PacketError::IoError)?;
                out.write_u16(x).await?;
                out.write_u16(y).await?;
                Ok(())
            }
            _ => {
                unimplemented!("{:?} not yet implemented", self)
            }
        }
    }
}
