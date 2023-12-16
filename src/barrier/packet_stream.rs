use core::str::from_utf8;
use defmt::*;

use super::{Packet, PacketError, PacketReader, PacketWriter};

pub struct PacketStream<S: PacketReader + PacketWriter> {
    stream: S,
}

impl<S: PacketReader + PacketWriter> PacketStream<S> {
    pub fn new(stream: S) -> Self {
        Self { stream }
    }

    pub async fn read(&mut self) -> Result<Packet, PacketError> {
        let size = self.stream.read_packet_size().await?;
        if size < 4 {
            return Err(PacketError::PacketTooSmall);
        }
        let mut chunk = super::take::Take::new(&mut self.stream, size as u64);
        let code: [u8; 4] = chunk.read_bytes_fixed().await?;
        debug!("Got packet {:?}", from_utf8(&code).unwrap_or("???"));
        if size > 2048 {
            warn!("Packet too large, discarding {} bytes", size);
            chunk
                .discard_all()
                .await
                .map_err(|_| PacketError::IoError)?;
            return Ok(Packet::Unknown(code));
        }

        let packet = match code.as_ref() {
            b"QINF" => Packet::QueryInfo,
            b"CIAK" => Packet::InfoAck,
            b"CALV" => Packet::KeepAlive,
            b"EUNK" => Packet::ErrorUnknownDevice,
            b"DMMV" => {
                let x = chunk.read_u16().await?;
                let y = chunk.read_u16().await?;
                Packet::MouseMoveAbs { x, y }
            }
            b"DMRM" => {
                let x = chunk.read_i16().await?;
                let y = chunk.read_i16().await?;
                Packet::MouseMove { x, y }
            }
            b"CINN" => {
                let x = chunk.read_u16().await?;
                let y = chunk.read_u16().await?;
                let seq_num = chunk.read_u32().await?;
                let mask = chunk.read_u16().await?;
                Packet::CursorEnter {
                    x,
                    y,
                    seq_num,
                    mask,
                }
            }
            b"COUT" => Packet::CursorLeave,
            b"DMUP" => {
                let id = chunk.read_i8().await?;
                Packet::MouseUp { id }
            }
            b"DMDN" => {
                let id = chunk.read_i8().await?;
                Packet::MouseDown { id }
            }
            b"DKUP" => {
                let id = chunk.read_u16().await?;
                let mask = chunk.read_u16().await?;
                let button = chunk.read_u16().await?;
                Packet::KeyUp { id, mask, button }
            }
            b"DKDN" => {
                let id = chunk.read_u16().await?;
                let mask = chunk.read_u16().await?;
                let button = chunk.read_u16().await?;
                Packet::KeyDown { id, mask, button }
            }
            b"DKRP" => {
                let id = chunk.read_u16().await?;
                let mask = chunk.read_u16().await?;
                let count = chunk.read_u16().await?;
                let button = chunk.read_u16().await?;
                Packet::KeyRepeat {
                    id,
                    mask,
                    button,
                    count,
                }
            }
            b"DMWM" => {
                let x_delta = chunk.read_i16().await?;
                let y_delta = chunk.read_i16().await?;
                Packet::MouseWheel { x_delta, y_delta }
            }
            _ => Packet::Unknown(code),
        };

        // Discard the rest of the packet
        while chunk.limit() > 0 {
            warn!(
                "Discarding rest of packet, code: {:?}, size: {}",
                from_utf8(&code).unwrap_or("???"),
                chunk.limit()
            );
            chunk
                .discard_all()
                .await
                .map_err(|_| PacketError::IoError)?;
        }

        Ok(packet)
    }

    pub async fn write(&mut self, packet: Packet) -> Result<(), PacketError> {
        packet.write_wire(&mut self.stream).await?;
        Ok(())
    }
}
