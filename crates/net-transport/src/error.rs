//! Transport error types

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Connection closed: {0}")]
    ConnectionClosed(String),

    #[error("Connection timeout")]
    Timeout,

    #[error("Stream error: {0}")]
    Stream(String),

    #[error("Datagram error: {0}")]
    Datagram(String),

    #[error("Datagram too large: {size} bytes (max: {max})")]
    DatagramTooLarge { size: usize, max: usize },

    #[error("TLS error: {0}")]
    Tls(String),

    #[error("Certificate error: {0}")]
    Certificate(String),

    #[error("Address parse error: {0}")]
    AddressParse(String),

    #[error("Bind error: {0}")]
    Bind(String),

    #[error("Send error: {0}")]
    Send(String),

    #[error("Receive error: {0}")]
    Receive(String),

    #[error("Encryption error: {0}")]
    Encryption(String),

    #[error("Already connected")]
    AlreadyConnected,

    #[error("Not connected")]
    NotConnected,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type TransportResult<T> = Result<T, TransportError>;
