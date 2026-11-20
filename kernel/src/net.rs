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
#[allow(clippy::too_many_arguments)]
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

// ---- IPv6 / NDP ----
//
// Minimal link-local IPv6: fixed 40-byte header (no extension-header
// walking — callers require next==58 directly), EUI-64 link-local
// addressing, ICMPv6 with the pseudo-header checksum, and the two
// NDP messages needed to resolve a peer on-link (NS/NA + SLLA/TLLA).
// Fragments cannot occur: no reassembly exists and extension headers
// are rejected outright.

pub const ET_IPV6: u16 = 0x86dd;
pub const NH_ICMPV6: u8 = 58;

/// A validated IPv6 datagram (fixed header only).
pub struct Ipv6<'a> {
    pub src: [u8; 16],
    pub dst: [u8; 16],
    pub next: u8,
    pub hop: u8,
    pub payload: &'a [u8],
}

pub fn parse_ipv6(buf: &[u8]) -> Option<Ipv6<'_>> {
    if buf.len() < 40 || buf[0] >> 4 != 6 {
        return None;
    }
    let plen = u16::from_be_bytes([buf[4], buf[5]]) as usize;
    if plen > buf.len() - 40 {
        return None;
    }
    let mut src = [0u8; 16];
    src.copy_from_slice(&buf[8..24]);
    let mut dst = [0u8; 16];
    dst.copy_from_slice(&buf[24..40]);
    Some(Ipv6 {
        src,
        dst,
        next: buf[6],
        hop: buf[7],
        payload: &buf[40..40 + plen],
    })
}

/// EUI-64 link-local address from a MAC (u/l bit flipped, fffe insert).
pub fn ll_addr(mac: [u8; 6]) -> [u8; 16] {
    let mut a = [0u8; 16];
    a[0] = 0xfe;
    a[1] = 0x80;
    a[8] = mac[0] ^ 0x02;
    a[9..11].copy_from_slice(&mac[1..3]);
    a[11] = 0xff;
    a[12] = 0xfe;
    a[13..16].copy_from_slice(&mac[3..6]);
    a
}

/// Solicited-node multicast address ff02::1:ffXX:XXXX for `target`.
pub fn solicited_node(target: &[u8; 16]) -> [u8; 16] {
    let mut a = [0u8; 16];
    a[0] = 0xff;
    a[1] = 0x02;
    a[11] = 0x01;
    a[12] = 0xff;
    a[13..16].copy_from_slice(&target[13..16]);
    a
}

/// IPv6 multicast MAC 33:33:XX:XX:XX:XX (low 32 bits of the v6 addr).
pub fn mcast_mac(addr: &[u8; 16]) -> [u8; 6] {
    [0x33, 0x33, addr[12], addr[13], addr[14], addr[15]]
}

/// True if the ICMPv6 checksum residual is valid — one's-complement
/// arithmetic has two zeros; a correct packet can fold to 0xffff.
pub fn csum6_ok(c: u16) -> bool {
    c == 0 || c == 0xffff
}

/// ICMPv6 checksum over pseudo-header + message (RFC 4443).
pub fn csum6(src: &[u8; 16], dst: &[u8; 16], plen: u32, next: u8, msg: &[u8]) -> u16 {
    let mut tail = [0u8; 8];
    tail[..4].copy_from_slice(&plen.to_be_bytes());
    tail[7] = next;
    // Adding one's-complement sums == complement of the total, mod 0xffff.
    let s = csum16(src) as u32
        + csum16(dst) as u32
        + csum16(&tail) as u32
        + csum16(msg) as u32;
    let s = (s & 0xffff) + (s >> 16);
    ((s + (s >> 16)) & 0xffff) as u16
}

/// An ICMPv6 message whose checksum was verified by the caller
/// (csum6 over the IPv6 pseudo-header).
pub struct Icmp6<'a> {
    pub ty: u8,
    pub code: u8,
    pub rest: &'a [u8],
}

pub fn parse_icmp6(msg: &[u8]) -> Option<Icmp6<'_>> {
    if msg.len() < 4 {
        return None;
    }
    Some(Icmp6 {
        ty: msg[0],
        code: msg[1],
        rest: &msg[4..],
    })
}

fn put_eth_v6(f: &mut [u8], dst_mac: [u8; 6], src_mac: [u8; 6]) -> usize {
    f[..6].copy_from_slice(&dst_mac);
    f[6..12].copy_from_slice(&src_mac);
    f[12..14].copy_from_slice(&ET_IPV6.to_be_bytes());
    14
}

fn put_ip6(
    f: &mut [u8],
    at: usize,
    plen: u16,
    hop: u8,
    src: &[u8; 16],
    dst: &[u8; 16],
) {
    f[at] = 0x60;
    f[at + 1..at + 4].copy_from_slice(&[0, 0, 0]);
    f[at + 4..at + 6].copy_from_slice(&plen.to_be_bytes());
    f[at + 6] = NH_ICMPV6;
    f[at + 7] = hop;
    f[at + 8..at + 24].copy_from_slice(src);
    f[at + 24..at + 40].copy_from_slice(dst);
}

fn finish_icmp6(f: &mut [u8], src: &[u8; 16], dst: &[u8; 16], at: usize, plen: usize) {
    let ck = csum6(src, dst, plen as u32, NH_ICMPV6, &f[at..at + plen]);
    f[at + 2..at + 4].copy_from_slice(&ck.to_be_bytes());
}

/// Ethernet + IPv6 + ICMPv6 Neighbor Solicitation (hop 255, SLLA option).
pub fn build_ns(
    f: &mut [u8],
    src_mac: [u8; 6],
    us: &[u8; 16],
    target: &[u8; 16],
) -> usize {
    // eth(14) + ip6(40) + icmp(4+4+16) + slla(8) = 86
    if f.len() < 86 {
        return 0;
    }
    let mc = solicited_node(target);
    let at = put_eth_v6(f, mcast_mac(&mc), src_mac);
    put_ip6(f, at, 32, 255, us, &mc);
    let ic = at + 40;
    f[ic] = 135;
    f[ic + 1] = 0;
    f[ic + 2..ic + 4].copy_from_slice(&[0, 0]);
    f[ic + 4..ic + 8].copy_from_slice(&[0; 4]);
    f[ic + 8..ic + 24].copy_from_slice(target);
    f[ic + 24] = 1; // SLLA option, len 1 (=8B)
    f[ic + 25] = 1;
    f[ic + 26..ic + 32].copy_from_slice(&src_mac);
    finish_icmp6(f, us, &mc, ic, 32);
    86
}

/// Ethernet + IPv6 + ICMPv6 Neighbor Advertisement (S|O flags, TLLA option).
pub fn build_na(
    f: &mut [u8],
    dst_mac: [u8; 6],
    src_mac: [u8; 6],
    us: &[u8; 16],
    dst: &[u8; 16],
) -> usize {
    if f.len() < 86 {
        return 0;
    }
    let at = put_eth_v6(f, dst_mac, src_mac);
    put_ip6(f, at, 32, 255, us, dst);
    let ic = at + 40;
    f[ic] = 136;
    f[ic + 1] = 0;
    f[ic + 2..ic + 4].copy_from_slice(&[0, 0]);
    f[ic + 4..ic + 8].copy_from_slice(&0x6000_0000u32.to_be_bytes()); // S|O
    f[ic + 8..ic + 24].copy_from_slice(us);
    f[ic + 24] = 2; // TLLA option
    f[ic + 25] = 1;
    f[ic + 26..ic + 32].copy_from_slice(&src_mac);
    finish_icmp6(f, us, dst, ic, 32);
    86
}

/// Ethernet + IPv6 + ICMPv6 echo (ty 128 request / 129 reply, hop 64).
#[allow(clippy::too_many_arguments)]
pub fn build_echo6(
    f: &mut [u8],
    ty: u8,
    dst_mac: [u8; 6],
    src_mac: [u8; 6],
    us: &[u8; 16],
    dst: &[u8; 16],
    id: u16,
    seq: u16,
    payload: &[u8],
) -> usize {
    let plen = 8 + payload.len();
    if f.len() < 14 + 40 + plen {
        return 0;
    }
    let at = put_eth_v6(f, dst_mac, src_mac);
    put_ip6(f, at, plen as u16, 64, us, dst);
    let ic = at + 40;
    f[ic] = ty;
    f[ic + 1] = 0;
    f[ic + 2..ic + 4].copy_from_slice(&[0, 0]);
    f[ic + 4..ic + 6].copy_from_slice(&id.to_be_bytes());
    f[ic + 6..ic + 8].copy_from_slice(&seq.to_be_bytes());
    f[ic + 8..ic + 8 + payload.len()].copy_from_slice(payload);
    finish_icmp6(f, us, dst, ic, plen);
    14 + 40 + plen
}

// ---- UDP over IPv4 ----

pub const NH_UDP: u8 = 17;

/// A validated UDP datagram. `cksum_field` is the wire value —
/// in IPv4 a zero field legally means "no checksum"; when nonzero
/// the caller has verified it via csum4 + csum6_ok semantics.
pub struct Udp<'a> {
    pub sport: u16,
    pub dport: u16,
    pub cksum_field: u16,
    pub payload: &'a [u8],
}

/// Parse a UDP datagram: header len >= 8, len field consistent,
/// payload bounded by the UDP length (never past `buf`).
pub fn parse_udp(buf: &[u8]) -> Option<Udp<'_>> {
    if buf.len() < 8 {
        return None;
    }
    let ulen = u16::from_be_bytes([buf[4], buf[5]]) as usize;
    if ulen < 8 || ulen > buf.len() {
        return None;
    }
    Some(Udp {
        sport: u16::from_be_bytes([buf[0], buf[1]]),
        dport: u16::from_be_bytes([buf[2], buf[3]]),
        cksum_field: u16::from_be_bytes([buf[6], buf[7]]),
        payload: &buf[8..ulen],
    })
}

/// UDP-over-IPv4 checksum: pseudo-header (src,dst,zero,proto,len)
/// plus the datagram. Same negative-zero residual caveat as csum6
/// — validate with csum6_ok, or accept field == 0 (v4 legal).
pub fn csum4(src: [u8; 4], dst: [u8; 4], proto: u8, udp: &[u8]) -> u16 {
    let mut tail = [0u8; 4];
    tail[1] = proto;
    tail[2..4].copy_from_slice(&(udp.len() as u16).to_be_bytes());
    let s = csum16(&src) as u32
        + csum16(&dst) as u32
        + csum16(&tail) as u32
        + csum16(udp) as u32;
    let s = (s & 0xffff) + (s >> 16);
    ((s + (s >> 16)) & 0xffff) as u16
}

/// Ethernet + IPv4 + UDP datagram into `f`; returns frame length.
/// UDP checksum is computed (not the RFC-768 zero form).
#[allow(clippy::too_many_arguments)]
pub fn build_udp4(
    f: &mut [u8],
    dst_mac: [u8; 6],
    src_mac: [u8; 6],
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    sport: u16,
    dport: u16,
    payload: &[u8],
) -> usize {
    let ulen = 8 + payload.len();
    let total = 20 + ulen;
    if f.len() < 14 + total {
        return 0;
    }
    f[..6].copy_from_slice(&dst_mac);
    f[6..12].copy_from_slice(&src_mac);
    f[12..14].copy_from_slice(&0x0800u16.to_be_bytes());

    let ip = 14;
    f[ip] = 0x45;
    f[ip + 1] = 0;
    f[ip + 2..ip + 4].copy_from_slice(&(total as u16).to_be_bytes());
    f[ip + 4..ip + 6].copy_from_slice(&2u16.to_be_bytes());
    f[ip + 6..ip + 8].copy_from_slice(&[0, 0]);
    f[ip + 8] = 64;
    f[ip + 9] = NH_UDP;
    f[ip + 10..ip + 12].copy_from_slice(&[0, 0]);
    f[ip + 12..ip + 16].copy_from_slice(&src_ip);
    f[ip + 16..ip + 20].copy_from_slice(&dst_ip);
    let ck = csum16(&f[ip..ip + 20]);
    f[ip + 10..ip + 12].copy_from_slice(&ck.to_be_bytes());

    let u = ip + 20;
    f[u..u + 2].copy_from_slice(&sport.to_be_bytes());
    f[u + 2..u + 4].copy_from_slice(&dport.to_be_bytes());
    f[u + 4..u + 6].copy_from_slice(&(ulen as u16).to_be_bytes());
    f[u + 6..u + 8].copy_from_slice(&[0, 0]);
    f[u + 8..u + 8 + payload.len()].copy_from_slice(payload);
    let ck = csum4(src_ip, dst_ip, NH_UDP, &f[u..u + ulen]);
    f[u + 6..u + 8].copy_from_slice(&ck.to_be_bytes());

    14 + total
}

// ---- TCP over IPv4 (minimal: no retransmit, no out-of-order queue) ----

pub const NH_TCP: u8 = 6;

pub const TCP_FIN: u8 = 0x01;
pub const TCP_SYN: u8 = 0x02;
pub const TCP_RST: u8 = 0x04;
pub const TCP_PSH: u8 = 0x08;
pub const TCP_ACK: u8 = 0x10;

/// A parsed TCP segment. `options` is the bounded region between the
/// fixed header and the payload (per the data-offset field); callers
/// walk it tag-by-tag or ignore it — it can never overrun the segment.
pub struct Tcp<'a> {
    pub sport: u16,
    pub dport: u16,
    pub seq: u32,
    pub ack: u32,
    pub flags: u8,
    pub wnd: u16,
    pub options: &'a [u8],
    pub payload: &'a [u8],
}

/// Parse a TCP segment: fixed header >= 20, data offset >= 5 words and
/// within the segment. Checksum is NOT verified here — callers validate
/// via `csum6_ok(csum4(src, dst, NH_TCP, segment))` like UDP.
pub fn parse_tcp(buf: &[u8]) -> Option<Tcp<'_>> {
    if buf.len() < 20 {
        return None;
    }
    let doff = ((buf[12] >> 4) as usize) * 4;
    if !(20..=buf.len()).contains(&doff) {
        return None;
    }
    Some(Tcp {
        sport: u16::from_be_bytes([buf[0], buf[1]]),
        dport: u16::from_be_bytes([buf[2], buf[3]]),
        seq: u32::from_be_bytes(buf[4..8].try_into().unwrap()),
        ack: u32::from_be_bytes(buf[8..12].try_into().unwrap()),
        flags: buf[13],
        wnd: u16::from_be_bytes([buf[14], buf[15]]),
        options: &buf[20..doff],
        payload: &buf[doff..],
    })
}

/// Ethernet + IPv4 + TCP segment into `f`; returns frame length.
/// `options` bytes are emitted verbatim after the fixed header and must
/// be a multiple of 4 in length (callers pad). Checksum field zeroed
/// before csum4 — same CVE-2025-38432 discipline as UDP.
#[allow(clippy::too_many_arguments)]
pub fn build_tcp4(
    f: &mut [u8],
    dst_mac: [u8; 6],
    src_mac: [u8; 6],
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    sport: u16,
    dport: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    options: &[u8],
    payload: &[u8],
) -> usize {
    if options.len() % 4 != 0 || options.len() > 40 {
        return 0;
    }
    let hlen = 20 + options.len();
    let tlen = hlen + payload.len();
    let total = 20 + tlen;
    if f.len() < 14 + total {
        return 0;
    }
    f[..6].copy_from_slice(&dst_mac);
    f[6..12].copy_from_slice(&src_mac);
    f[12..14].copy_from_slice(&0x0800u16.to_be_bytes());

    let ip = 14;
    f[ip] = 0x45;
    f[ip + 1] = 0;
    f[ip + 2..ip + 4].copy_from_slice(&(total as u16).to_be_bytes());
    f[ip + 4..ip + 6].copy_from_slice(&2u16.to_be_bytes());
    f[ip + 6..ip + 8].copy_from_slice(&[0, 0]);
    f[ip + 8] = 64;
    f[ip + 9] = NH_TCP;
    f[ip + 10..ip + 12].copy_from_slice(&[0, 0]);
    f[ip + 12..ip + 16].copy_from_slice(&src_ip);
    f[ip + 16..ip + 20].copy_from_slice(&dst_ip);
    let ck = csum16(&f[ip..ip + 20]);
    f[ip + 10..ip + 12].copy_from_slice(&ck.to_be_bytes());

    let t = ip + 20;
    f[t..t + 2].copy_from_slice(&sport.to_be_bytes());
    f[t + 2..t + 4].copy_from_slice(&dport.to_be_bytes());
    f[t + 4..t + 8].copy_from_slice(&seq.to_be_bytes());
    f[t + 8..t + 12].copy_from_slice(&ack.to_be_bytes());
    f[t + 12] = ((hlen / 4) as u8) << 4;
    f[t + 13] = flags;
    f[t + 14..t + 16].copy_from_slice(&1460u16.to_be_bytes()); // window
    f[t + 16..t + 18].copy_from_slice(&[0, 0]); // checksum: zero first
    f[t + 18..t + 20].copy_from_slice(&[0, 0]); // urg ptr
    f[t + 20..t + hlen].copy_from_slice(options);
    f[t + hlen..t + tlen].copy_from_slice(payload);
    let ck = csum4(src_ip, dst_ip, NH_TCP, &f[t..t + tlen]);
    f[t + 16..t + 18].copy_from_slice(&ck.to_be_bytes());

    14 + total
}
