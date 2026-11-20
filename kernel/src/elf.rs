//! ELF64 program loading: header + program-header parsing where every
//! field is validated before use. The image is hostile input — a bad
//! offset, size, or count must be rejected, never wrapped, clamped
//! silently, or trusted (the CVE-2004-1070..1072 class was unchecked
//! header arithmetic and unchecked reads in binfmt_elf).
//!
//! Scope: static ET_EXEC images only — no PT_INTERP, no dynamic
//! linking, no relocations. Rejected, not partially loaded.

pub const PT_LOAD: u32 = 1;
pub const PF_X: u32 = 1;
pub const PF_W: u32 = 2;
#[allow(dead_code)]
pub const PF_R: u32 = 4;

const EHDR_SIZE: usize = 64;
const PHDR_SIZE: usize = 56;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 0x3e;
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;

/// Hard bound on program headers — a real binary needs a handful;
/// beyond this the image is malformed for our purposes.
const MAX_PHNUM: usize = 64;

/// User virtual addresses live in our dedicated user half: PML4 slots
/// 128..256 (slots <128 hold the kernel identity map and stay
/// supervisor-only; slots >=256 are non-canonical/kernel-reserved).
/// [0x4000_0000_0000, 0x8000_0000_0000) is canonical and untranslated
/// by anything else.
pub const USER_VA_LO: u64 = 0x4000_0000_0000;
pub const USER_VA_HI: u64 = 0x8000_0000_0000;

fn u16le(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

fn u32le(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

fn u64le(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes([
        b[off], b[off + 1], b[off + 2], b[off + 3],
        b[off + 4], b[off + 5], b[off + 6], b[off + 7],
    ])
}

/// Validated ELF64 header fields of a static executable.
pub struct Elf {
    pub entry: u64,
    phoff: usize,
    phentsize: usize,
    phnum: usize,
}

/// A parsed program header.
pub struct Phdr {
    pub ty: u32,
    pub flags: u32,
    pub off: u64,
    pub va: u64,
    pub filesz: u64,
    pub memsz: u64,
    /// Alignment requirement — parsed for completeness, enforced
    /// implicitly by our per-page mapping (any alignment works).
    #[allow(dead_code)]
    pub align: u64,
}

/// Parse and validate the ELF header. Every extent check happens
/// before the bytes are touched.
pub fn parse(b: &[u8]) -> Option<Elf> {
    if b.len() < EHDR_SIZE {
        return None;
    }
    if b[0..4] != *b"\x7fELF" {
        return None;
    }
    if b[4] != ELFCLASS64 || b[5] != ELFDATA2LSB {
        return None;
    }
    if u16le(b, 16) != ET_EXEC || u16le(b, 18) != EM_X86_64 {
        return None;
    }
    if u32le(b, 20) != 1 {
        return None;
    }
    let entry = u64le(b, 24);
    let phoff = u64le(b, 32) as usize;
    let phentsize = u16le(b, 54) as usize;
    let phnum = u16le(b, 56) as usize;
    if phnum == 0 || phnum > MAX_PHNUM || phentsize < PHDR_SIZE {
        return None;
    }
    // The phdr table must lie fully inside the image — checked
    // arithmetic, never trusting the count.
    let phsize = phentsize.checked_mul(phnum)?;
    let end = phoff.checked_add(phsize)?;
    if end > b.len() {
        return None;
    }
    // Entry point must be a canonical user address.
    if !(USER_VA_LO..USER_VA_HI).contains(&entry) {
        return None;
    }
    Some(Elf {
        entry,
        phoff,
        phentsize,
        phnum,
    })
}

impl Elf {
    /// Iterate the validated program headers.
    pub fn phdrs<'a>(&'a self, img: &'a [u8]) -> impl Iterator<Item = Phdr> + 'a {
        (0..self.phnum).map(move |i| {
            // SAFETY: parse() proved phoff + phentsize*phnum <= img.len().
            let off = self.phoff + i * self.phentsize;
            let p = &img[off..off + PHDR_SIZE];
            Phdr {
                ty: u32le(p, 0),
                flags: u32le(p, 4),
                off: u64le(p, 8),
                va: u64le(p, 16),
                filesz: u64le(p, 32),
                memsz: u64le(p, 40),
                align: u64le(p, 48),
            }
        })
    }
}
