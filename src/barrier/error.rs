use thiserror::Error;

#[derive(Error, Debug)]
pub enum PacketError {
    #[error("io error")]
    IoError,
    #[error("did not match format")]
    FormatError,
    #[error("Packet too small")]
    PacketTooSmall,
}

impl<T> From<embedded_io_async::ReadExactError<T>> for PacketError {
    fn from(_: embedded_io_async::ReadExactError<T>) -> Self {
        PacketError::IoError
    }
}

#[derive(Error, Debug)]
pub enum ConnectionError {
    #[error("tcp connection failed")]
    TcpError,
    #[error("invalid data received")]
    ProtocolError(#[from] PacketError),
}

impl From<embassy_net::tcp::ConnectError> for ConnectionError {
    fn from(_: embassy_net::tcp::ConnectError) -> Self {
        ConnectionError::TcpError
    }
}
