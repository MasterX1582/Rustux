//! cpu — descriptor tables and exception dispatch.
//!
//! After ExitBootServices we still run on OVMF's GDT and IDT. This module
//! installs our own: a flat 64-bit GDT (null / kernel code / kernel data)
//! and a 256-entry IDT whose per-vector naked stubs funnel into one Rust
//! reporter. Exceptions print vector/rip/err on COM1 and either resume
//! (#UD skips the faulting instruction for the boot proof) or halt.
//!
//! extern "x86-interrupt" is nightly-only, so the stubs are `#[unsafe(naked)]`
//! shims that normalize the stack into a `TrapFrame` and jump to `dispatch`.
//! No TSS/IST yet — #DF lands on the current stack (noted for the real
//! interrupt layer).

use core::fmt::Write;

use crate::uart::{Uart16550, COM1};

const KCODE: u16 = 0x08;
const KDATA: u16 = 0x10;
/// Selector of the TSS descriptor (GDT slots 3-4; a 64-bit TSS descriptor
/// is 16 bytes wide).
const TSS_SEL: u16 = 0x18;

// Flat 64-bit segments: base/limit ignored by the CPU in long mode.
// code: P|DPL0|S|Exec|Read|L   data: P|DPL0|S|Write
// Slots 3-4 hold the TSS descriptor, filled at init (needs the runtime base).
static mut GDT: [u64; 5] = [0, 0x00af_9a00_0000_ffff, 0x00af_9200_0000_ffff, 0, 0];

/// x86_64 task state segment. Only the IST entries matter today: no
/// privilege transitions exist yet, so the rsp fields stay zeroed. iopb
/// past the struct end disables the I/O bitmap.
#[repr(C, packed)]
struct Tss {
    _r0: u32,
    _rsp: [u64; 3],
    _r1: u64,
    ist: [u64; 7],
    _r2: u64,
    _r3: u16,
    iopb: u16,
}

/// Dedicated double-fault stack (IST1). Without it a #DF raised on an
/// already-corrupt stack triple-faults the machine — the vector-8 gate
/// below switches here first, so the handler always has room to report.
#[repr(align(16))]
struct DfStack(#[allow(dead_code)] [u8; 8192]);

static mut TSS: Tss = Tss {
    _r0: 0,
    _rsp: [0; 3],
    _r1: 0,
    ist: [0; 7],
    _r2: 0,
    _r3: 0,
    iopb: 0,
};

static mut DF_STACK: DfStack = DfStack([0; 8192]);

/// Build the 16-byte system descriptor for a 64-bit available TSS.
fn tss_descriptor(base: u64, limit: u64) -> (u64, u64) {
    let low = (limit & 0xffff)
        | ((base & 0x00ff_ffff) << 16)
        | (0b1001_u64 << 40) // type: 64-bit TSS (available)
        | (1_u64 << 47) // present
        | (((limit >> 16) & 0xf) << 48)
        | (((base >> 24) & 0xff) << 56);
    (low, base >> 32)
}

#[repr(C, packed)]
struct TablePtr {
    limit: u16,
    base: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct IdtEntry {
    off_lo: u16,
    sel: u16,
    ist: u8,
    attr: u8,
    off_mid: u16,
    off_hi: u32,
    zero: u32,
}

#[repr(C, align(16))]
struct Idt([IdtEntry; 256]);

const EMPTY: IdtEntry = IdtEntry {
    off_lo: 0,
    sel: 0,
    ist: 0,
    attr: 0,
    off_mid: 0,
    off_hi: 0,
    zero: 0,
};

static mut IDT: Idt = Idt([EMPTY; 256]);

fn set_gate(v: usize, handler: u64, ist: u8) {
    // SAFETY: single-threaded early boot; writes stay inside the static IDT.
    unsafe {
        (*core::ptr::addr_of_mut!(IDT)).0.as_mut_ptr().add(v).write(IdtEntry {
            off_lo: handler as u16,
            sel: KCODE,
            ist,
            attr: 0x8e, // present, DPL0, 64-bit interrupt gate (clears IF)
            off_mid: (handler >> 16) as u16,
            off_hi: (handler >> 32) as u32,
            zero: 0,
        });
    }
}

/// What a stub leaves on the stack for `dispatch`: vector, error code
/// (real or pushed dummy 0), then the hardware frame.
#[repr(C)]
struct TrapFrame {
    vec: u64,
    err: u64,
    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: u64,
    ss: u64,
}

const NAMES: [&str; 32] = [
    "#DE", "#DB", "NMI", "#BP", "#OF", "#BR", "#UD", "#NM", "#DF", "?", "#TS", "#NP", "#SS", "#GP",
    "#PF", "?", "#MF", "#AC", "#MC", "#XM", "#VE", "#CP", "?", "?", "?", "?", "?", "?", "?", "#VC",
    "#SX", "?",
];

/// Rust-side reporter. `rcx` carries `&mut TrapFrame` (set by `dispatch`) —
/// the UEFI target uses the Windows x64 ABI, so the first arg arrives in
/// rcx, not rdi.
extern "C" fn report_frame(f: &mut TrapFrame) {
    // Hardware IRQs (LAPIC timer, spurious) take the cheap path: no serial
    // port setup, no printing. Every other vector is an exception report.
    if f.vec == crate::apic::IRQ_TIMER || f.vec == crate::apic::IRQ_SPURIOUS {
        crate::apic::irq_dispatch(f.vec);
        return;
    }
    let mut u = unsafe { Uart16550::new(COM1) };
    u.init();
    match f.vec {
        // #UD boot proof: report and skip the 2-byte ud2.
        6 => {
            let _ = writeln!(
                u,
                "rustux: cpu: caught #UD rip={:#x} -- skipping",
                f.rip
            );
            f.rip += 2;
        }
        // #BP/int3 resumes in place.
        3 => {
            let _ = writeln!(u, "rustux: cpu: caught #BP rip={:#x} -- resuming", f.rip);
        }
        v => {
            let name = if v == u64::MAX {
                "unknown"
            } else if v < 32 {
                NAMES[v as usize]
            } else {
                "irq"
            };
            let _ = writeln!(
                u,
                "rustux: cpu: EXCEPTION {} rip={:#x} err={:#x} cs={:#x} -- halting",
                name, f.rip, f.err, f.cs
            );
            loop {
                // SAFETY: halting forever is the intended terminal state.
                unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)) }
            }
        }
    }
}

/// Shared tail for every stub. Stubs push ONLY immediates so the frame can
/// be located without clobbering any register: on entry rsp points at `vec`.
/// Here we save rcx (the arg register we are about to reuse) first, derive
/// `&TrapFrame` from the stack, save every other caller-saved register
/// (asynchronous IRQs must not corrupt the interrupted context), call the
/// reporter, restore, unwind to the hardware frame and iretq.
#[unsafe(naked)]
extern "C" fn dispatch() -> ! {
    core::arch::naked_asm!(
        "push rcx", // interrupted rcx -- stubs leave it untouched
        "mov rcx, rsp",
        "lea rcx, [rcx + 8]", // rcx = &TrapFrame (first arg, Windows x64 ABI)
        "push rbp",
        "mov rbp, rsp",
        "and rsp, -16",
        "push r11",
        "push r10",
        "push r9",
        "push r8",
        "push rdi",
        "push rsi",
        "push rdx",
        "push rcx",
        "push rax",
        "sub rsp, 8",
        "call {r}",
        "add rsp, 8",
        "pop rax",
        "pop rcx",
        "pop rdx",
        "pop rsi",
        "pop rdi",
        "pop r8",
        "pop r9",
        "pop r10",
        "pop r11",
        "mov rsp, rbp",
        "pop rbp",
        "pop rcx", // interrupted rcx back
        "add rsp, 16",
        "iretq",
        r = sym report_frame,
    );
}

/// Stub without CPU error code: pushes a dummy err=0 then the vector. Must
/// not write any register -- the interrupted context's rcx et al. are live.
macro_rules! stub {
    ($name:ident, $vec:expr) => {
        #[unsafe(naked)]
        extern "C" fn $name() {
            core::arch::naked_asm!(
                "push 0",
                concat!("push ", $vec),
                "jmp {d}",
                d = sym dispatch,
            );
        }
    };
}

/// Stub for vectors where the CPU already pushed the error code.
macro_rules! stub_err {
    ($name:ident, $vec:expr) => {
        #[unsafe(naked)]
        extern "C" fn $name() {
            core::arch::naked_asm!(
                concat!("push ", $vec),
                "jmp {d}",
                d = sym dispatch,
            );
        }
    };
}

stub!(stub_default, "-1"); // unknown vector sentinel
stub_err!(stub_df, 8);
stub_err!(stub_ts, 10);
stub_err!(stub_np, 11);
stub_err!(stub_ss, 12);
stub_err!(stub_gp, 13);
stub_err!(stub_pf, 14);
stub_err!(stub_ac, 17);
stub_err!(stub_cp, 21);
stub_err!(stub_vc, 29);
stub!(stub_ud, 6);
stub!(stub_bp, 3);
stub!(stub_irq_timer, "0xf0");
stub!(stub_irq_spurious, "0xff");

/// Install our GDT (reload CS via far return, data segments via ax), TSS
/// (#DF gets a dedicated IST stack), and IDT (all 256 vectors, incl. IRQ
/// stubs for the LAPIC timer and spurious vectors). Interrupts stay
/// masked — apic::init + `sti` come later.
pub fn init() {
    // Fill the TSS descriptor and the DF IST stack before lgdt — the base
    // address only exists at runtime.
    // SAFETY: single-threaded; the statics are written once, before ltr.
    unsafe {
        let tss = core::ptr::addr_of_mut!(TSS);
        (*tss).iopb = core::mem::size_of::<Tss>() as u16;
        (*tss).ist[0] = core::ptr::addr_of!(DF_STACK) as u64 + 8192;
        let (lo, hi) =
            tss_descriptor(tss as u64, core::mem::size_of::<Tss>() as u64 - 1);
        let gdt = core::ptr::addr_of_mut!(GDT);
        (*gdt)[3] = lo;
        (*gdt)[4] = hi;
    }

    let gdtr = TablePtr {
        limit: (5 * 8 - 1) as u16,
        base: core::ptr::addr_of!(GDT) as u64,
    };
    // SAFETY: gdtr points at our valid flat GDT; CS reload uses a far return
    // to the next instruction; running code/data remain reachable under the
    // identity map.
    unsafe {
        core::arch::asm!("lgdt [{}]", in(reg) &gdtr, options(readonly, nostack));
        core::arch::asm!(
            "mov rax, {sel}",
            "mov ds, ax",
            "mov es, ax",
            "mov ss, ax",
            sel = in(reg) KDATA as u64,
            out("rax") _,
            options(nostack, preserves_flags),
        );
        // Reload CS with our kernel code selector via a far return so that
        // interrupt frames carry a cs that exists in OUR GDT -- otherwise
        // iretq back to the firmware's 0x38 selector would #GP on return.
        core::arch::asm!(
            "push {sel}",
            "lea rax, [rip + 2f]",
            "push rax",
            ".byte 0x48, 0xcb", // lretq: REX.W makes it pop 8-byte rip + cs
            "2:",
            sel = in(reg) KCODE as u64,
            out("rax") _,
            options(nostack),
        );
        // Load the task register so the IST mechanism is armed; #DF gates
        // reference IST1 below.
        core::arch::asm!("ltr {0:x}", in(reg) TSS_SEL, options(nostack, preserves_flags));
    }

    let gates: [(usize, u64, u8); 13] = [
        (3, stub_bp as *const () as u64, 0),
        (6, stub_ud as *const () as u64, 0),
        (8, stub_df as *const () as u64, 1), // IST1: dedicated DF stack
        (10, stub_ts as *const () as u64, 0),
        (11, stub_np as *const () as u64, 0),
        (12, stub_ss as *const () as u64, 0),
        (13, stub_gp as *const () as u64, 0),
        (14, stub_pf as *const () as u64, 0),
        (17, stub_ac as *const () as u64, 0),
        (21, stub_cp as *const () as u64, 0),
        (29, stub_vc as *const () as u64, 0),
        (0xf0, stub_irq_timer as *const () as u64, 0),
        (0xff, stub_irq_spurious as *const () as u64, 0),
    ];
    for v in 0..256 {
        set_gate(v, stub_default as *const () as u64, 0);
    }
    for (v, h, ist) in gates {
        set_gate(v, h, ist);
    }

    let idtr = TablePtr {
        limit: (256 * 16 - 1) as u16,
        base: core::ptr::addr_of!(IDT) as u64,
    };
    // SAFETY: idtr points at the fully populated 256-entry static IDT.
    unsafe {
        core::arch::asm!("cli", options(nomem, nostack));
        core::arch::asm!("lidt [{}]", in(reg) &idtr, options(readonly, nostack));
    }
}
