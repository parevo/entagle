//! Cryptographic session management with X25519 + ChaCha20Poly1305

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use rand::rngs::OsRng;
use x25519_dalek::{EphemeralSecret, PublicKey, SharedSecret};
use zeroize::Zeroizing;

use crate::{CryptoError, CryptoResult, NONCE_SIZE, PUBLIC_KEY_SIZE, TAG_SIZE};

/// Key pair for ephemeral key exchange
pub struct KeyPair {
    secret: EphemeralSecret,
    public: PublicKey,
}

impl KeyPair {
    /// Generate a new ephemeral key pair
    pub fn generate() -> Self {
        let secret = EphemeralSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    /// Get the public key bytes
    pub fn public_key_bytes(&self) -> [u8; PUBLIC_KEY_SIZE] {
        *self.public.as_bytes()
    }

    /// Perform Diffie-Hellman key exchange
    pub fn diffie_hellman(self, their_public: &[u8; PUBLIC_KEY_SIZE]) -> SharedSecret {
        let their_public = PublicKey::from(*their_public);
        self.secret.diffie_hellman(&their_public)
    }
}

/// Direction of encryption (affects nonce generation)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// We initiated the connection
    Initiator,
    /// We received the connection
    Responder,
}

/// Established cryptographic session
pub struct CryptoSession {
    /// Cipher for encryption/decryption
    cipher: ChaCha20Poly1305,
    /// Message counter for nonce generation (send)
    send_counter: u64,
    /// Message counter for nonce generation (receive)
    recv_counter: u64,
    /// Our direction in the handshake
    direction: Direction,
}

impl CryptoSession {
    /// Create a new session from a shared secret
    ///
    /// # Arguments
    /// * `shared_secret` - The result of X25519 key exchange
    /// * `direction` - Whether we initiated or responded
    pub fn from_shared_secret(
        shared_secret: &SharedSecret,
        direction: Direction,
    ) -> CryptoResult<Self> {
        // Use the shared secret directly as the symmetric key
        // In production, you might want to use HKDF for key derivation
        let key = Zeroizing::new(*shared_secret.as_bytes());

        let cipher = ChaCha20Poly1305::new_from_slice(&*key)
            .map_err(|e| CryptoError::KeyGeneration(e.to_string()))?;

        Ok(Self {
            cipher,
            send_counter: 0,
            recv_counter: 0,
            direction,
        })
    }

    /// Generate nonce from counter
    ///
    /// Nonce format: [4 bytes direction prefix][8 bytes counter]
    fn generate_nonce(&self, counter: u64, is_send: bool) -> [u8; NONCE_SIZE] {
        let mut nonce = [0u8; NONCE_SIZE];

        // Direction prefix ensures sender and receiver nonces never collide
        let prefix: u32 = match (self.direction, is_send) {
            (Direction::Initiator, true) => 0x00000000,
            (Direction::Initiator, false) => 0xFFFFFFFF,
            (Direction::Responder, true) => 0xFFFFFFFF,
            (Direction::Responder, false) => 0x00000000,
        };

        nonce[0..4].copy_from_slice(&prefix.to_le_bytes());
        nonce[4..12].copy_from_slice(&counter.to_le_bytes());

        nonce
    }

    /// Encrypt data with authentication
    ///
    /// Returns: [ciphertext][16-byte auth tag]
    pub fn encrypt(&mut self, plaintext: &[u8]) -> CryptoResult<Vec<u8>> {
        // Check for nonce overflow
        if self.send_counter == u64::MAX {
            return Err(CryptoError::NonceOverflow);
        }

        let nonce_bytes = self.generate_nonce(self.send_counter, true);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| CryptoError::Encryption(e.to_string()))?;

        self.send_counter += 1;

        Ok(ciphertext)
    }

    /// Decrypt and verify data
    pub fn decrypt(&mut self, ciphertext: &[u8]) -> CryptoResult<Vec<u8>> {
        if ciphertext.len() < TAG_SIZE {
            return Err(CryptoError::DecryptionFailed);
        }

        // Check for nonce overflow
        if self.recv_counter == u64::MAX {
            return Err(CryptoError::NonceOverflow);
        }

        let nonce_bytes = self.generate_nonce(self.recv_counter, false);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let plaintext = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| CryptoError::DecryptionFailed)?;

        self.recv_counter += 1;

        Ok(plaintext)
    }

    /// Get the current send counter (for debugging/stats)
    pub fn send_count(&self) -> u64 {
        self.send_counter
    }

    /// Get the current receive counter (for debugging/stats)
    pub fn recv_count(&self) -> u64 {
        self.recv_counter
    }
}

/// Builder for establishing a crypto session through handshake
pub struct HandshakeBuilder {
    our_keypair: KeyPair,
    direction: Direction,
}

impl HandshakeBuilder {
    /// Start a new handshake as initiator
    pub fn new_initiator() -> Self {
        Self {
            our_keypair: KeyPair::generate(),
            direction: Direction::Initiator,
        }
    }

    /// Start a new handshake as responder
    pub fn new_responder() -> Self {
        Self {
            our_keypair: KeyPair::generate(),
            direction: Direction::Responder,
        }
    }

    /// Get our public key to send to the peer
    pub fn public_key(&self) -> [u8; PUBLIC_KEY_SIZE] {
        self.our_keypair.public_key_bytes()
    }

    /// Complete the handshake with the peer's public key
    pub fn complete(self, their_public: &[u8; PUBLIC_KEY_SIZE]) -> CryptoResult<CryptoSession> {
        let shared_secret = self.our_keypair.diffie_hellman(their_public);
        CryptoSession::from_shared_secret(&shared_secret, self.direction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handshake_and_encryption() {
        // Simulate two peers
        let initiator = HandshakeBuilder::new_initiator();
        let responder = HandshakeBuilder::new_responder();

        let initiator_public = initiator.public_key();
        let responder_public = responder.public_key();

        let mut initiator_session = initiator.complete(&responder_public).unwrap();
        let mut responder_session = responder.complete(&initiator_public).unwrap();

        // Test initiator -> responder
        let message = b"Hello from initiator!";
        let encrypted = initiator_session.encrypt(message).unwrap();
        let decrypted = responder_session.decrypt(&encrypted).unwrap();
        assert_eq!(message.as_slice(), decrypted.as_slice());

        // Test responder -> initiator
        let response = b"Hello from responder!";
        let encrypted = responder_session.encrypt(response).unwrap();
        let decrypted = initiator_session.decrypt(&encrypted).unwrap();
        assert_eq!(response.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_nonce_uniqueness() {
        let initiator = HandshakeBuilder::new_initiator();
        let responder = HandshakeBuilder::new_responder();

        let initiator_public = initiator.public_key();
        let responder_public = responder.public_key();

        let mut initiator_session = initiator.complete(&responder_public).unwrap();
        let mut responder_session = responder.complete(&initiator_public).unwrap();

        // Send multiple messages and verify they can all be decrypted
        for i in 0..100 {
            let msg = format!("Message {}", i);
            let encrypted = initiator_session.encrypt(msg.as_bytes()).unwrap();
            let decrypted = responder_session.decrypt(&encrypted).unwrap();
            assert_eq!(msg.as_bytes(), decrypted.as_slice());
        }
    }
}
