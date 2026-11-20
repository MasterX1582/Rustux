//! net — minimal IPv4/ICMP wire layer.
//!
//! Everything parsed here is untrusted wire data (device-DMA'd
//! buffers): every field is validated before use and every derived
//! bound is checked — the "don't port the bugs" rule applied at the
//! trust boundary the moment it appears.
//!
//! Deliberate scope cuts:
//! - No IP fragment reassembly queue — fragments are dropped. Frag
//!   queues are a recurring CVE class (e.g. GSO-state leaks into
//!   reassembly); reassembly needs its own design + audit, not an
//!   incidental implementation.
//! - No IP options handling — IHL > 5 is accepted as header bytes but
//!   the options themselves are never interpreted.

/// Internet checksum (RFC 1071): one's-complement sum of the 16-bit
/// words, end-around carry, complemented. A buffer whose checksum
/// field is already correct folds to 0.
pub fn csum16(buf: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let (words, rem) = buf.as_chunks::<2>();
    for w in words {
        sum += u16::from_be_bytes(*w) as u32;
    }
    if let Some(&b) = rem.first() {
        sum += (b as u32) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// A validated IPv4 packet inside a complete frame payload (the bytes
/// after the Ethernet header).
pub struct Ipv4<'a> {
    pub src: [u8; 4],
    pub dst: [u8; 4],
    pub proto: u8,
    /// Payload bytes bounded by total_len — never extends past `buf`.
    pub payload: &'a [u8],
}

/// Parse and validate an IPv4 datagram. Returns None for anything
/// that isn't a complete, unfragmented, well-formed v4 packet:
/// version/IHL/total_len bounds, header checksum, no fragments.
pub fn parse_ipv4(buf: &[u8]) -> Option<Ipv4<'_>> {
    if buf.len() < 20 {
        return None;
    }
    if buf[0] >> 4 != 4 {
        return None;
    }
    let ihl = (buf[0] & 0x0f) as usize * 4;
    if ihl < 20 || ihl > buf.len() {
        return None;
    }
    let total = u16::from_be_bytes([buf[2], buf[3]]) as usize;
    if total < ihl || total > buf.len() {
        return None;
    }
    // Drop fragments: MF set or nonzero offset.
    if u16::from_be_bytes([buf[6], buf[7]]) & 0x3fff != 0 {
        return None;
    }
    // Correct header incl. its checksum field folds to 0.
    if csum16(&buf[..ihl]) != 0 {
        return None;
    }
    let mut src = [0u8; 4];
    src.copy_from_slice(&buf[12..16]);
    let mut dst = [0u8; 4];
    dst.copy_from_slice(&buf[16..20]);
    Some(Ipv4 {
        src,
        dst,
        proto: buf[9],
        payload: &buf[ihl..total],
    })
}

/// A parsed ICMP message (the IPv4 payload).
pub struct Icmp<'a> {
    pub ty: u8,
    pub code: u8,
    pub id: u16,
    pub seq: u16,
    pub rest: &'a [u8],
}

/// Parse and checksum-validate an ICMP message.
pub fn parse_icmp(buf: &[u8]) -> Option<Icmp<'_>> {
    if buf.len() < 8 || csum16(buf) != 0 {
        return None;
    }
    Some(Icmp {
        ty: buf[0],
        code: buf[1],
        id: u16::from_be_bytes([buf[4], buf[5]]),
        seq: u16::from_be_bytes([buf[6], buf[7]]),
        rest: &buf[8..],
    })
}

/// An L2+L3 address pair — one endpoint of a frame.
pub struct NetAddr {
    pub mac: [u8; 6],
    pub ip: [u8; 4],
}

// ---- IPv8 (draft-thain-ipv8, experimental reference impl) ----
//
// Wire-specified so far: version=8 header = IPv4 layout + four 32-bit
// address fields (SrcRN, SrcLocal, DstRN, DstLocal), IHL>=7 (28B),
// EtherType 0x8080 for frames carrying explicit Route Numbers.
// NOT specified: ARP8 / ICMPv8 formats (draft-thain-support8 was
// never submitted to the datatracker). Until that spec exists, our
// echo body is plain ICMPv4 format — a first-implementation choice.

/// EtherType for native IPv8 frames (draft §11.2 / ETH_P_IPV8).
pub const ET_IPV8: u16 = 0x8080;

/// A validated IPv8 datagram (draft-thain-ipv8-03 §7).
pub struct Ipv8<'a> {
    pub src_rn: u32,
    pub src: [u8; 4],
    pub dst_rn: u32,
    pub dst: [u8; 4],
    pub proto: u8,
    pub payload: &'a [u8],
}

/// Parse and validate an IPv8 datagram — the same discipline as
/// parse_ipv4: bounds first, checksum, no fragments.
pub fn parse_ipv8(buf: &[u8]) -> Option<Ipv8<'_>> {
    if buf.len() < 28 {
        return None;
    }
    if buf[0] >> 4 != 8 {
        return None;
    }
    let ihl = (buf[0] & 0x0f) as usize * 4;
    if ihl < 28 || ihl > buf.len() {
        return None;
    }
    let total = u16::from_be_bytes([buf[2], buf[3]]) as usize;
    if total < ihl || total > buf.len() {
        return None;
    }
    if u16::from_be_bytes([buf[6], buf[7]]) & 0x3fff != 0 {
        return None;
    }
    if csum16(&buf[..ihl]) != 0 {
        return None;
    }
    let mut src = [0u8; 4];
    src.copy_from_slice(&buf[16..20]);
    let mut dst = [0u8; 4];
    dst.copy_from_slice(&buf[24..28]);
    Some(Ipv8 {
        src_rn: u32::from_be_bytes([buf[12], buf[13], buf[14], buf[15]]),
        src,
        dst_rn: u32::from_be_bytes([buf[20], buf[21], buf[22], buf[23]]),
        dst,
        proto: buf[9],
        payload: &buf[ihl..total],
    })
}

/// A full IPv8 address: 32-bit Route Number + 32-bit local (IPv4-form)
/// address.
pub struct Addr8 {
    pub rn: u32,
    pub local: [u8; 4],
}

/// Build Ethernet + IPv8 + ICMP echo message into `f`
/// (`ty` = 8 request, 0 reply). Returns frame length, 0 if `f` is too small.
pub fn build_ipv8_echo(
    f: &mut [u8],
    ty: u8,
    dst_mac: [u8; 6],
    src_mac: [u8; 6],
    dst: &Addr8,
    src: &Addr8,
    id: u16,
    seq: u16,
    payload: &[u8],
) -> usize {
    let total = 28 + 8 + payload.len(); // IPv8 total length
    if f.len() < 14 + total {
        return 0;
    }
    f[..6].copy_from_slice(&dst_mac);
    f[6..12].copy_from_slice(&src_mac);
    f[12..14].copy_from_slice(&ET_IPV8.to_be_bytes());

    let ip = 14;
    f[ip] = 0x87; // v8, IHL 7
    f[ip + 1] = 0;
    f[ip + 2..ip + 4].copy_from_slice(&(total as u16).to_be_bytes());
    f[ip + 4..ip + 6].copy_from_slice(&1u16.to_be_bytes());
    f[ip + 6..ip + 8].copy_from_slice(&0x4000u16.to_be_bytes()); // DF
    f[ip + 8] = 64;
    f[ip + 9] = 1;
    f[ip + 10..ip + 12].copy_from_slice(&[0, 0]);
    f[ip + 12..ip + 16].copy_from_slice(&src.rn.to_be_bytes());
    f[ip + 16..ip + 20].copy_from_slice(&src.local);
    f[ip + 20..ip + 24].copy_from_slice(&dst.rn.to_be_bytes());
    f[ip + 24..ip + 28].copy_from_slice(&dst.local);
    let ck = csum16(&f[ip..ip + 28]);
    f[ip + 10..ip + 12].copy_from_slice(&ck.to_be_bytes());

    let ic = ip + 28;
    f[ic] = ty;
    f[ic + 1] = 0;
    f[ic + 2..ic + 4].copy_from_slice(&[0, 0]);
    f[ic + 4..ic + 6].copy_from_slice(&id.to_be_bytes());
    f[ic + 6..ic + 8].copy_from_slice(&seq.to_be_bytes());
    f[ic + 8..ic + 8 + payload.len()].copy_from_slice(payload);
    let ck = csum16(&f[ic..ic + 8 + payload.len()]);
    f[ic + 2..ic + 4].copy_from_slice(&ck.to_be_bytes());

    14 + total
}

/// Build Ethernet + IPv4 + ICMPv4 echo-request into `f`.
/// Returns the frame length on success, 0 if `f` is too small.
pub fn build_echo_request(
    f: &mut [u8],
    dst: &NetAddr,
    src: &NetAddr,
    id: u16,
    seq: u16,
    payload: &[u8],
) -> usize {
    let total = 20 + 8 + payload.len(); // IP total length
    if f.len() < 14 + total {
        return 0;
    }
    f[..6].copy_from_slice(&dst.mac);
    f[6..12].copy_from_slice(&src.mac);
    f[12..14].copy_from_slice(&[0x08, 0x00]);

    let ip = 14;
    f[ip] = 0x45; // v4, IHL 5
    f[ip + 1] = 0;
    f[ip + 2..ip + 4].copy_from_slice(&(total as u16).to_be_bytes());
    f[ip + 4..ip + 6].copy_from_slice(&1u16.to_be_bytes()); // id
    f[ip + 6..ip + 8].copy_from_slice(&0x4000u16.to_be_bytes()); // DF
    f[ip + 8] = 64; // TTL
    f[ip + 9] = 1; // proto ICMP
    f[ip + 10..ip + 12].copy_from_slice(&[0, 0]);
    f[ip + 12..ip + 16].copy_from_slice(&src.ip);
    f[ip + 16..ip + 20].copy_from_slice(&dst.ip);
    let ck = csum16(&f[ip..ip + 20]);
    f[ip + 10..ip + 12].copy_from_slice(&ck.to_be_bytes());

    let ic = ip + 20;
    f[ic] = 8; // echo request
    f[ic + 1] = 0;
    f[ic + 2..ic + 4].copy_from_slice(&[0, 0]);
    f[ic + 4..ic + 6].copy_from_slice(&id.to_be_bytes());
    f[ic + 6..ic + 8].copy_from_slice(&seq.to_be_bytes());
    f[ic + 8..ic + 8 + payload.len()].copy_from_slice(payload);
    let ck = csum16(&f[ic..ic + 8 + payload.len()]);
    f[ic + 2..ic + 4].copy_from_slice(&ck.to_be_bytes());

    14 + total
}
