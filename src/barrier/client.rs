use defmt::debug;
use embassy_net::tcp::TcpSocket;
use embassy_rp::watchdog::Watchdog;
use embedded_io_async::Write;

use crate::barrier::{packet_stream::PacketStream, PacketError};

use super::{Actuator, ConnectionError, Packet, PacketReader, PacketWriter};

pub async fn start<'a, A: Actuator>(
    mut stream: TcpSocket<'a>,
    device_name: &str,
    actor: &mut A,
    watchdog: &mut Watchdog,
) -> Result<(), ConnectionError> {
    let screen_size: (u16, u16) = actor.get_screen_size().await;

    // Turn off Nagle, this may not be available on ESP-IDF, so ignore the error.
    // stream.set_nodelay(true).ok();

    let _size = stream.read_packet_size().await?;
    stream.read_str_lit("Barrier").await?;
    let major = stream.read_u16().await?;
    let minor = stream.read_u16().await?;
    debug!("Got hello {}:{}", major, minor);

    stream
        .write_u32("Barrier".len() as u32 + 2 + 2 + 4 + device_name.bytes().len() as u32)
        .await?;
    stream
        .write_all(b"Barrier")
        .await
        .map_err(|_| PacketError::IoError)?;
    stream.write_u16(1).await?;
    stream.write_u16(6).await?;
    stream.write_str(device_name).await?;

    actor.connected().await;

    let mut packet_stream = PacketStream::new(stream);
    while let Ok(packet) = packet_stream.read().await {
        match packet {
            Packet::QueryInfo => {
                let (x, y) = actor.get_cursor_position().await;
                match packet_stream
                    .write(Packet::DeviceInfo {
                        x,
                        y,
                        w: screen_size.0,
                        h: screen_size.1,
                        _dummy: 0,
                        mx: 0,
                        my: 0,
                    })
                    .await
                {
                    Ok(_) => {}
                    Err(e) => {
                        actor.disconnected().await;
                        return Err(e.into());
                    }
                }
            }
            Packet::KeepAlive => match packet_stream.write(Packet::KeepAlive).await {
                Ok(_) => {
                    watchdog.feed();
                }
                Err(e) => {
                    actor.disconnected().await;
                    return Err(e.into());
                }
            },
            Packet::MouseMoveAbs { x, y } => {
                actor.set_cursor_position(x, y).await;
            }
            Packet::MouseMove { x, y } => {
                actor.move_cursor(x, y).await;
            }
            Packet::KeyUp { id, mask, button } => {
                actor.key_up(id, mask, button).await;
            }
            Packet::KeyDown { id, mask, button } => {
                actor.key_down(id, mask, button).await;
            }
            Packet::KeyRepeat {
                id,
                mask,
                button,
                count,
            } => {
                actor.key_repeat(id, mask, button, count).await;
            }
            Packet::MouseDown { id } => {
                actor.mouse_down(id).await;
            }
            Packet::MouseUp { id } => {
                actor.mouse_up(id).await;
            }
            Packet::MouseWheel { x_delta, y_delta } => {
                actor.mouse_wheel(x_delta, y_delta).await;
            }
            Packet::InfoAck => { //Ignore
            }
            Packet::ResetOptions => {
                actor.reset_options().await;
            }
            Packet::CursorEnter { .. } => {
                actor.enter().await;
            }
            Packet::CursorLeave => {
                actor.leave().await;
            }
            Packet::DeviceInfo { .. } | Packet::ErrorUnknownDevice | Packet::ClientNoOp => {
                // Server only packets
            }
            Packet::Unknown(_) => {}
        }
    }
    panic!("Connection closed.");
}
