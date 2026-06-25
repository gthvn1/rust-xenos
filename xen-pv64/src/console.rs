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

#[unsafe(no_mangle)]
static mut pv_start_info: u64 = 0;

// x86_64 PV start_info: must match Xen ABI exactly.
// The C compiler inserts 4-byte padding before each u64 field that follows
// a u32 (_pad1 before store_mfn, _pad2 before the console union).
//
// See xen/include/public/xen.h
#[repr(C)]
struct StartInfo {
    magic: [u8; 32],
    nr_pages: u64,
    shared_info: u64, // Machine address of shared info struct
    flags: u32,
    _pad1: u32,
    store_mfn: u64,
    store_evtchn: u32,
    _pad2: u32,
    console_mfn: u64,    // offset 72/
    console_evtchn: u32, // offset 80
}

// Map the console MFN over CONSOLE_RING so we can access it.
// Must be called once before pv_console_write.
//
// HYPERVISOR_update_va_mapping(linear_addr, pte, UVMF_INVLPG)
//   rdi = virtual address of the page to remap (our placeholder)
//   rsi = new PTE: (mfn << 12) | Present | RW
//   rdx = UVMF_INVLPG (2): invalidate this single TLB entry
pub fn init_pv_console() {
    let si = unsafe { pv_start_info as *const StartInfo };
    let mfn = unsafe { (*si).console_mfn };

    unsafe {
        CONSOLE_EVTCHN = (*si).console_evtchn;
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

pub fn console_write(s: &str) {
    let ptr = s.as_ptr();
    let len = s.len();
    unsafe {
        hypercall3::<{ Hypercall::ConsoleIo as usize }>(0, len, ptr as usize);
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
pub fn pv_console_write(s: &str) {
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
