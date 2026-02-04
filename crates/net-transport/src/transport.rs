//! QUIC transport implementation

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use parking_lot::RwLock;
use quinn::{
    ClientConfig, Connection, Endpoint, RecvStream, SendStream, ServerConfig, TransportConfig,
};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::{CongestionController, TransportError, TransportResult, MAX_DATAGRAM_SIZE};

/// QUIC transport for Entangle
pub struct QuicTransport {
    endpoint: Endpoint,
    connection: RwLock<Option<Connection>>,
    congestion: Arc<CongestionController>,
    datagram_tx: mpsc::Sender<Bytes>,
    datagram_rx: RwLock<Option<mpsc::Receiver<Bytes>>>,
}

impl QuicTransport {
    /// Create a new QUIC transport (client mode)
    pub async fn new_client(bind_addr: SocketAddr) -> TransportResult<Self> {
        let client_config = Self::create_client_config()?;
        
        let mut endpoint = Endpoint::client(bind_addr)?;
        endpoint.set_default_client_config(client_config);

        let (datagram_tx, datagram_rx) = mpsc::channel(1000);

        Ok(Self {
            endpoint,
            connection: RwLock::new(None),
            congestion: Arc::new(CongestionController::new(Default::default())),
            datagram_tx,
            datagram_rx: RwLock::new(Some(datagram_rx)),
        })
    }

    /// Create a new QUIC transport (server mode)  
    pub async fn new_server(bind_addr: SocketAddr) -> TransportResult<Self> {
        let (server_config, _cert) = Self::create_server_config()?;
        
        let endpoint = Endpoint::server(server_config, bind_addr)?;

        let (datagram_tx, datagram_rx) = mpsc::channel(1000);

        Ok(Self {
            endpoint,
            connection: RwLock::new(None),
            congestion: Arc::new(CongestionController::new(Default::default())),
            datagram_tx,
            datagram_rx: RwLock::new(Some(datagram_rx)),
        })
    }

    /// Create client TLS config (insecure for development)
    fn create_client_config() -> TransportResult<ClientConfig> {
        let crypto = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
            .with_no_client_auth();

        let mut transport = TransportConfig::default();
        transport.max_idle_timeout(Some(Duration::from_secs(30).try_into().unwrap()));
        transport.keep_alive_interval(Some(Duration::from_secs(5)));
        transport.datagram_receive_buffer_size(Some(MAX_DATAGRAM_SIZE * 1000));

        let mut config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
                .map_err(|e| TransportError::Tls(e.to_string()))?,
        ));
        config.transport_config(Arc::new(transport));

        Ok(config)
    }

    /// Create server TLS config with self-signed certificate
    fn create_server_config() -> TransportResult<(ServerConfig, rcgen::CertifiedKey)> {
        let certified_key = rcgen::generate_simple_self_signed(vec!["entangle.local".to_string()])
            .map_err(|e| TransportError::Certificate(e.to_string()))?;

        let cert_der = certified_key.cert.der().clone();
        let key_der = certified_key.key_pair.serialize_der();

        let cert_chain = vec![cert_der];
        let key = rustls::pki_types::PrivatePkcs8KeyDer::from(key_der);

        let mut server_crypto = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, key.into())
            .map_err(|e| TransportError::Tls(e.to_string()))?;

        server_crypto.max_early_data_size = u32::MAX;
        server_crypto.alpn_protocols = vec![b"entangle".to_vec()];

        let mut transport = TransportConfig::default();
        transport.max_idle_timeout(Some(Duration::from_secs(30).try_into().unwrap()));
        transport.keep_alive_interval(Some(Duration::from_secs(5)));
        transport.datagram_receive_buffer_size(Some(MAX_DATAGRAM_SIZE * 1000));

        let mut config = ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)
                .map_err(|e| TransportError::Tls(e.to_string()))?,
        ));
        config.transport_config(Arc::new(transport));

        Ok((config, certified_key))
    }

    /// Connect to a remote peer
    pub async fn connect(&self, addr: SocketAddr, server_name: &str) -> TransportResult<()> {
        if self.connection.read().is_some() {
            return Err(TransportError::AlreadyConnected);
        }

        info!("Connecting to {} ({})", addr, server_name);

        let connection = self
            .endpoint
            .connect(addr, server_name)
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?
            .await
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        info!("Connected to {}", addr);

        *self.connection.write() = Some(connection.clone());

        // Start datagram receiver task
        self.spawn_datagram_receiver(connection);

        Ok(())
    }

    /// Accept an incoming connection (server mode)
    pub async fn accept(&self) -> TransportResult<()> {
        info!("Waiting for incoming connection...");

        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or_else(|| TransportError::ConnectionFailed("Endpoint closed".to_string()))?;

        let connection = incoming
            .await
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        info!("Accepted connection from {}", connection.remote_address());

        *self.connection.write() = Some(connection.clone());

        // Start datagram receiver task
        self.spawn_datagram_receiver(connection);

        Ok(())
    }

    /// Spawn a task to receive datagrams
    fn spawn_datagram_receiver(&self, connection: Connection) {
        let tx = self.datagram_tx.clone();
        let _congestion = self.congestion.clone();

        tokio::spawn(async move {
            loop {
                match connection.read_datagram().await {
                    Ok(data) => {
                        if tx.send(data).await.is_err() {
                            debug!("Datagram receiver channel closed");
                            break;
                        }
                    }
                    Err(e) => {
                        if e.to_string().contains("closed") {
                            debug!("Connection closed, stopping datagram receiver");
                        } else {
                            warn!("Datagram receive error: {}", e);
                        }
                        break;
                    }
                }
            }
        });
    }

    /// Send a datagram (unreliable, fire-and-forget)
    pub fn send_datagram(&self, data: Bytes) -> TransportResult<()> {
        let conn = self.connection.read();
        let connection = conn
            .as_ref()
            .ok_or(TransportError::NotConnected)?;

        if data.len() > MAX_DATAGRAM_SIZE {
            return Err(TransportError::DatagramTooLarge {
                size: data.len(),
                max: MAX_DATAGRAM_SIZE,
            });
        }

        connection
            .send_datagram(data)
            .map_err(|e| TransportError::Send(e.to_string()))?;

        Ok(())
    }

    /// Receive a datagram
    pub async fn recv_datagram(&self) -> TransportResult<Bytes> {
        let mut rx_guard = self.datagram_rx.write();
        let rx = rx_guard
            .as_mut()
            .ok_or(TransportError::NotConnected)?;

        rx.recv()
            .await
            .ok_or(TransportError::Receive("Channel closed".to_string()))
    }

    /// Open a new bidirectional stream (reliable)
    pub async fn open_bi_stream(&self) -> TransportResult<(SendStream, RecvStream)> {
        let conn = self.connection.read();
        let connection = conn
            .as_ref()
            .ok_or(TransportError::NotConnected)?;

        connection
            .open_bi()
            .await
            .map_err(|e| TransportError::Stream(e.to_string()))
    }

    /// Open a new unidirectional stream (reliable)
    pub async fn open_uni_stream(&self) -> TransportResult<SendStream> {
        let conn = self.connection.read();
        let connection = conn
            .as_ref()
            .ok_or(TransportError::NotConnected)?;

        connection
            .open_uni()
            .await
            .map_err(|e| TransportError::Stream(e.to_string()))
    }

    /// Accept an incoming bidirectional stream
    pub async fn accept_bi_stream(&self) -> TransportResult<(SendStream, RecvStream)> {
        let conn = self.connection.read();
        let connection = conn
            .as_ref()
            .ok_or(TransportError::NotConnected)?
            .clone();
        drop(conn);

        connection
            .accept_bi()
            .await
            .map_err(|e| TransportError::Stream(e.to_string()))
    }

    /// Get the congestion controller
    pub fn congestion(&self) -> &CongestionController {
        &self.congestion
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.connection.read().is_some()
    }

    /// Get remote address
    pub fn remote_address(&self) -> Option<SocketAddr> {
        self.connection.read().as_ref().map(|c| c.remote_address())
    }

    /// Close the connection
    pub fn close(&self, reason: &str) {
        if let Some(conn) = self.connection.write().take() {
            conn.close(0u32.into(), reason.as_bytes());
            info!("Connection closed: {}", reason);
        }
    }

    /// Get connection stats
    pub fn stats(&self) -> Option<ConnectionStats> {
        self.connection.read().as_ref().map(|conn| {
            let stats = conn.stats();
            ConnectionStats {
                rtt: stats.path.rtt,
                congestion_window: stats.path.cwnd as usize,
                bytes_sent: stats.udp_tx.bytes,
                bytes_received: stats.udp_rx.bytes,
                packets_sent: stats.path.sent_packets,
                packets_lost: stats.path.lost_packets,
            }
        })
    }
}

/// Connection statistics
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub rtt: Duration,
    pub congestion_window: usize,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_lost: u64,
}

/// Skip TLS server verification (for development only)
#[derive(Debug)]
struct SkipServerVerification;

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::ED25519,
        ]
    }
}
