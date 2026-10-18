//! Stream ciphers: what a connection does to bytes before they leave and after they arrive.

/// Scrambles a connection's byte stream. Bytes go through in order, each direction once, so
/// a cipher may keep state (counters, keys) per direction.
pub trait Cipher: Send {
    /// Encrypts outgoing bytes in place.
    fn encrypt(&mut self, data: &mut [u8]);
    /// Decrypts incoming bytes in place.
    fn decrypt(&mut self, data: &mut [u8]);
    /// Sees each whole packet after it was decrypted, before the next one is: a cipher that
    /// changes keys on a login packet does it here, so the following bytes use the new keys.
    fn received(&mut self, _packet: &[u8]) {}
}

/// No encryption.
#[derive(Debug, Default, Clone, Copy)]
pub struct Plain;

impl Cipher for Plain {
    fn encrypt(&mut self, _data: &mut [u8]) {}
    fn decrypt(&mut self, _data: &mut [u8]) {}
}
