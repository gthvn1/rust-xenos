use crate::hypercall::{Hypercall, hypercall2, hypercall3};

// xencons_interface: in[1024], out[2048], then producer/consumer indices
// See xen/include/public/io/console.h
#[repr(C)]
struct XenConsInterface {
    in_buf: [u8; 1024],
    out_buf: [u8; 2048],
    in_cons: u32,
    in_prod: u32,
    out_cons: u32,
    out_prod: u32,
}

// Placeholder page in BSS that we remap to the console MFN.
// mfn_to_virt (mfn * 4096) does NOT work on XCP-ng because MFNs are
// in the ~16GB range: far outside our guest's page tables.
// Instead we use HYPERVISOR_update_va_mapping to map the console page
// over this page, which Xen has already given us a valid mapping for.
#[allow(dead_code)]
#[repr(align(4096))]
struct ConsPage([u8; 4096]);

static mut CONSOLE_RING: ConsPage = ConsPage([0; 4096]);
static mut CONSOLE_EVTCHN: u32 = 0;

// Map the console MFN over CONSOLE_RING so we can access it.
// Must be called once before pv_console_write.
//
// HYPERVISOR_update_va_mapping(linear_addr, pte, UVMF_INVLPG)
//   rdi = virtual address of the page to remap (our placeholder)
//   rsi = new PTE: (mfn << 12) | Present | RW
//   rdx = UVMF_INVLPG (2): invalidate this single TLB entry
pub fn init_pv_console(mfn: u64, evtchn: u32) {
    unsafe {
        CONSOLE_EVTCHN = evtchn;
    }

    // Page is 4096 bytes, so mfn << 12 gives the physical address
    // As we are PV guest machine and physical address are the same.
    let virt = &raw const CONSOLE_RING as *const _ as usize;
    let pte: usize = ((mfn as usize) << 12) | 0x3; /* P | RW */
    let uvmf_invlpg: usize = 2; // use UVMF_INVLPG to tell Xen to flush TLB

    unsafe {
        hypercall3::<{ Hypercall::UpdateVaMapping as usize }>(virt, pte, uvmf_invlpg);
    }
}

pub struct ConsoleWriter;

impl core::fmt::Write for ConsoleWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        console_write(s);
        Ok(())
    }
}

fn console_write(s: &str) {
    let ptr = s.as_ptr();
    let len = s.len();
    unsafe {
        hypercall3::<{ Hypercall::ConsoleIo as usize }>(0, len, ptr as usize);
    }
}

pub struct PvConsoleWriter;

impl core::fmt::Write for PvConsoleWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        pv_console_write(s);
        Ok(())
    }
}

// It is xenconsoled that runs in dom0 that reads/writes console ring.
// - xenconsoled reads out_prod
// - xenconsoled writes out_cons
//
// - We write a byte in out_buf[out_prod & 2047] then increments out_prod
// - xenconsoled reads byte at out_buf[out_cons & 2047] then increments out_cons
//
// Initial state: out_prod = 0, out_cons = 0 => prod == cons <=> buffer empty
// Guest writes 'H':
//   - read out_prod (=0)
//   - out_buf[out_prod] = 'H'
//   - fence
//   - out_prod = 1
//   -> Sends event (EVTCHNOP_send) -> wakes xenconsoled
// Xenconsoled wakes:
//   - out_prod(= 1) != out_cons(= 0) => something to read
//   - reads out_buf[0] == 'H'
//   - out_cons = 1, acknowledges
// Guest writes 'e':
//   - read out_prod (=1)
//   - out_buf[out_prod] = 'e'
//   - fence
//   - out_prod = 2
//   -> Sends event (EVTCHNOP_send) -> wakes xenconsoled
// ...
// and so on
// the indice grow forever but we use `& (2048 -1)` to artificially wrap
//
fn pv_console_write(s: &str) {
    let cons = &raw mut CONSOLE_RING as *mut XenConsInterface;
    let port = unsafe { CONSOLE_EVTCHN };

    for &byte in s.as_bytes() {
        loop {
            // Use volatile to not cache value in register and not reorder reads/writes
            let prod = unsafe { core::ptr::read_volatile(&(*cons).out_prod) };
            let cons_idx = unsafe { core::ptr::read_volatile(&(*cons).out_cons) };
            // Ensure that there is space to write byte
            if prod.wrapping_sub(cons_idx) < 2048 {
                let idx = (prod as usize) & (2048 - 1);
                unsafe { core::ptr::write_volatile(&mut (*cons).out_buf[idx], byte) };
                // Ensure the byte written to out_buf is visible before out_prod is incremented
                core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
                unsafe { core::ptr::write_volatile(&mut (*cons).out_prod, prod.wrapping_add(1)) };
                break;
            }
            // spin_loop() is just a hint for CPU.
            core::hint::spin_loop();
        }
    }

    // Notify xenconsoled
    let evtchnop_send: usize = 4;
    unsafe {
        hypercall2::<{ Hypercall::EventChannelOp as usize }>(
            evtchnop_send,
            &port as *const u32 as usize,
        );
    }
}

// Note: String are not available in no_std because it needs heap allocator.
// Use a fixed-suze stack buffer instead.
pub fn pv_console_read_line(buf: &mut [u8]) -> usize {
    let mut idx = 0;
    while idx < buf.len() {
        let b = pv_console_read_byte_blocking();
        buf[idx] = b;
        idx += 1;
        if b == b'\n' || b == b'\r' {
            break;
        }
    }

    idx
}

fn pv_console_read_byte_blocking() -> u8 {
    loop {
        if let Some(b) = pv_console_read_byte() {
            return b;
        }
        core::hint::spin_loop();
    }
}

// Xenconsoled writes into in_buf and advances in_prod. We need to
// read in_buf and advance in_cons.
pub fn pv_console_read_byte() -> Option<u8> {
    let cons = &raw mut CONSOLE_RING as *mut XenConsInterface;

    let in_prod = unsafe { core::ptr::read_volatile(&(*cons).in_prod) };
    let in_cons = unsafe { core::ptr::read_volatile(&(*cons).in_cons) };
    // There is something to read if in_prod != in_cons
    if in_prod != in_cons {
        // in_buf is 1024 bytes
        let idx = (in_cons as usize) & (1024 - 1);
        let b = unsafe { core::ptr::read_volatile(&(*cons).in_buf[idx]) };
        unsafe { core::ptr::write_volatile(&mut (*cons).in_cons, in_cons.wrapping_add(1)) };
        Some(b)
    } else {
        None
    }
}
