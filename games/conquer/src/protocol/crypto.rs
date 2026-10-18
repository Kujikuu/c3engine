//! Conquer 5065's ciphers: the account server's TQ XOR stream, the login password's RC5,
//! and the game server's Blowfish-CFB64 whose key comes from a Diffie-Hellman exchange (the
//! server's key packet, the client's reply). See docs/conquer5065_protocol.md, part A.

use blowfish::Blowfish;
use c3_net::Cipher;
use num_bigint::BigUint;

/// The two 256-byte key tables of the TQ XOR cipher.
fn tq_tables() -> ([u8; 256], [u8; 256]) {
    let (mut k1, mut k2) = ([0u8; 256], [0u8; 256]);
    k1[0] = 0x9D;
    k2[0] = 0x62;
    for i in 0..255 {
        let a = k1[i];
        k1[i + 1] = 0x0Fu8
            .wrapping_add(a.wrapping_mul(0xFA))
            .wrapping_mul(a)
            .wrapping_add(0x13);
        let b = k2[i];
        k2[i + 1] = 0x79u8
            .wrapping_sub(b.wrapping_mul(0x5C))
            .wrapping_mul(b)
            .wrapping_add(0x6D);
    }
    (k1, k2)
}

/// The account server's cipher (server side): one transform, a counter per direction.
pub struct TqCipher {
    k1: [u8; 256],
    k2: [u8; 256],
    sent: u16,
    received: u16,
}

impl TqCipher {
    pub fn new() -> Self {
        let (k1, k2) = tq_tables();
        Self {
            k1,
            k2,
            sent: 0,
            received: 0,
        }
    }

    fn apply(k1: &[u8; 256], k2: &[u8; 256], counter: &mut u16, data: &mut [u8]) {
        for b in data {
            let v = (*b ^ 0xAB).rotate_left(4);
            *b = v ^ k2[(*counter >> 8) as usize] ^ k1[(*counter & 0xFF) as usize];
            *counter = counter.wrapping_add(1);
        }
    }
}

impl Default for TqCipher {
    fn default() -> Self {
        Self::new()
    }
}

impl Cipher for TqCipher {
    fn encrypt(&mut self, data: &mut [u8]) {
        Self::apply(&self.k1, &self.k2, &mut self.sent, data);
    }

    fn decrypt(&mut self, data: &mut [u8]) {
        Self::apply(&self.k1, &self.k2, &mut self.received, data);
    }
}

/// The key the client RC5-encrypts its password with.
const RC5_KEY: [u8; 16] = [
    0x3C, 0xDC, 0xFE, 0xE8, 0xC4, 0x54, 0xD6, 0x7E, 0x16, 0xA6, 0xF8, 0x1A, 0xE8, 0xD0, 0x38, 0xBE,
];
const RC5_ROUNDS: usize = 12;

/// RC5-32/12/16's expanded key.
fn rc5_schedule() -> [u32; 26] {
    let mut s = [0u32; 26];
    s[0] = 0xB7E1_5163;
    for i in 1..26 {
        s[i] = s[i - 1].wrapping_add(0x9E37_79B9);
    }
    let mut l = [0u32; 4];
    for (i, w) in l.iter_mut().enumerate() {
        *w = u32::from_le_bytes(RC5_KEY[i * 4..i * 4 + 4].try_into().expect("4 bytes"));
    }
    let (mut i, mut j, mut x, mut y) = (0, 0, 0u32, 0u32);
    for _ in 0..78 {
        s[i] = s[i].wrapping_add(x).wrapping_add(y).rotate_left(3);
        x = s[i];
        i = (i + 1) % 26;
        l[j] = l[j]
            .wrapping_add(x)
            .wrapping_add(y)
            .rotate_left(x.wrapping_add(y) % 32);
        y = l[j];
        j = (j + 1) % 4;
    }
    s
}

/// The password the client sent in its login packet (16 RC5-encrypted bytes).
pub fn decrypt_password(sealed: &[u8; 16]) -> String {
    let s = rc5_schedule();
    let mut plain = [0u8; 16];
    for block in 0..2 {
        let word = |k: usize| {
            u32::from_le_bytes(
                sealed[block * 8 + k..block * 8 + k + 4]
                    .try_into()
                    .expect("4 bytes"),
            )
        };
        let (mut a, mut b) = (word(0), word(4));
        for r in (1..=RC5_ROUNDS).rev() {
            b = b.wrapping_sub(s[2 * r + 1]).rotate_right(a % 32) ^ a;
            a = a.wrapping_sub(s[2 * r]).rotate_right(b % 32) ^ b;
        }
        b = b.wrapping_sub(s[1]);
        a = a.wrapping_sub(s[0]);
        plain[block * 8..block * 8 + 4].copy_from_slice(&a.to_le_bytes());
        plain[block * 8 + 4..block * 8 + 8].copy_from_slice(&b.to_le_bytes());
    }
    let end = plain.iter().position(|&c| c == 0).unwrap_or(16);
    String::from_utf8_lossy(&plain[..end]).into_owned()
}

/// Blowfish in CFB-64 stream mode (OpenSSL's `BF_cfb64_encrypt`), one direction.
struct BlowfishCfb {
    cipher: Blowfish,
    iv: [u8; 8],
    num: usize,
}

impl BlowfishCfb {
    /// Keyed with `key` of any length (the DH secret may be 64 bytes).
    fn new(key: &[u8], iv: [u8; 8]) -> Self {
        let mut cipher = Blowfish::bc_init_state();
        cipher.bc_expand_key(key);
        Self { cipher, iv, num: 0 }
    }

    fn next_pad(&mut self) {
        let l = u32::from_be_bytes(self.iv[..4].try_into().expect("4 bytes"));
        let r = u32::from_be_bytes(self.iv[4..].try_into().expect("4 bytes"));
        let [l, r] = self.cipher.bc_encrypt([l, r]);
        self.iv[..4].copy_from_slice(&l.to_be_bytes());
        self.iv[4..].copy_from_slice(&r.to_be_bytes());
    }

    fn encrypt(&mut self, data: &mut [u8]) {
        for b in data {
            if self.num == 0 {
                self.next_pad();
            }
            *b ^= self.iv[self.num];
            self.iv[self.num] = *b;
            self.num = (self.num + 1) & 7;
        }
    }

    fn decrypt(&mut self, data: &mut [u8]) {
        for b in data {
            if self.num == 0 {
                self.next_pad();
            }
            let c = *b;
            *b ^= self.iv[self.num];
            self.iv[self.num] = c;
            self.num = (self.num + 1) & 7;
        }
    }
}

/// The key both sides use until the exchange is done.
const BOOTSTRAP_KEY: &[u8] = b"DR654dt34trg4UI6";
/// The exchange's 512-bit prime and generator.
const DH_P: &str = "E7A69EBDF105F2A6BBDEAD7E798F76A209AD73FB466431E2E7352ED262F8C558F10BEFEA977DE9E21DCEE9B04D245F300ECCBBA03E72630556D011023F9E857F";
const DH_G: &str = "05";
/// The IVs the server announces (as TQ's server did: zero).
const IV_CLIENT: [u8; 8] = [0; 8];
const IV_SERVER: [u8; 8] = [0; 8];
/// Padding and junk lengths of the key packets.
pub const SERVER_PAD: usize = 11;
pub const CLIENT_PAD: usize = 7;
const JUNK: usize = 12;
/// Bytes of the seal after every game packet.
pub const SEAL: usize = 8;

fn random_bytes(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    // A failing system RNG leaves zeros: the exchange still works, just predictably.
    let _ = getrandom::fill(&mut buf);
    buf
}

/// The game server's cipher: Blowfish under the bootstrap key until the client's key reply
/// arrives, then under the Diffie-Hellman secret.
pub struct GameCipher {
    encrypt: BlowfishCfb,
    decrypt: BlowfishCfb,
    secret: BigUint,
    exchanged: bool,
}

impl GameCipher {
    /// A cipher and the key packet to greet the client with (without its seal).
    pub fn new() -> (Self, Vec<u8>) {
        let p = BigUint::parse_bytes(DH_P.as_bytes(), 16).expect("constant");
        let g = BigUint::parse_bytes(DH_G.as_bytes(), 16).expect("constant");
        let secret = BigUint::from_bytes_be(&random_bytes(32));
        let public = g.modpow(&secret, &p).to_str_radix(16).to_uppercase();
        let cipher = Self {
            encrypt: BlowfishCfb::new(BOOTSTRAP_KEY, [0; 8]),
            decrypt: BlowfishCfb::new(BOOTSTRAP_KEY, [0; 8]),
            secret,
            exchanged: false,
        };
        (cipher, key_packet(&public))
    }

    /// Re-keys from the client's key reply.
    fn exchange(&mut self, reply: &[u8]) {
        let Some(public) = client_public_key(reply) else {
            return;
        };
        let p = BigUint::parse_bytes(DH_P.as_bytes(), 16).expect("constant");
        let key = public.modpow(&self.secret, &p).to_bytes_be();
        self.encrypt = BlowfishCfb::new(&key, IV_CLIENT);
        self.decrypt = BlowfishCfb::new(&key, IV_SERVER);
        self.exchanged = true;
    }
}

impl Cipher for GameCipher {
    fn encrypt(&mut self, data: &mut [u8]) {
        self.encrypt.encrypt(data);
    }

    fn decrypt(&mut self, data: &mut [u8]) {
        self.decrypt.decrypt(data);
    }

    fn received(&mut self, packet: &[u8]) {
        if !self.exchanged {
            self.exchange(packet);
        }
    }
}

/// `[len: i32][bytes]`.
fn put_field(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as i32).to_le_bytes());
    out.extend_from_slice(bytes);
}

/// The server's key packet, less the seal the framing adds.
fn key_packet(public_hex: &str) -> Vec<u8> {
    let mut body = Vec::new();
    put_field(&mut body, &random_bytes(JUNK));
    put_field(&mut body, &IV_CLIENT);
    put_field(&mut body, &IV_SERVER);
    put_field(&mut body, DH_P.as_bytes());
    put_field(&mut body, DH_G.as_bytes());
    put_field(&mut body, public_hex.as_bytes());
    let mut out = random_bytes(SERVER_PAD);
    // The length counts what follows the padding, seal included.
    out.extend_from_slice(&((4 + body.len() + SEAL) as i32).to_le_bytes());
    out.extend(body);
    out
}

fn i32_at(bytes: &[u8], at: usize) -> Option<usize> {
    let v = i32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?);
    usize::try_from(v).ok()
}

/// The client's public key in its key reply.
fn client_public_key(reply: &[u8]) -> Option<BigUint> {
    let junk = i32_at(reply, CLIENT_PAD + 4)?;
    let at = CLIENT_PAD + 8 + junk;
    let len = i32_at(reply, at)?;
    let hex = reply.get(at + 4..at + 4 + len)?;
    BigUint::parse_bytes(hex, 16)
}

/// The client's side of the account cipher: the server's transform undone, both ways.
#[cfg(test)]
#[derive(Default)]
pub(super) struct TqClient {
    sent: u16,
    received: u16,
}

#[cfg(test)]
impl TqClient {
    fn undo(counter: &mut u16, data: &mut [u8]) {
        let (k1, k2) = tq_tables();
        for b in data {
            let v = *b ^ k2[(*counter >> 8) as usize] ^ k1[(*counter & 0xFF) as usize];
            *b = v.rotate_right(4) ^ 0xAB;
            *counter = counter.wrapping_add(1);
        }
    }

    pub fn encrypt(&mut self, data: &mut [u8]) {
        Self::undo(&mut self.sent, data);
    }

    pub fn decrypt(&mut self, data: &mut [u8]) {
        Self::undo(&mut self.received, data);
    }
}

/// The client's side of the game cipher, as `ConquerLoginHandshake.cpp` does it.
#[cfg(test)]
pub(super) struct ClientCipher {
    encrypt: BlowfishCfb,
    decrypt: BlowfishCfb,
}

#[cfg(test)]
impl ClientCipher {
    pub fn new() -> Self {
        Self {
            encrypt: BlowfishCfb::new(BOOTSTRAP_KEY, [0; 8]),
            decrypt: BlowfishCfb::new(BOOTSTRAP_KEY, [0; 8]),
        }
    }

    pub fn encrypt(&mut self, data: &mut [u8]) {
        self.encrypt.encrypt(data);
    }

    pub fn decrypt(&mut self, data: &mut [u8]) {
        self.decrypt.decrypt(data);
    }

    /// Answers the server's key packet (decrypted, seal included) with the reply to send,
    /// encrypted; from then on both ways are under the shared secret.
    pub fn answer(&mut self, greeting: &[u8]) -> Vec<u8> {
        let mut at = SERVER_PAD + 4;
        let mut fields = Vec::new();
        while at + 4 <= greeting.len() - SEAL {
            let len = i32_at(greeting, at).expect("field length");
            fields.push(greeting[at + 4..at + 4 + len].to_vec());
            at += 4 + len;
        }
        let p = BigUint::parse_bytes(&fields[3], 16).expect("P");
        let g = BigUint::parse_bytes(&fields[4], 16).expect("G");
        let mine = BigUint::from(0x0123_4567_89AB_CDEF_u64);
        let my_public = g.modpow(&mine, &p).to_str_radix(16);
        let server_public = BigUint::parse_bytes(&fields[5], 16).expect("server key");
        let shared = server_public.modpow(&mine, &p).to_bytes_be();
        let mut body = Vec::new();
        put_field(&mut body, &[0; JUNK]);
        put_field(&mut body, my_public.as_bytes());
        body.extend_from_slice(b"TQClient");
        let mut reply = vec![0u8; CLIENT_PAD];
        reply.extend_from_slice(&((4 + body.len()) as i32).to_le_bytes());
        reply.extend(body);
        self.encrypt(&mut reply);
        self.encrypt = BlowfishCfb::new(&shared, IV_SERVER);
        self.decrypt = BlowfishCfb::new(&shared, IV_CLIENT);
        reply
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn tq_cipher_matches_the_client() {
        let mut c = TqCipher::new();
        let mut wire = hex("17840465d713c4a5df0f33a514cb756f5f89b022");
        c.decrypt(&mut wire);
        assert_eq!(
            &wire[..8],
            &[0x34, 0x00, 0x1B, 0x04, b't', b'e', b's', b't']
        );
        assert!(wire[8..].iter().all(|&b| b == 0));
        let mut out = hex("20001F0441420F0002000000");
        c.encrypt(&mut out);
        assert_eq!(out, hex("4748aa121d70cf2eddf0335a"));
        let mut client = TqClient::default();
        client.decrypt(&mut out);
        assert_eq!(out, hex("20001F0441420F0002000000"));
    }

    #[test]
    fn passwords_decrypt() {
        let sealed: [u8; 16] = hex("669ca864ed573e600da6084df367eb73").try_into().unwrap();
        assert_eq!(decrypt_password(&sealed), "test1234");
        let s = rc5_schedule();
        assert_eq!(
            &s[..4],
            &[0xA991_5556, 0x48E4_4110, 0x9F32_308F, 0x27F4_1D3E]
        );
    }

    #[test]
    fn blowfish_cfb_matches_openssl() {
        let mut data: Vec<u8> = (0..32).collect();
        let mut bf = BlowfishCfb::new(BOOTSTRAP_KEY, [0; 8]);
        let (a, b) = data.split_at_mut(5);
        bf.encrypt(a);
        bf.encrypt(b);
        assert_eq!(
            data,
            hex("24760fc778e4e572320f1225ccf12805bf7a38a96026f6e01f1c6f3ecc04f638")
        );
        let mut back = BlowfishCfb::new(BOOTSTRAP_KEY, [0; 8]);
        back.decrypt(&mut data);
        assert_eq!(data, (0..32).collect::<Vec<u8>>());
    }

    #[test]
    fn key_exchange_agrees() {
        let (mut server, mut greeting) = GameCipher::new();
        greeting.extend_from_slice(b"TQServer");
        server.encrypt(&mut greeting);
        let mut client = ClientCipher::new();
        client.decrypt(&mut greeting);
        assert_eq!(
            i32_at(&greeting, SERVER_PAD).unwrap() + SERVER_PAD,
            greeting.len()
        );
        assert_eq!(&greeting[greeting.len() - 8..], b"TQServer");
        let mut reply = client.answer(&greeting);
        let mut first = b"hello game".to_vec();
        client.encrypt(&mut first);
        // Server: the reply under the old key, the packet after it under the new one.
        server.decrypt(&mut reply);
        server.received(&reply);
        server.decrypt(&mut first);
        assert_eq!(first, b"hello game");
        let mut answer = b"welcome".to_vec();
        server.encrypt(&mut answer);
        client.decrypt(&mut answer);
        assert_eq!(answer, b"welcome");
    }
}
