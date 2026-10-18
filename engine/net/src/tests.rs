use super::packet::{Packet, PacketError, Reader, Writer, packet_type};
use super::*;
use std::time::{Duration, Instant};

/// Waits until `poll` gives something matching `want`.
fn wait_for<T>(mut poll: impl FnMut() -> Vec<T>, want: impl Fn(&T) -> bool) -> T {
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        if let Some(found) = poll().into_iter().find(|e| want(e)) {
            return found;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("timed out");
}

/// XORs with a key that the packet `[0xFF, k]` changes to `k` (stands for TQ's re-keying).
struct Rekeying {
    send: u8,
    recv: u8,
}

impl Cipher for Rekeying {
    fn encrypt(&mut self, data: &mut [u8]) {
        data.iter_mut().for_each(|b| *b ^= self.send);
    }
    fn decrypt(&mut self, data: &mut [u8]) {
        data.iter_mut().for_each(|b| *b ^= self.recv);
    }
    fn received(&mut self, packet: &[u8]) {
        if let [_, _, _, _, 0xFF, k] = packet {
            self.recv = *k;
        }
    }
}

#[test]
fn packets_cross_both_ways_and_close() {
    let server = Server::bind("127.0.0.1:0", Wire::plain).unwrap();
    let client = Client::connect(server.local_addr(), Wire::plain()).unwrap();
    let id = match wait_for(
        || server.poll(),
        |e| matches!(e, NetEvent::Connected { .. }),
    ) {
        NetEvent::Connected { id, .. } => id,
        _ => unreachable!(),
    };
    // Several packets at once arrive whole and in order, big ones too.
    let big = vec![7u8; 100_000];
    client.send(b"hello".to_vec());
    client.send(big.clone());
    client.send(Vec::new());
    let mut got = Vec::new();
    while got.len() < 3 {
        got.extend(server.poll().into_iter().filter_map(|e| match e {
            NetEvent::Received { packet, .. } => Some(packet),
            _ => None,
        }));
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(got, [b"hello".to_vec(), big, Vec::new()]);
    server.send(id, b"welcome".to_vec());
    assert_eq!(
        wait_for(|| client.poll(), |_| true),
        ClientEvent::Received(b"welcome".to_vec())
    );
    server.close(id);
    wait_for(|| client.poll(), |e| matches!(e, ClientEvent::Closed(_)));
    assert!(!client.is_open());
    wait_for(|| server.poll(), |e| matches!(e, NetEvent::Closed { .. }));
    assert_eq!(server.connections(), 0);
}

#[test]
fn ciphers_rekey_exactly_after_the_packet() {
    let wire = || Wire {
        framing: Box::new(LengthPrefixed),
        cipher: Box::new(Rekeying {
            send: 0x5A,
            recv: 0x5A,
        }),
    };
    let server = Server::bind("127.0.0.1:0", wire).unwrap();
    // The client switches its sending key right after sending the re-key packet, and sends
    // the next packet at once: the server must decrypt it with the new key.
    struct ClientSide(u8, bool);
    impl Cipher for ClientSide {
        fn encrypt(&mut self, data: &mut [u8]) {
            data.iter_mut().for_each(|b| *b ^= self.0);
            if self.1 {
                self.0 = 0x33;
                self.1 = false;
            }
        }
        fn decrypt(&mut self, _data: &mut [u8]) {}
    }
    let client = Client::connect(
        server.local_addr(),
        Wire {
            framing: Box::new(LengthPrefixed),
            cipher: Box::new(ClientSide(0x5A, true)),
        },
    )
    .unwrap();
    client.send(vec![0xFF, 0x33]);
    client.send(vec![1, 2, 3]);
    let mut packets = Vec::new();
    let end = Instant::now() + Duration::from_secs(5);
    while packets.len() < 2 && Instant::now() < end {
        packets.extend(server.poll().into_iter().filter_map(|e| match e {
            NetEvent::Received { packet, .. } => Some(packet),
            _ => None,
        }));
    }
    assert_eq!(packets, [vec![0xFF, 0x33], vec![1, 2, 3]]);
}

#[test]
fn oversized_frames_close_the_connection() {
    let server = Server::bind("127.0.0.1:0", Wire::plain).unwrap();
    let mut raw = std::net::TcpStream::connect(server.local_addr()).unwrap();
    use std::io::Write;
    raw.write_all(&u32::MAX.to_le_bytes()).unwrap();
    let closed = wait_for(|| server.poll(), |e| matches!(e, NetEvent::Closed { .. }));
    assert!(matches!(closed, NetEvent::Closed { reason: Some(r), .. } if r.contains("too big")));
}

struct Login {
    account: String,
    key: u32,
}

impl Packet for Login {
    const TYPE: u16 = 1051;
    fn write(&self, w: &mut Writer) {
        w.fixed_str(&self.account, 16).u32(self.key);
    }
    fn read(r: &mut Reader) -> packet::Result<Self> {
        Ok(Self {
            account: r.fixed_str(16)?,
            key: r.u32()?,
        })
    }
}

#[test]
fn fixed_layout_packets_round_trip() {
    let bytes = Login {
        account: "hero".into(),
        key: 7,
    }
    .to_bytes();
    assert_eq!(bytes.len(), 4 + 16 + 4);
    assert_eq!(&bytes[..4], &[24, 0, 0x1B, 0x04]);
    assert_eq!(packet_type(&bytes), Ok(1051));
    let back = Login::from_bytes(&bytes).unwrap();
    assert_eq!((back.account.as_str(), back.key), ("hero", 7));
    assert!(matches!(
        Login::from_bytes(&bytes[..10]),
        Err(PacketError::Short { .. })
    ));
    let mut w = Writer::new();
    w.short_str("abc").i16(-2);
    let b = w.into_bytes();
    let mut r = Reader::new(&b);
    assert_eq!(
        (r.short_str().unwrap(), r.i16().unwrap(), r.remaining()),
        ("abc".into(), -2, 0)
    );
}
