# Conquer Online patch 5065: wire protocol spec for a minimal server front

Sources (read only):

- **Client**: `D:\Conquer2\Tools\eosource\cosource`, the reconstructed 5065 client source. It is written to match `Conquer.exe` and cites native addresses. Paths below are relative to `cosource\`. The client is authoritative for what the client sends and expects.
- **Redux**: `D:\Conquer2\Tools\eosource\Redux 3.05\Redux`, the working C# 5065 server. Paths below are relative to `Redux\`.
- **Original client install** (data files only): `D:\Conquer2\Tools\eosource\client5065` (`ini\StrRes.ini`, `Server.dat`, `res.dat`).
- **Original TQ AccountServer** (older generation, used only to confirm the error reply style): `D:\Conquer2\Tools\eosource\AccountServer\LoginThread.cpp:823-835`.

Conventions:

- All integers are **little-endian**.
- Offsets are from the start of the packet, header included.
- "u16/u32/u64" are unsigned sizes; "i16/i32" are signed.
- `L:` cites a line number.

---

## A. Transport and cipher

### A.1 Framing

Every packet starts with the same 4-byte header:

| off | size | field |
|---|---|---|
| 0 | u16 | `size`: length of the packet **including** this 4-byte header |
| 2 | u16 | `type`: message id |

- Max packet size is 1024 (`_MAX_MSGSIZE`, `3drole/Network/NetworkDef.h:5`). The client rejects larger packets and also rejects a `size` below 4.
- Several packets may share one TCP read. The client loops on `size` (`3drole/Network/NetMsg.cpp:48-123`).

**Account (login) connection: no seal.**

- The client sends `size` bytes and reads exactly `size` bytes (`3drole/Network/Socket.h:456-476` and `:550-615`).
- On the account socket the client calls `exit(5)` on a bad size (`Socket.h:559-568`).

**Game connection: 8-byte ASCII seal after every packet** (after the key exchange is done).

- `size` does **not** count the seal, so the bytes on the wire are `size + 8`.
- Client to server ends with `"TQClient"`: the client appends it after `size` bytes (`Socket.h:440-452`, with `nWireSize = nSize + 8`).
- Server to client must end with `"TQServer"`. The client checks the seal at `wire[size]` and **disconnects** on a mismatch, or on `size < 4` / `size > 1024` (`Socket.h:986-1020`).
- Redux does the same:
  - It allocates `body + 8`, writes `size = total - 8` (`Packets/PacketBuilder.cs:14-18`) and copies `"TQServer"` into the last 8 bytes (`Objects/Player.cs:2115-2122`, `Common.cs:19`).
  - It splits incoming data with `size + 8` (`Network/GameServer.cs:85`).
- Redux's splitter (`GameServer.cs:80-107`) has a bug: when several packets share one buffer, the last one is processed from offset 0. Do not copy that.

### A.2 Account-server cipher (the "TQ XOR" cipher, `CConquerCipher`)

- This is a byte-stream cipher with **independent send and receive counters**. Both counters reset to 0 when the TCP connection is created (`Socket.h:342-345`).
- Key tables: two 256-byte tables generated with `k1[0]=0x9D`, `k2[0]=0x62` (`3drole/Network/ConquerCipher.cpp:5-12`):
  ```
  k1[i+1] = (u8)((0x0F + k1[i]*0xFA) * k1[i] + 0x13)
  k2[i+1] = (u8)((0x79 - k2[i]*0x5C) * k2[i] + 0x6D)
  ```
  (This is also the template `CGameSocket<0xFA,0x0F,0x13,0x9D, 0xA4,0x79,0x6D,0x62>`, `Socket.h:1026`, with `EncryptClient.h:41-58`.)
- Redux hard-codes the identical tables: `Cryptography/AuthCryptographer.cs:54-71` (Key1) and `:74-91` (Key2). I regenerated them and they match.

KEY1 (`k1`):
```
0x9D,0x90,0x83,0x8A,0xD1,0x8C,0xE7,0xF6,0x25,0x28,0xEB,0x82,0x99,0x64,0x8F,0x2E,
0x2D,0x40,0xD3,0xFA,0xE1,0xBC,0xB7,0xE6,0xB5,0xD8,0x3B,0xF2,0xA9,0x94,0x5F,0x1E,
0xBD,0xF0,0x23,0x6A,0xF1,0xEC,0x87,0xD6,0x45,0x88,0x8B,0x62,0xB9,0xC4,0x2F,0x0E,
0x4D,0xA0,0x73,0xDA,0x01,0x1C,0x57,0xC6,0xD5,0x38,0xDB,0xD2,0xC9,0xF4,0xFF,0xFE,
0xDD,0x50,0xC3,0x4A,0x11,0x4C,0x27,0xB6,0x65,0xE8,0x2B,0x42,0xD9,0x24,0xCF,0xEE,
0x6D,0x00,0x13,0xBA,0x21,0x7C,0xF7,0xA6,0xF5,0x98,0x7B,0xB2,0xE9,0x54,0x9F,0xDE,
0xFD,0xB0,0x63,0x2A,0x31,0xAC,0xC7,0x96,0x85,0x48,0xCB,0x22,0xF9,0x84,0x6F,0xCE,
0x8D,0x60,0xB3,0x9A,0x41,0xDC,0x97,0x86,0x15,0xF8,0x1B,0x92,0x09,0xB4,0x3F,0xBE,
0x1D,0x10,0x03,0x0A,0x51,0x0C,0x67,0x76,0xA5,0xA8,0x6B,0x02,0x19,0xE4,0x0F,0xAE,
0xAD,0xC0,0x53,0x7A,0x61,0x3C,0x37,0x66,0x35,0x58,0xBB,0x72,0x29,0x14,0xDF,0x9E,
0x3D,0x70,0xA3,0xEA,0x71,0x6C,0x07,0x56,0xC5,0x08,0x0B,0xE2,0x39,0x44,0xAF,0x8E,
0xCD,0x20,0xF3,0x5A,0x81,0x9C,0xD7,0x46,0x55,0xB8,0x5B,0x52,0x49,0x74,0x7F,0x7E,
0x5D,0xD0,0x43,0xCA,0x91,0xCC,0xA7,0x36,0xE5,0x68,0xAB,0xC2,0x59,0xA4,0x4F,0x6E,
0xED,0x80,0x93,0x3A,0xA1,0xFC,0x77,0x26,0x75,0x18,0xFB,0x32,0x69,0xD4,0x1F,0x5E,
0x7D,0x30,0xE3,0xAA,0xB1,0x2C,0x47,0x16,0x05,0xC8,0x4B,0xA2,0x79,0x04,0xEF,0x4E,
0x0D,0xE0,0x33,0x1A,0xC1,0x5C,0x17,0x06,0x95,0x78,0x9B,0x12,0x89,0x34,0xBF,0x3E
```
KEY2 (`k2`):
```
0x62,0x4F,0xE8,0x15,0xDE,0xEB,0x04,0x91,0x1A,0xC7,0xE0,0x4D,0x16,0xE3,0x7C,0x49,
0xD2,0x3F,0xD8,0x85,0x4E,0xDB,0xF4,0x01,0x8A,0xB7,0xD0,0xBD,0x86,0xD3,0x6C,0xB9,
0x42,0x2F,0xC8,0xF5,0xBE,0xCB,0xE4,0x71,0xFA,0xA7,0xC0,0x2D,0xF6,0xC3,0x5C,0x29,
0xB2,0x1F,0xB8,0x65,0x2E,0xBB,0xD4,0xE1,0x6A,0x97,0xB0,0x9D,0x66,0xB3,0x4C,0x99,
0x22,0x0F,0xA8,0xD5,0x9E,0xAB,0xC4,0x51,0xDA,0x87,0xA0,0x0D,0xD6,0xA3,0x3C,0x09,
0x92,0xFF,0x98,0x45,0x0E,0x9B,0xB4,0xC1,0x4A,0x77,0x90,0x7D,0x46,0x93,0x2C,0x79,
0x02,0xEF,0x88,0xB5,0x7E,0x8B,0xA4,0x31,0xBA,0x67,0x80,0xED,0xB6,0x83,0x1C,0xE9,
0x72,0xDF,0x78,0x25,0xEE,0x7B,0x94,0xA1,0x2A,0x57,0x70,0x5D,0x26,0x73,0x0C,0x59,
0xE2,0xCF,0x68,0x95,0x5E,0x6B,0x84,0x11,0x9A,0x47,0x60,0xCD,0x96,0x63,0xFC,0xC9,
0x52,0xBF,0x58,0x05,0xCE,0x5B,0x74,0x81,0x0A,0x37,0x50,0x3D,0x06,0x53,0xEC,0x39,
0xC2,0xAF,0x48,0x75,0x3E,0x4B,0x64,0xF1,0x7A,0x27,0x40,0xAD,0x76,0x43,0xDC,0xA9,
0x32,0x9F,0x38,0xE5,0xAE,0x3B,0x54,0x61,0xEA,0x17,0x30,0x1D,0xE6,0x33,0xCC,0x19,
0xA2,0x8F,0x28,0x55,0x1E,0x2B,0x44,0xD1,0x5A,0x07,0x20,0x8D,0x56,0x23,0xBC,0x89,
0x12,0x7F,0x18,0xC5,0x8E,0x1B,0x34,0x41,0xCA,0xF7,0x10,0xFD,0xC6,0x13,0xAC,0xF9,
0x82,0x6F,0x08,0x35,0xFE,0x0B,0x24,0xB1,0x3A,0xE7,0x00,0x6D,0x36,0x03,0x9C,0x69,
0xF2,0x5F,0xF8,0xA5,0x6E,0xFB,0x14,0x21,0xAA,0xD7,0xF0,0xDD,0xA6,0xF3,0x8C,0xD9
```

**Client transform.** The client uses this one transform for both its send and its receive stream (`ConquerCipher.cpp:23-49`, used at `Socket.h:474` (send) and `:689` (recv)). `c` is a u16 counter per direction (low byte = `pos1`, high byte = `pos2`):
```
v = in[i] ^ k1[c & 0xFF] ^ k2[c >> 8];  v = (v << 4 | v >> 4) & 0xFF;  out[i] = v ^ 0xAB;  c++
```

**Server side.** Use the inverse in both directions, each direction with its own u16 counter starting at 0 (`AuthCryptographer.cs:8-21` decrypt, `:38-51` encrypt; the formula is identical):
```
v = in[i] ^ 0xAB;  v = (v << 4 | v >> 4) & 0xFF;  v ^= k2[c >> 8];  v ^= k1[c & 0xFF];  out[i] = v;  c++
```
The server's decrypt inverts the client's transform. The server's encrypt yields bytes that the client's transform turns back into plaintext.

**Test vectors.** I computed these with `scratchpad\vectors.py` and checked the round trip.

- Client to server, counter 0. Plain `34 00 1B 04 74 65 73 74` + 12 zero bytes (header 52/1051 + "test"):
  - wire = `17840465d713c4a5df0f33a514cb756f5f89b022`
- Server to client, counter 0. Plain `20 00 1F 04 41 42 0F 00 02 00 00 00` (32/1055, uid 1000001, data 2):
  - wire = `4748aa121d70cf2eddf0335a`

`ChangeCode(dw)` re-keys these tables: `k1` dwords `^= dw`, `k2` dwords `^= dw*dw` (`ConquerCipher.cpp:51-61`). The 5065 client calls it on the *game* socket after `MsgLoginReplyEx` with `dwData ^ (dwData + idAccount) ^ 0x4321` (`MsgLoginReplyEx.cpp:95-98`). The game socket does not use this cipher (it is in Blowfish mode, see A.3), so the call has **no effect on the wire**. Ignore it.

### A.3 Game-server cipher: Blowfish-CFB64 with a Diffie-Hellman key exchange

Client code is in `3drole/Network/ConquerLoginHandshake.cpp`; Redux code is in `Cryptography/BlowfishExchange.cs`, `Cryptography/GameCryptographer.cs` and `Objects/Player.cs:2160-2187`.

**Cipher.** Standard Blowfish (16 rounds, OpenSSL/libeay `BF_set_key` and `BF_cfb64_encrypt` semantics) in CFB-64 stream mode.

- There is one cipher state per direction: an 8-byte IV plus a byte position `num` from 0 to 7 (`third_party/cops_v7/blowfish.cpp:270-322`).
- Encrypt one byte:
  ```
  if num == 0: IV = BF_encrypt_block(IV)      // standard big-endian Blowfish block
  c = p ^ IV[num];  IV[num] = c;  num = (num + 1) & 7
  ```
- Decrypt one byte:
  ```
  if num == 0: IV = BF_encrypt_block(IV)
  p = c ^ IV[num];  IV[num] = c;  num = (num + 1) & 7
  ```
- The state **persists across packets**. Never reset it per packet.
- **Key length.** Up to 72 bytes are used (`blowfish.h:35`: `KEY_SIZE = 72 // TQ uses libeay which uses 576 bits keys`). The DH shared key is up to 64 bytes and is used whole.
  - RustCrypto `blowfish` (`new_from_slice`) and pycryptodome both reject keys longer than 56 bytes. Use a Blowfish key schedule that accepts the full key (OpenSSL behaviour: cycle the key bytes over P[0..17]), for example a small hand-written Blowfish.
- **Bootstrap key** = ASCII `"DR654dt34trg4UI6"` (16 bytes), with both IVs zero and `num = 0` (client `ConquerLoginHandshake.cpp:14` and `:58-67`; Redux `Common.cs:18`, `Player.cs:504`).
- Test vector (bootstrap key, IV 0). Encrypting bytes `00 01 .. 1F` as one stream (split 5+27 bytes, same result) gives:
  ```
  24760fc778e4e572320f1225ccf12805bf7a38a96026f6e01f1c6f3ecc04f638
  ```

**Handshake order.** No other packet may be interleaved before step 4.

1. TCP connect. The **server speaks first**: it sends the *server key packet*, encrypted with the bootstrap cipher (Redux `GameServer.cs:58-63` and `Player.cs:2160-2163`, which calls `DirectSend` and so encrypts).
   - The client has already queued its `MsgConnect 1052` and holds it until the handshake is ready (`Socket.h:429-438`).
   - Once the handshake is running, the client decrypts **every received byte** with whatever key is current (`Socket.h:686-687`). Any server bytes sent after the key packet but before the client's reply are therefore garbled.
2. The client decrypts the key packet with the bootstrap key and validates it (`ConquerLoginHandshake.cpp:121-170`). It then builds its reply, **encrypts the reply with the bootstrap key** (continuing its encrypt stream, `:198`) and sends it. Right after that it re-keys (step 4).
3. The server decrypts the reply with the bootstrap decrypt stream and extracts the client's public key.
   - Redux reads it at fixed offsets (`Player.cs:2165-2186`).
   - Decrypt only the reply's own bytes (7 + bodyLen) with the old key. CFB is byte-wise, so decrypt 11 bytes, read `bodyLen` at offset 7, then decrypt `7 + bodyLen - 11` more bytes. Bytes after that (the client's first `1052`) belong to the **new** key.
   - Redux decrypts the whole receive buffer with the old key and drops any trailing bytes. That works only because TCP usually delivers the 1052 in a separate segment.
4. Both sides re-key: Blowfish key = the shared secret bytes. Both IVs are taken from the key packet and both `num` are set to 0.
   - Client: `generateKey(shared)` then `setIVs(enc = IV#2, dec = IV#1)` (`:204-206`). `setIVs` zeroes `num` (`blowfish.cpp:325-339`).
   - Redux: `SetKey(shared)`, then `EncryptIV = clientIV (IV#1)` and `DecryptIV = serverIV (IV#2)` (`BlowfishExchange.cs:28-33`).
   - So the server **encrypts with IV#1 and decrypts with IV#2**. Redux sends both IVs as zero (`BlowfishExchange.cs:19-20`).
5. From then on every packet in either direction is `body + seal`, encrypted with the new streams. The client sends the held `1052` first (`Socket.h:967-977`).

**Server key packet (S to C).** Built in `BlowfishExchange.cs:35-98`; parsed in `ConquerLoginHandshake.cpp:121-170`.

| off | size | field |
|---|---|---|
| 0 | 11 | random padding (`PAD_LENGTH = 11`) |
| 11 | i32 | `bodyLen` = total - 11 (it **includes** the 8-byte seal). The client requires 12 <= bodyLen <= 4096 and `total = 11 + bodyLen` (`:136-140`). |
| 15 | i32 | `junkLen` (Redux: 12) |
| 19 | junkLen | random junk |
| .. | i32 + 8 | IV#1 length (must be 8) + IV#1 ("clientIV"; the client calls it `encryptIv`) |
| .. | i32 + 8 | IV#2 length (must be 8) + IV#2 ("serverIV"; the client calls it `decryptIv`) |
| .. | i32 + n | P as an ASCII hex string (no NUL), n = 128 |
| .. | i32 + n | G as ASCII hex, `"05"` |
| .. | i32 + n | server public key `g^x mod P` as ASCII hex (Redux: OpenSSL `BN_bn2hex`, upper case) |
| total-8 | 8 | `"TQServer"`. The client checks it at `total - 8` (`:141-146`). |

- Total size = 28 + 11 + 12 + 8 + 8 + 128 + 2 + len(pub) + 8 (`BlowfishExchange.cs:45`).
- Every length field must be >= 1 and fit in the packet (`ConquerLoginHandshake.cpp:82-96`).
- P (512-bit) and G are at `BlowfishExchange.cs:7-8`:
  ```
  P = E7A69EBDF105F2A6BBDEAD7E798F76A209AD73FB466431E2E7352ED262F8C558F10BEFEA977DE9E21DCEE9B04D245F300ECCBBA03E72630556D011023F9E857F
  G = 05
  ```

**Client key reply (C to S).** Built at `ConquerLoginHandshake.cpp:185-198`; read by Redux at `Player.cs:2170-2186`.

| off | size | field |
|---|---|---|
| 0 | 7 | random padding (`CLIENT_PADDING_LENGTH = 7`) |
| 7 | i32 | `bodyLen` = total - 7 (includes the seal) |
| 11 | i32 | `junkLen` = 12 |
| 15 | 12 | random junk |
| 27 | i32 | `pubLen` |
| 31 | pubLen | client public key `g^b mod P`, ASCII hex (no NUL) |
| 31+pubLen | 8 | `"TQClient"` |

- In general the public key is at `15 + junkLen` (length) and `19 + junkLen` (bytes), which is how Redux reads it.
- Redux treats any received buffer longer than 36 bytes as the reply.
- Parse the hex case-insensitively.

**Shared key.** `s = clientPub^x mod P`.

- Client: `getSharedKey()` returns `s` as a hex string (`third_party/cops_v7/diffiehellman.cpp:91-97`). `HexToBytes` prepends `'0'` when the length is odd (`ConquerLoginHandshake.cpp:98-119`).
- So the key is the **minimal big-endian bytes of `s` (no leading zero bytes)**, the same as OpenSSL `DH_compute_key` / `BN_bn2bin` used by Redux (`BlowfishExchange.cs:30-31`).
- The client's private exponent is a random 256-bit prime (`diffiehellman.cpp:68`). The server can use any random private exponent; Redux uses OpenSSL `DH_generate_key`.

**Server side after the handshake.**

- Encrypt every outgoing `body + "TQServer"` with the server encrypt stream (Redux `Player.cs:2115-2122`).
- Decrypt incoming bytes with the decrypt stream, then split by `size + 8` and verify `"TQClient"` (Redux does not verify it).

### A.4 Password in the login packet (RC5-32/12/16)

The client copies the typed password into a zeroed 16-byte buffer and RC5-encrypts the 16 bytes, i.e. two 8-byte blocks (`3drole/GameMain.cpp:118-126`).

- Key (16 bytes) (`GameMain.cpp:23`; the same bytes in Redux `Constants.cs:96-100`):
  ```
  3C DC FE E8 C4 54 D6 7E 16 A6 F8 1A E8 D0 38 BE
  ```
- Parameters: w = 32, r = 12, b = 16. `Pw = 0xB7E15163`, `Qw = 0x9E3779B9`. `S` has 26 words, `L` has 4 words (`3drole/RC5_321216.cpp:16-47`; Redux `Cryptography/PasswordCryptography.cs:171-204` and `Constants.cs:101-107`).
- Key schedule:
  ```
  L = key as 4 LE u32;  S[0] = Pw;  S[i] = S[i-1] + Qw
  i = j = x = y = 0
  repeat 78 times:
      S[i] = rol(S[i] + x + y, 3);  x = S[i];  i = (i + 1) % 26
      L[j] = rol(L[j] + x + y, (x + y) % 32);  y = L[j];  j = (j + 1) % 4
  ```
- Encrypt each 8-byte block (A, B = LE u32):
  ```
  A += S[0];  B += S[1]
  for r in 1..=12:  A = rol(A ^ B, B % 32) + S[2r];  B = rol(B ^ A, A % 32) + S[2r+1]
  ```
- The server decrypts (`PasswordCryptography.cs:232-256`):
  ```
  for r in 12..=1:  B = ror(B - S[2r+1], A % 32) ^ A;  A = ror(A - S[2r], B % 32) ^ B
  B -= S[1];  A -= S[0]
  ```
  The result is the NUL-padded plaintext password.
- Test vectors: `S[0..3] = A9915556 48E44110 9F32308F 27F41D3E`; `"test1234"` padded to 16 bytes encrypts to `669ca864ed573e600da6084df367eb73`.
- There is **no seed packet in 5065**. Redux has a `PasswordSeedPacket` (`Packets/Login/[1056]PasswordSeed.cs`) but never sends it, and the client never waits for one.
- Client limits: account <= 15 characters, password <= 14 characters, neither empty (`3drole/Network/ConquerLoginLayouts.cpp:53-60`).

### A.5 Ports

| | client source | Redux |
|---|---|---|
| Account server | `_PORT_ACCOUNTSERVER = 9958` (`NetworkDef.h:11`). The original client's `Server.dat` lists `Port1=9958` per server, and the IP also comes from `Server.dat`. | default 9958 (`Database/Readers/SettingsReader.cs:21`; `Constants.cs:78` has a stale 9959) |
| Game server | `_PORT_GAMESERVER = 5816` (`NetworkDef.h:12`). The 1055 reply carries IP and port. | 5816 (`SettingsReader.cs:20`) |

**Reconstruction-only restriction.** The source client rejects a 1055 whose IP is not local or whose port is not 5816 ("Rejected non-local game server redirect.", `MsgLoginReplyEx.cpp:67-73`). The original `Conquer.exe` uses the IP and port from the packet. For safety, use 5816 and a local or LAN IP.

---

## B. Login sequence

### B.1 Account server (port 9958; A.2 cipher, no seal)

1. **C to S: `MsgAccount` 1051**, size 52 = 0x34. Layout: `ConquerLoginLayouts.h:15-21` and `:56-59`; built at `MsgAccount.cpp:22-47` and `GameMain.cpp:128-134`.

   | off | size | field |
   |---|---|---|
   | 0 | u16 | 52 |
   | 2 | u16 | 1051 |
   | 4 | char[16] | account, NUL-terminated |
   | 20 | u8[16] | password, RC5-encrypted (A.4) |
   | 36 | char[16] | server name (from `Server.dat` `ServerNameN`, e.g. "Dark") |

   Redux parses it at `Packets/Login/[1086]Account.cs:58-66` and dispatches type 1051 at `Network/LoginServer.cs:56`. Any other type disconnects.

2. **S to C: `MsgLoginReplyEx` 1055**, size 32 = 0x20. Layout: `ConquerLoginLayouts.h:41-48` and `:62-64`; client `MsgLoginReplyEx.cpp:31-108`; Redux `Packets/Login/[1055]AuthResponsePacket.cs:43-55` (no seal: buffer 32, size field 32).

   | off | size | field |
   |---|---|---|
   | 0 | u16 | 32 |
   | 2 | u16 | 1055 |
   | 4 | u32 | `idAccount`: login token, **non-zero on success**. The client sends it back in the game `1052`. |
   | 8 | u32 | `dwData`: authentication value on success (Redux sends 2, `RESPONSE_VALID`); **error code on failure** |
   | 12 | char[16] | game server IP as a dotted string, NUL-terminated (<= 15 characters) |
   | 28 | u32 | game server port (5816) |

   **Failure:** `idAccount = 0`, `dwData` = code. The client shows `StrRes 100099` = "Failed to login: %s" with the string for the code (`MsgLoginReplyEx.cpp:31-63`, strings in `client5065\ini\StrRes.ini:254-317`):

   | dwData | StrRes id | text | Redux use |
   |---|---|---|---|
   | 1 | 10418 | Invalid account or password! | `RESPONSE_INVALID`: bad password or unknown account |
   | 10 | 10417 | The server is down! | |
   | 11 | 10419 | Please login later! | account already online; Redux kicks the online char (`LoginServer.cs:85-90`, `:128`) |
   | 12 | 10438 | This account has been banned. | `RESPONSE_BANNED` |
   | 20, 21 | 10480 | This server is full... | |
   | 22 | 10440 | Your account is locked... | |
   | other | 10439 | Unknown Error | |

   Redux logic: `Network/LoginServer.cs:92-141`. The original TQ AccountServer also answered errors with `id = ID_NONE, data = type` (`AccountServer/LoginThread.cpp:834-835`). The legacy handler `CMsgConnect::Process` (type 1052, `MsgConnect.cpp:82-171`) uses the same code table.

3. **Client after a successful 1055** (`MsgLoginReplyEx.cpp:75-108`):
   - It sends `MsgConnect 1052` on the **account** socket: `{idAccount, dwData = res.dat number, info = "res.dat"}` (size 28). It then closes that socket.
   - The account server can ignore or close; Redux disconnects on any type other than 1051.
   - The client then opens the game socket (`CNetwork::InitGame` calls `PrepareGameLogin`, `NetMsg.cpp:186-214`), queues the game `1052` and waits for the key exchange (A.3).

### B.2 Game server (port 5816; A.3 cipher with seal)

**C to S first packet: `MsgConnect` 1052**, size 28 = 0x1C. Layout: `ConquerLoginLayouts.h:23-39` and `:57-61`; built at `MsgConnect.cpp:54-68` with call sites `MsgLoginReplyEx.cpp:100-107`; Redux `Packets/Game/[1052] Connect.cs:56-76`.

| off | size | field |
|---|---|---|
| 4 | u32 | `idAccount` = 1055 offset 4 (the token) |
| 8 | u32 | `dwData` = 1055 offset 8 |
| 12 | u16 | client version. The source client sends 123; Redux refuses versions below 123 with Entrance "Invalid version number! Please update the client!" |
| 14 | char[2] | language tag. Redux refuses GBK "中" (`0xD6 0xD0`). |
| 16 | u8[6] | machine (zeros) |
| 22 | u8[2] | reserved |
| 24 | u32 | resource version (the number in `res.dat`; the original client's file holds `0010`) |

**Server handling** (Redux `Network/GameServer.cs:398-461`):

- Look up the account by token. An unknown token disconnects. Redux also rejects a token older than 10 s.
- **The character exists:** load it, then send:
  - MsgTalk on channel **2101 (ENTRANCE)**, words **"ANSWER_OK"** (B.3), and
  - **MsgUserInfo 1006** for the hero, plus inventory and equipment `MsgItemInfo 1008`.

  Order: Redux builds 1006 and the items first (`Player.cs:1378-1440`, `Populate`, queued). It then `DirectSend`s ANSWER_OK, so on the wire ANSWER_OK usually comes **first**. The client accepts either order:
  - ANSWER_OK drives the UI (`MyShell/MyShellDlg.cpp:1309-1314`, `StartGame`).
  - 1006 creates the hero and **triggers the next step**: on processing 1006 the client sends `MsgAction 74` (`MsgUserInfo.cpp:107-110`).

  **Both packets are required.**
- **No character for the account:** send MsgTalk on channel 2101 with words **"NEW_ROLE"**. The client opens the role-creation dialog (`MyShellDlg.cpp:1315-1320`) and stays connected.
- **Other refusals:** any other string on channel 2101 is shown in a MessageBox and the client goes back to the login screen (`MyShellDlg.cpp:1321-1332`). Redux examples: "The server is full. Please log in again later!" and "account locked.".

**Enter-world chain** (client `3drole/Network/MsgAction.cpp:431-590`; server Redux `GameServer.cs:516-579`). Each client request is MsgAction 1010 with `idUser = hero id` and the other fields 0.

| # | client sends (action) | server must answer | client then |
|---|---|---|---|
| 1 | **74** EnterMap / SetLocation (sent after it processes 1006) | **MsgAction 74** with `data@12 = map id` (the *doc* map id used to look up `ini/GameMap.dat`), `x@16`, `y@18`, `dir@20`. `idUser` is ignored; the client overwrites it with the hero id (`MsgAction.cpp:206-209`). Redux then calls `EnterMap`, which sends the surrounding spawns, MsgMapInfo 1110, weather and so on. | Sets `_STATUS_NORMAL`, loads the map with `MYASSERT(g_objGameMap.Create(data))`, places the hero, sends 75, and starts the 10 s MsgItem PING timer (`:431-466`) |
| 2 | **75** GetItemSet | **MsgAction 75** echo `{id, action 75}`. Redux sends items earlier (in Populate). Before the echo it also sends MsgTime 1033, nobility, equip-lock, 1032 and the bonus hint (`:529-539`); all are optional. | Sends 76 (`:546-554`) |
| 3 | **76** GetGoodFriend | MsgAction 76 echo. Friends (MsgFriend 1019) go before it; optional. | Sends 77 |
| 4 | **77** GetWeaponSkillSet | MsgAction 77 echo, plus MsgWeaponSkill 1025 entries (optional) | Sends 78 |
| 5 | **78** GetMagicSet | MsgAction 78 echo, plus MsgMagicInfo 1103 entries (optional) | Sends 97; sets stamina to 100 |
| 6 | **97** GetSynAttr | MsgAction 97 echo. Redux also sends MsgSynAttrInfo 1106 and the guild info. | Sends **130** (`:582-590`) |
| 7 | **130** (client name `actionMentorRequest`; Redux `CompleteLogin`) | **no reply needed** (Redux sends optional extras, `GameServer.cs:930-961`) | the player is in game |

The echo format is Redux's `LoginReply` (`GameServer.cs:858-861`): `MsgAction{idUser = hero id, data = 0, action = same}`. Redux enforces the order 74, 75, 76, 77, 78, 97 (`Objects/Player.Connection.cs:416-428`).

**New-character path:**

1. **C to S: `MsgRegister` 1001**, size 60 = 0x3C, on the same game connection (`MsgRegister.h:19-45`; sent from `MyShell/DlgRoleCreate.cpp:344-364`; Redux `Packets/Game/[1001] Register.cs:55-65`).

   | off | size | field |
   |---|---|---|
   | 4 | char[16] | account name |
   | 20 | char[16] | character name |
   | 36 | char[16] | password (the dialog string) |
   | 52 | u16 | body (`look`): 1003 / 1004 male, 2001 / 2002 female (`(sel >= 4 ? 2000 : 1002) + body(1..2)`, `MsgRegister.h:59-66`) |
   | 54 | u16 | profession: 10 Trojan, 20 Warrior, 40 Archer, 100 Taoist (`MsgRegister.h:52-57`) |
   | 56 | u32 | account identity (the token from 1055) |

2. **S to C:** MsgTalk on channel **2100 (REGISTER)**:
   - Success: "ANSWER_OK". The client waits 10 s, closes the connection and returns to the login screen, so the user logs in again and the char now exists (`MyShellDlg.cpp:1337-1376`).
   - Error: any other string is shown in a MessageBox and the creation dialog reopens. Redux strings: "Invalid character name!" and "This character name has already existed!" (`GameServer.cs:201-225`).
   - Redux validates the body (1003, 1004, 2001, 2002) and profession (10, 20, 40, 100 and 30) (`Common.cs:41-42`).

### B.3 Strings in packets

The client string packer (`3DBaseCode/NetStringPacker.cpp:28-63`) and Redux (`Packets/NetStringPacker.cs:27-39`, `:105-130`) use:
```
u8 count, then count x (u8 len, len bytes of text, no NUL)
```
Fixed-size string fields elsewhere are NUL-padded `char[16]`.

---

## C. Packet layouts

Sizes are the `size` field, i.e. without the seal.

### MsgTalk 1004 (`MsgTalk.h:83-93`; Redux `Packets/Game/[1004]Talk.cs:100-127`)

| off | size | field |
|---|---|---|
| 4 | u32 | colour, 0x00RRGGBB (Redux default 0x00FFFFFF; system yellow 0xFFFF00; red 0xFF0000) |
| 8 | u16 | channel (`unTxtAttribute`) |
| 10 | u16 | style: 0 normal, 1 scroll, 2 flash, 8 blast (`NetworkDef.h:128-131`) |
| 12 | u32 | time. The client puts `SysTimeGet()`; Redux puts `hh*100 + mm`. |
| 16 | u32 | the client calls it `dwSenderLook`; Redux calls it `HearerLookface` |
| 20 | u32 | the client calls it `dwReceiverLook`; Redux calls it `SpeakerLookface` |
| 24 | packer | 4 strings: speaker, hearer, emotion, words |

- Size = 24 + packer length. Redux adds 3 zero pad bytes; the client accepts any size whose header matches (`MsgTalk.cpp:364-375`).
- The two lookface fields are named the opposite way in the two sources. Only whispers use them (Redux sets the speaker lookface for whispers). Zeros are safe.
- Words are at most 255 characters (the client buffer is 256).
- Channels (`NetworkDef.h:134-179`; Redux `Enum/ChatType.cs`):

  | value | name |
  |---|---|
  | 2000 | Normal; the client turns it into 2007 Talk (`MsgTalk.cpp:997-999`) |
  | 2001 | Private / whisper (hearer = target name) |
  | 2002 | Action |
  | 2003 | Team |
  | 2004 | Syndicate |
  | 2005 | System |
  | 2007 | Talk (local) |
  | 2009 | Friend |
  | 2010 | Global |
  | 2011 | GM |
  | 2012 | Whisper (alt) |
  | 2013 | Ghost |
  | 2014 | Serve |
  | 2100 | **Register** (`g_objRegister`) |
  | 2101 | **Entrance** (login answers: ANSWER_OK, NEW_ROLE, errors) |
  | 2102 | Shop |
  | 2105 | Webpage |
  | 2110 | LeaveWord |
  | 2111 | SynAnnounce |
  | 2112 | MessageBox |
  | 2115 | Trace |
  | 2500 | Pigeon / broadcast |

- Server messages: speaker "SYSTEM", hearer "ALLUSERS" or the player's name, emotion "" (`Constants.cs:86-91`). The words for login answers: "ANSWER_OK", "NEW_ROLE" (`MyShell/MyShellDlg.cpp:47-48`).
- C to S chat: the speaker must equal the sender's name. Redux relays channel 2007 to the screen and 2001 to the named player (`GameServer.cs:248-375`).

### MsgUserInfo 1006 (S to C, hero only) (`MsgUserInfoLayout.h:11-61`; Redux `Packets/Game/[1006]HeroInformation.cs:70-104`)

| off | size | client field | Redux writes |
|---|---|---|---|
| 4 | u32 | identity (hero id, >= 1000000) | UID |
| 8 | u32 | lookFace = face*10000 + body (body 1003/1004/2001/2002). The client uses `%1000` = model, `/1000%10` = sex, `/10000%1000` = face (`MsgUserInfo.cpp:76-79`). | Lookface |
| 12 | u16 | hair (style %1000, colour /1000) | Hair |
| 14 | u32 | money | Money |
| 18 | u32 | conquerPoints | CP |
| 22 | u32 | experience. The client reads a u32; Redux writes a u64 at 22. | Experience (u64, high dword at 26) |
| 26 | u32 | unknown1A | (exp high) |
| 30 | u32 | unknown1E | Deed (i32) |
| 34 | u32 | unknown22 | Medal |
| 38 | u32 | unknown26 | MedalSelect |
| 42 | u32 | unknown2A ("goes to a hero field nothing reads", `MsgUserInfo.cpp:97-100`) | Virtue |
| 46 | u32 | **nobilityPacked**: the client does `SetNobilityRank(v / 100000)` (`:88`) | **MeteLev**. They disagree. Send 0 (or rank*100000) for a minimal server. |
| 50 | u16 | strength | |
| 52 | u16 | agility | |
| 54 | u16 | vitality | |
| 56 | u16 | spirit | |
| 58 | u16 | additional (unspent) points | |
| 60 | u16 | life | |
| 62 | u16 | mana | |
| 64 | i16 | pk points | |
| 66 | u8 | level | |
| 67 | u8 | profession | |
| 68 | u8 | **metempsychosis** (first rebirth) | Redux writes NobilityRecord here. They disagree; the client is authoritative. |
| 69 | u8 | **secondMetempsychosis** | Redux writes Reborn here |
| 70 | u8 | autoAllot | AutoAllot |
| 71 | packer | 2 strings: name, spouse ("None") | |

- Size = 71 + packer.
- Max life and max mana are not in 1006. Send them with MsgUserAttrib 1 / 3 if the bars must be right.

### MsgItemInfo 1008 (S to C) (`MsgItemInfo.h:39-60`; Redux `Packets/Game/[1008] ItemInformation.cs:91-117`)

Size 44 = 0x2C.

| off | size | field |
|---|---|---|
| 4 | u32 | item unique id (or owner id for action 4) |
| 8 | u32 | item type (itemtype id) |
| 12 | u16 | amount / durability |
| 14 | u16 | amount limit / max durability |
| 16 | u8 | action: 1 ADDITEM, 2 TRADE, 3 UPDATE, 4 OTHERPLAYER_EQUIPMENT, 5 PACKAGE (`MsgItemInfo.h:16-21`) |
| 17 | u8 | status (ident flags; Redux `Ident`) |
| 18 | u8 | position: 0 backpack, 1 helmet, 2 necklace, 3 armor, 4 weaponR, 5 weaponL, 6 ring, 7 treasure / talisman, 8 shoes / boots, 9 mount / garment (Redux 9 = Garment), 201 warehouse ... (`3drole/Item.h:31-51`; Redux `Enum/ItemLocation.cs`) |
| 19 | u8 | reserved |
| 20 | u32 | socket progress (Redux `Data`) |
| 24 | u8 | gem1 |
| 25 | u8 | gem2 |
| 26 | u8 | magic1 (effect) |
| 27 | u8 | magic2 |
| 28 | u8 | magic3 = **plus** |
| 29 | u8 | bless |
| 30 | u8 | enchant |
| 31 | u8 | reserved |
| 32 | u32 | dwData (Redux anti-monster) |
| 36 | u16 | suspicious (Redux plunder) |
| 38 | u16 | lockStatus (Redux special flag) |
| 40 | u16 | colour |
| 42 | u8[2] | reserved |

### MsgItem 1009 (`MsgItem.h:14-100`; Redux `Packets/Game/[1009] ItemActionPacket.cs:24-48`)

Size 24 = 0x18.

| off | size | field |
|---|---|---|
| 4 | u32 | id (item id, npc id, ...) |
| 8 | u32 | data (for drops: low u16 = y, high u16 = x) |
| 12 | u16 | action |
| 14 | u16 | reserved |
| 16 | u32 | verification / timestamp |
| 20 | u32 | reserved (Redux "Amount") |

Actions (client names, `MsgItem.h:16-46`):

| action | name | direction and meaning |
|---|---|---|
| 1 | BUY | C to S: id = npc, data = itemtype |
| 2 | SELL | C to S: id = npc, data = item id |
| 3 | DROP | C to S: id = item, data = y \| x << 16. S to C: delete the item from the bag (`MsgItem.cpp:160-166`; Redux "RemoveInventory", `Player.cs:1533`). |
| 4 | USE | C to S: id = item, data = target position (0 = use, 1..9 = equip slot); Redux `ItemUse.OnUseItem(uid = id, data)` |
| 5 | EQUIP | S to C: id = item, data = slot; the item moves from the bag to the slot (`MsgItem.cpp:175-202`, Redux `EquipmentManager.cs:183`) |
| 6 | UNEQUIP | C to S: id = item, data = slot. S to C: id = item, data = slot; the slot moves back to the bag (`MsgItem.cpp:203-231`, Redux `EquipmentManager.cs:311`) |
| 7 / 8 | split / combine | |
| 9 / 10 / 11 | warehouse money query / save / draw (id = npc, data = money) | |
| 12 | DROPMONEY | C to S: data = amount, x / y as for DROP (Redux `GameServer.cs:1512`) |
| 14 / 15 | repair / repair-all | |
| 16 | ident | |
| 17 | DURABILITY | S to C: id = position, data = durability |
| 18 | DROPEQUIPMENT | S to C: id = item, data = position |
| 25 | SYNCHRO_AMOUNT | S to C: id = item, data = amount |
| 27 | **PING** | C to S every 10 s after EnterMap (`Hero.cpp:356-364`), `verification = timeGetTime ^ heroId`. The server **echoes the packet unchanged** (Redux `GameServer.cs:1279`); the client computes the ping from it (`MsgItem.cpp:336-346`). No disconnect if unanswered. |

### MsgAction 1010 (`MsgAction.h:22-58`; Redux `Packets/Game/[1010] GeneralData.cs:86-111`)

Size 24 = 0x18.

| off | size | field |
|---|---|---|
| 4 | u32 | timestamp (the client sends `TimeGet()`) |
| 8 | u32 | idUser |
| 12 | u32 | data (or target x = low u16 at 12, target y = high u16 at 14) |
| 16 | u16 | x |
| 18 | u16 | y |
| 20 | u16 | direction |
| 22 | u16 | action |

Redux names these fields Data1 = 12, Data2 = 16 (Low = x, High = y) and Data3 = 20. The full action list is in `MsgAction.h:85-160`.

| action | client name | sent by | meaning / server reply |
|---|---|---|---|
| 74 | EnterMap | C to S, S to C | B.2 step 1 |
| 75, 76, 77, 78, 97 | Get{ItemSet, GoodFriend, WeaponSkillSet, MagicSet, SynAttr} | C to S, echo S to C | B.2 |
| 130 | MentorRequest | C to S | end of login; no reply |
| 79 | ChgDir | C to S: dir@20, x / y = position (`Hero.cpp:999`) | broadcast to others (Redux: `SendToScreen` without self, `GameServer.cs:582`). The client applies `SetDir(dir)`. |
| 81 | Emotion | C to S: data = pose (1..8, 150..271; Redux whitelist `GameServer.cs:479`), dir | broadcast to the screen **including the sender** (`GameServer.cs:1006-1023`). Cool pose 230: Redux ORs `(prof [+0x100]) << 16` into data. |
| 85 | ChgMap (use portal) | C to S: data = portal cell x \| y << 16, x / y = hero (`Hero.cpp:3318`) | the server teleports the player with action 86 (Redux `UsePortal`, `GameServer.cs:1059`) |
| 86 | FlyMap (teleport) | S to C: `idUser` **must be the hero id** (MYASSERT, `MsgAction.cpp:470`); data = map doc id, x, y, dir | the client loads the map and positions the hero (`MsgAction.cpp:468-525`, Redux `Player.cs:1779-1815`). Follow it with fresh spawns. |
| 88 | Fireworks | | room broadcast |
| 92 | Uplev | S to C | level-up effect |
| 93 | XpClear | C to S | |
| 94 | Reborn (revive) | C to S: data = mode (0 = at the revive point; 1 = here, only with a blessing) | the server revives the player: life = max (attr 0), clears status dead / ghost (attr 26), restores the lookface, then action 86 to the revive point (Redux `Player.cs:1226-1247`) |
| 96 | SetPkMode | C to S: data 0..3 | Redux echoes the packet and sends a system message (`GameServer.cs:637`) |
| 99 | Mine | | |
| 100 | PositionHeartbeat | S to C challenge / C to S answer | optional anti-cheat (C.E) |
| 102 | QueryPlayer | C to S: data = unknown id | the client sends it whenever it gets a walk / jump / action for an id it does not know (`MsgAction.cpp:211-231`, `MsgWalk.cpp`). **The server replies with that entity's spawn packet (1014)** (`GameServer.cs:663`). |
| 106 | QueryTeamMember | | |
| 108 | KickBack | S to C: position correction (data low / high and x / y = cell) | |
| 114 | DestroyBooth | | Redux calls 114 "GetSurroundings". The 5065 client has no such request; 114 is DestroyBooth. The server must push spawns itself. |
| 116 | PostCmd | S to C | the client posts UI command `data` (e.g. open a window) |
| 120 | Landing | | |
| 126 | OpenDialog | S to C: data = dialog id | 1 compose, 4 warehouse, ... (`MsgAction.cpp:724-750`) |
| 131 | MonsterAction | S to C | |
| 132 | **LeaveMap** | S to C: idUser = entity id | **despawns a player / monster / NPC actor** (`MsgAction.cpp:527-543`; Redux `Monster.cs:134`, `Npc.cs:61`). A dead NPC-range actor gets the "BodyDisapear" effect. |
| 133 | Jump | C to S: data = target x (low) / y (high), x / y = current cell, dir, timestamp (`Hero.cpp:2606`) | broadcast to the screen **including the sender** (Redux `Player.cs:1913`); the client sets the hero's align position from data (`MsgAction.cpp:400-429`) |
| 135 | SetPose | | |
| 137 | Ghost | C to S, after death | the server makes the player a ghost (status 0x400 plus the ghost lookface) |
| 138 | Synchro | S to C / ack C to S | |
| 139 | BurstXp / VerifyData | | |
| 140, 141, 142, 143 | friend info / leave word / change face / business | | |
| 310 | QueryChatterEquipment | | |

### MsgWalk 1005 (`MsgWalk.h:28-35`; Redux `Packets/Game/[1005] Walk.cs`)

Size 12 (Redux sends 20 with the seal, so the size field is 12).

| off | size | field |
|---|---|---|
| 4 | u32 | idUser |
| 8 | u8 | dir (0..7; the client applies `%8`) |
| 9 | u8 | run (0 walk, 1 run) |
| 10 | u8[2] | reserved |

- Direction deltas: `dx = {0,-1,-1,-1,0,1,1,1}`, `dy = {1,1,0,-1,-1,-1,0,1}` (`MsgAction.cpp:296-297`, Redux `Common.cs:21-22`).
- C to S: one step.
- The server must **echo it to the sender** as well as broadcast it (Redux `SendToScreen(packet, true)`, `GameServer.cs:1373`). For the hero the echo is what commits the logical cell (`MsgWalk.cpp`, the hero branch). For other players it queues walk / run animations. An unknown id makes the client send action 102.

### MsgPlayer 1014: spawn of players **and monsters** (`MsgPlayer.h:26-69`; Redux `Packets/Game/[1014] SpawnEntity.cs:117-169`)

| off | size | field |
|---|---|---|
| 4 | u32 | id |
| 8 | u32 | lookFace: `%1000` = model / mesh; `/1000%10` = sex; `/10000%1000` = face; `/10000000` = transform (`MsgPlayer.cpp:72-200`) |
| 12 | u32 | status[0] (flags: 0x20 dead, 0x400 ghost, 0x40 team leader, 0x1 blue name, 0x4000 red, 0x8000 black, ...; Redux `Enum/ClientEffect.cs`) |
| 16 | u32 | status[1] |
| 20 | u32 | synId (low 24 bits) \| rank << 24 |
| 24 | u32 | garment type |
| 28 | u32 | armet (helmet) type |
| 32 | u32 | armor type |
| 36 | u32 | weaponR type |
| 40 | u32 | weaponL type |
| 44 | u16 | right-weapon restrain (players; Redux anti-monster) |
| 46 | u16 | left-weapon restrain |
| 48 | u16 / u32 | **monsters: u16 life @48, u16 level @50**. Players: the client also reads life from 48 (`MsgPlayer.cpp:145`); Redux writes medal_select (u32) here. |
| 52 | u16 | x |
| 54 | u16 | y |
| 56 | u16 | hair |
| 58 | u8 | dir |
| 59 | u8 | pose / action (100 = stand, 250 = sit, ...) |
| 60 | u8 | metempsychosis (rebirth count) |
| 61 | u8 | reserved |
| 62 | u16 | player level (used for players) |
| 64 | u8 | dummy flag (1 = an "inspect" copy, not added to the world) |
| 65 | u8[3] | reserved |
| 68 | u32 | nobility battle power / rank |
| 72 | i32 | mentor battle power |
| 76 | u32 | aura / team leader id |
| 80 | u32 | aura / team member count |
| 84 | u16 | armet colour |
| 86 | u16 | armor colour |
| 88 | u16 | left-weapon (shield) colour |
| 90 | packer | 1 string: name |
| 90+len | u8 | trailing pad |

- Size = 90 + packer + 2. The client struct is 0x5C with a 1-byte `szBuf` and a trailing byte; Redux allocates `100 + packer` with the seal, which gives size = 92 + packer.
- **Monsters:** id 400001..599999 (`id < 1000000`):
  - The client takes level and life from 48 / 50.
  - It looks the **name** up in the client monster table (`g_obj3DRoleData.GetMonsterInfo(szName)`, built from `ini/Monster.dat`) to get max life, size, armet / weapons and mount (`MsgPlayer.cpp:99-131`). **The name must match a `Monster.dat` entry**, or the monster renders bare and has no max HP.
  - The model is `lookface % 1000` (Redux sends the cq_monstertype `lookface` / mesh).
  - The equipment fields are ignored for monsters.
- A packet with the hero's own id is ignored (`MsgPlayer.cpp:52-56`).
- Dead body: status bit 0x20 makes the actor play the body pose (`MsgPlayer.cpp:205-230`).

### MsgName / Strings 1015 (`MsgName.h:68-81`; Redux `Packets/Game/[1015] Strings.cs:38-46`)

- Layout: `@4 u32 idTarget / data (or x / y)`, `@8 u8 type`, `@9 packer`.
- Types (`MsgName.h:16-51`): 1 fireworks, 6 mate, 9 map effect, 10 role effect, 13 / 14 wanted queries, 16 query mate, 20 sound, ...
- **Not needed for a minimal server.** Names travel in 1014, 1006 and 2030.

### MsgUserAttrib 1017 (`MsgUserAttrib.h:20-109`; Redux `Packets/Game/[1017] UpdatePacket.cs:77-89`)

| off | size | field |
|---|---|---|
| 4 | u32 | idUser |
| 8 | u32 | count |
| 12 + 12k | u32 | type |
| 16 + 12k | u64 | value (low dword at 16 + 12k) |

- Size = 12 + 12*count. Redux always sends size 36: one entry plus one zero entry. Maximum 84 entries.
- Attribute ids (client enum `MsgUserAttrib.h:24-68`; Redux `Enum/UpdateType.cs`):

  | id | attribute |
  |---|---|
  | 0 | life |
  | 1 | max life |
  | 2 | mana |
  | 3 | max mana |
  | 4 | money |
  | 5 | exp (u64) |
  | 6 | pk |
  | 7 | profession |
  | 8 | size-add / extended status |
  | 9 | stamina (PP) |
  | 10 | warehouse money |
  | 11 | add points |
  | 12 | lookface |
  | 13 | level |
  | 14 | spirit |
  | 15 | vitality |
  | 16 | strength |
  | 17 | agility |
  | 18 | bless time |
  | 19 | double-exp time |
  | 20 | guild donation |
  | 21 | curse time |
  | 22 | KO order |
  | 23 | metempsychosis |
  | 24 | exploit |
  | 25 | reserved |
  | **26** | **status flags (word 0)**; Redux names it `StatusEffects` |
  | 27 | hair |
  | 28 | xp |
  | 29 | lucky time |
  | 30 | CP / "global" emoney |
  | 32 | online training |
  | 37 | mentor BP |
  | 38 | nobility rank |
  | 39 | merchant |
  | 128 | exp-ball usage |

- Any visible actor id works: the client updates the role found by id (`MsgUserAttrib.cpp:272-460`).

### MsgInteract 1022 (`MsgInteract.h:14-105`; Redux `Packets/Game/[1022] InteractPacket.cs:43-73`)

Size 32 = 0x20.

| off | size | field |
|---|---|---|
| 4 | u32 | timestamp |
| 8 | u32 | sender |
| 12 | u32 | target |
| 16 | u16 | x |
| 18 | u16 | y |
| 20 | u16 | type |
| 22 | u16 | reserved |
| 24 | u32 | data (damage; or u16 magic type @24 + u16 magic level @26) |
| 28 | i32 | battle effect (Redux `Data2`) |

- Types: 1 steal, **2 attack** (melee), 3 heal, 4 poison, 5 assassinate, 6 freeze, 7 unfreeze, 8 court, 9 marry, 10 divorce, 11 present money, 12 present item, 13 send flowers, **14 kill**, 15 join syn, ... 19 query info, 20 rush attack, **21 magic attack**, 22 abort magic, 23 reflect weapon, 24 bump, **25 shoot** (bow), 26 reflect magic, 27..35 value / CP / business ops (`MsgInteract.h:14-49`).
- **C to S attack:** type 2 (or 25 for a bow) with the target.
- **C to S magic (21):** type, target, x and y are **scrambled** with the sender id. Decode them before use:
  ```
  magicType = ((rotl16(magicType ^ uid ^ 0x915D, 16-3)) + 0x14BE) & 0xFFFF
  magicLevel = (u8)magicLevel ^ 0x21
  target = (rotl32(target, 13) ^ uid ^ 0x5F2D2463) + 0x8B90B51A
  x = (rotl16(x ^ uid ^ 0x2ED6, 16-1) + 0xDD12) & 0xFFFF
  y = (rotl16(y ^ uid ^ 0xB99B, 16-5) + 0x76DE) & 0xFFFF
  ```
  Sources: Redux `InteractPacket.cs:87-94`, client macros `MsgInteract.h:111-117`. `ExchangeShortBits` / `ExchangeLongBits` are rotate-left.
- **S to C hit:** broadcast type 2 (or 25) `{sender = attacker, target, x/y = target cell, data = damage}` to the screen including the attacker (Redux `CombatManager.cs:518`), then MsgUserAttrib life for the target.
- **S to C death:** type 14 KILL `{sender = killer, target = victim, x, y, data = die type (1)}` to the screen including self (Redux `Entity.cs:105-112`). Set status 0x20 on the victim. For monsters, later send action 132 to remove the body (Redux `Monster.cs:113-143`).

### MsgMapItem 1101: ground items and money (`MsgMapItem.h:16-50`; Redux `Packets/Game/[1101] GroundItem.cs:37-52`)

Size 20.

| off | size | field |
|---|---|---|
| 4 | u32 | map item id (Redux uses its own id space) |
| 8 | u32 | itemtype id. Money uses the money-pile ids: amount < 11 = 1090000, < 101 = 1090010, < 1001 = 1090020, < 2001 = 1091000, <= 5000 = 1091010, else 1091020 (Redux `Objects/GroundItem.cs:72-73`). |
| 12 | u16 | x |
| 14 | u16 | y |
| 16 | u16 | extra / look (Redux: colour in the low byte) |
| 18 | u8 | action |
| 19 | u8 | ground addition: `0x80 \| plus` (plus <= 12) on create |

Actions:

- **1 CREATE** (S to C)
- **2 DELETE** (S to C, removes the item)
- **3 PICK**: C to S `{id, x, y}`. S to C `{id = picker id, x, y}` plays the pickup animation.
- 10..13: traps / effects

Client handler: `MsgMapItem.cpp`. Redux handler: `GameServer.cs` `Process_GroundItemPacket`.

### MsgNpcInfo 2030: task-NPC spawn (`MsgNpcInfo.h:29-41`; Redux `Packets/Game/[2030] SpawnNpc.cs:48-71`)

| off | size | field |
|---|---|---|
| 4 | u32 | npc id (1..99999 system NPCs) |
| 8 | u16 | cell x |
| 10 | u16 | cell y |
| 12 | u16 | look = npcTypeKey*10 + dir |
| 14 | u16 | type: 1 shop, **2 task**, 3 storage, 9 statuary, 14 booth, 25/26/102/103 terrain types (`3drole/Role.h:81-129`, Redux `Enum/NpcType.cs`) |
| 16 | u16 | sort |
| 18 | packer | optional name (count 0 = none) |

- Redux sends a fixed buffer of 48 with the seal, i.e. size 40.
- The client deletes and recreates the NPC.
- For non-terrain types it does `MYASSERT(GetNpcTypeInfo(look / 10))`, sets the direction to `look % 10` and the name from the NPC type table, unless the packer has a name (`MsgNpcInfo.cpp`, Process).
- **`look / 10` must exist in the client's NPC type data**, or the assert fires.
- Removal: MsgNpc 2031 action 2 / 3 or MsgAction 132.

### MsgNpc 2031 (`MsgNpc.h:17-55`; Redux `Packets/Game/[2031] Npc.cs`)

Size 16.

| off | size | field |
|---|---|---|
| 4 | u32 | id |
| 8 | u32 | data (x low / y high) |
| 12 | u16 | action |
| 14 | u16 | type |

Actions:

- **0 BEACTIVED**: C to S when the player clicks an NPC (`HeroTask.cpp:43-49`). The server answers with a dialog (2032).
- 1 ADDNPC
- **2 LEAVEMAP**: S to C, removes the NPC
- 3 DELNPC
- 4 CHANGEPOS
- 5 LAYNPC

### MsgTaskDialog / NpcReply 2032 (`MsgTaskDialog.h:17-63`; Redux `Packets/Game/[2032] NpcDialog.cs:37-75`)

| off | size | field |
|---|---|---|
| 4 | u32 | x low / y high (or task id) |
| 8 | u16 | data (face for an avatar; max input length for an edit box) |
| 10 | u8 | index (link-back) |
| 11 | u8 | action |
| 12 | packer | 0 or 1 string |

- Size = 12 + packer. Redux uses 15 + packer, three trailing zeros.
- Client accumulation (`MsgTaskDialog.cpp`, Process; `MyShell/DlgTask.cpp:260-334`):
  - **1 TEXT** appends text. All text lines are concatenated; `~` becomes a space.
  - **2 LINK** adds an option with `index`.
  - **3 EDIT** adds an input box with `index`; data = max length.
  - **4 PIC** sets the portrait face = `data`. Redux uses x = 10, y = 10.
  - **100 CREATE** sets the close index = `index` and **opens the dialog**. Send it last; Redux uses index 255.
- Server dialog = `[PIC face] + TEXT ... + LINK(text, idx) ... + CREATE(255)` (Redux `Npcs/INpc.cs:47-128`).
- **C to S answer:** action **101 ANSWER**, index = the chosen link index, or the close index (255) when closed, or the edit box index plus a string with the typed text (`HeroTask.cpp:30-35`, `MsgTaskDialog.cpp` Create, `DlgTask.cpp:579-603`). The other fields are 0.
- Action 102 TASKID: the interface launches a task (id @4).
- Redux handler: `GameServer.cs:1484-1503`.

### Booths (`BoothManager.cpp`, `Booth.cpp`, `MsgItemInfoEx.h`; TQ booth_trade spec §1)

- The client handles a click on a booth flag (NPC type 16) itself: confirm (StrRes 10364),
  walk two cells out from the flag along its facing, then MsgAction 111 with the cell in
  front (facing 6). The server echoes 111 with `data` = the booth's id; the client then sits.
  MsgAction 114 closes the booth; 115 (`data` a player) asks for its cry.
- The booth is an NPC of its own: MsgNpcInfo 2030 with kind 14, look 400 + facing, the owner's
  name. It leaves with MsgNpc 2031 action 2. Ids sit in the dynamic NPC range 100001-199999.
- MsgItem 1009: 21 asks booth `data`'s goods; 22 / 29 set bag item `id` out at silver / CP
  price `data` (echoed to the owner on success, the client then takes it out of its bag);
  23 takes it back or tells viewers it is gone (`data` the booth); 24 buys `id` from booth
  `data`.
- MsgItemInfoEx 1108 (0x34 bytes): id u32, owner (the booth) u32, price u32, type u32,
  amount u16, limit u16, mode u8 (1 silver, 3 CPs) at 0x18, status u8, position u8, pad,
  socket progress u32, gem1, gem2, magic1, magic2, magic3 (plus), bless, enchant, pad at 0x20,
  data u32 at 0x28, suspicious u16, lock u16, color u16 at 0x30, pad.
- The cry is MsgTalk channel 2104.

### Other login-time packets (optional)

- **MsgWeaponSkill 1025** (`MsgWeaponSkill.h:26-34`): `@4 u32 type`, `@8 u16 level`, `@10 u16 reserved`, `@12 u32 exp`, size **16**. Redux sends size 14 (buffer 22 - 8, `[1025] WeaponProficiency.cs:25-31`), so its exp is truncated. Use 16.
- **MsgMagicInfo 1103** (`MsgMagicInfo.h:25-30`): `@4 u32 exp`, `@8 u16 type`, `@10 u16 level`, size 12.
- **MsgTime 1033**: Redux `[1033] ServerTime.cs:27-37`, size 36.
- **MsgMapInfo 1110**: Redux `[1110] MapStatus.cs`: `@4 uid`, `@8 map id`, `@12 type flags`, size 16.

---

## D. Identifier conventions

Client `3drole/Role.h:30-61`; Redux `[1014] SpawnEntity.cs:114-115`:

| range | meaning |
|---|---|
| 1 .. 99,999 | system / task NPCs (MsgNpcInfo 2030) |
| 100,001 .. 199,999 | dynamic NPCs. 1014 with this id range is drawn as a statuary or terrain NPC. |
| 400,001 .. 499,999 | monsters (`IsMonster` covers 400,001 .. 599,999) |
| 500,001 .. 599,999 | "pet" monsters (action-created) |
| 700,001 .. 799,999 | call pets |
| 900,001 .. 999,999 | traps / magic items |
| **1,000,000 .. 1,999,999,999** | **players** (Redux treats > 999,999 and < 4,000,000,000 as a player) |
| >= 4,000,000,000 | client-reserved |

- Ground items (1101) use their own id namespace (`g_objGameMap` map-item list), separate from roles.
- **Monster look:**
  - `lookface % 1000` is the model / mesh. This is cq_monstertype.lookface, e.g. 101 for a Pheasant.
  - Life and level go at 48 / 50.
  - The **name** selects the client `Monster.dat` record, which supplies max life, size, weapons and so on.
- **Player look:** `face * 10000 + body`, with body 1003 / 1004 (male) or 2001 / 2002 (female); add `transform * 10000000` for a disguise.
- **Map id in SetLocation / FlyMap:** MsgAction `data@12` = the map **document id** looked up in the client's `ini/GameMap.dat` (it must exist, `MYASSERT`). Redux sends `MapInfo.ID` (the cq_map `mapdoc`) there, not the instance id; its `idUser` field carries the dynamic id and is ignored by the client.

---

## E. What the client strictly needs: timeouts, heartbeats, failure modes

- **Game socket: the server must send the key packet right after accept.** The client waits for >= 15 bytes, then for a full, valid key packet with the `"TQServer"` tail (`Socket.h:936-979`). A bad packet just leaves it waiting.
- **Seal and size:** every S to C game packet needs `"TQServer"` after `size` bytes, and 4 <= size <= 1024. Otherwise the client **closes the socket** (`Socket.h:993-1013`).
- **Account socket:** a size of 0 or more than 1024 calls `exit(5)` on the client (`Socket.h:559-568`).
- **Required for entering the world:** ANSWER_OK (2101), 1006, and the six MsgAction echoes 74, 75, 76, 77, 78, 97. The 74 reply must carry a valid map doc id. Without the echoes the client stalls on the loading screen; it has **no retry**.
- **MsgTick 1012** (`MsgTick.h:25-32`):
  - Layout: `@4 u32 idPlayer`, `@8 u32 data`, `@12 u32[4] rand`, `@28 u32 chk`, size 32.
  - The client **echoes** any tick with `data = heroId ^ timeGetTime()` and `chk = (name[1]^0x98) << 8 | (name[0]^0x23)` (or `0x9d4b5703` for names under 4 characters).
  - It only *warns* ("network is congested", `StrRes 100115`) if two ticks are more than 13 s apart (`MsgTick.cpp:56-89`).
  - The **client never disconnects for missing ticks**. Redux sends one every 10 s after login and kicks after 25 s without a reply (`Player.Connection.cs:448-505`); that is server policy, so it is optional for you.
- **No client idle timeout:** `_MAX_IDLETIME = 600` is defined (`NetworkDef.h:7`) but never used.
- **Item PING (MsgItem 27)** every 10 s: echo it for a correct ping display. Optional.
- **MsgAction 100 PositionHeartbeat:** a server-initiated anti-cheat. When it is received, the client answers every 2 / 5 / 8 s depending on `heroId % 10` (`MsgAction.cpp:379-397`). Optional; do not send it.
- **Asserts / soft failures in the client:**
  - unknown map doc id in 74 / 86;
  - NPC look / 10 missing from the NPC type data (2030);
  - action 86 with `idUser != hero`;
  - a 1014 with the hero's own id (ignored);
  - monster names missing from `Monster.dat` (renders bare).
- **Disconnect detection:** the client polls `recv(MSG_PEEK)`. On a closed socket it shows `StrRes 100116` "Failed to connect to the server. Please login again." (`NetMsg.cpp:29-46`).

### Disagreements between the sources (the client is authoritative)

| item | client | Redux |
|---|---|---|
| 1006 @46 | nobility packed (rank*100000) | MeteLev |
| 1006 @68 / @69 | metempsychosis / second metempsychosis | NobilityRecord / Reborn |
| 1006 exp | u32 @22 | u64 @22 (harmless while < 2^32) |
| 1014 @48 for players | life (u16) | medal_select (u32) |
| MsgTalk @16 / @20 | sender look / receiver look | hearer / speaker lookface |
| 1025 size | 16 | 14 (exp truncated) |
| account port | 9958 (`NetworkDef`, `Server.dat`) | `Constants.cs` 9959, but the settings default is 9958 |
| action 114 | DestroyBooth | named "GetSurroundings" |
| 1055 redirect | reconstruction: local IP and port 5816 only | any |
