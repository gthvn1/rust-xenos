use crate::hypercall::{Hypercall, hypercall2, hypercall3};

// RTFS: https://xenbits.xen.org/gitweb/?p=xen.git;a=tree
// - xen/include/public/io/xs_wire.h
// - docs/misc/xenstore.txt
// - docs/misc/xenstore-ring.txt
//
// We will communicate via event channel + shared memory
//

#[allow(dead_code)]
#[repr(align(4096))]
struct XsPage([u8; 4096]);

static mut XS_RING: XsPage = XsPage([0; 4096]);
static mut XS_EVTCHN: u32 = 0;

#[allow(dead_code)]
#[repr(u32)]
enum SockMsgType {
    Control,
    Directory,
    Read,
    // Skip 4 to 9
    GetDomainPath = 10,
    Write,
    // Skip the rest for now
}

#[repr(C)]
struct SockMsg {
    ty: SockMsgType, // must be u32
    req_id: u32,     // Request id, echoed in daemon's response
    tx_id: u32,      // Transaction id (0 if not related to a transaction
    len: u32,        // Length of data following this
                     // The payload follow
}

// Inter domain shared memory communications
#[repr(C)]
struct DomainInterface {
    req: [u8; 1024],      // (circular buffers), requests to xenstore daemon
    rsp: [u8; 1024],      // (circular buffers), replies and async watch events
    req_cons: u32,        // where xenstored will read the next byte
    req_prod: u32,        // where we will write the next byte
    rsp_cons: u32,        // Output consumer offset
    rsp_prod: u32,        // Output producer offset
    server_features: u32, // Bitmap of features supported by the server
    connection: u32,
    error: u32,
}

// We need to map the page in our page table
pub fn init(xs_mfn: u64, evtchn: u32) {
    unsafe {
        XS_EVTCHN = evtchn;
    }

    let virt = &raw const XS_RING as *const _ as usize;
    let pte: usize = ((xs_mfn << 12) as usize) | 0x3; /* P | RW */
    let uvmf_invlpg: usize = 2; // use UVMF_INVLPG to tell Xen to flush TLB

    unsafe {
        hypercall3::<{ Hypercall::UpdateVaMapping as usize }>(virt, pte, uvmf_invlpg);
    }
}

pub fn write() {
    let iface = &raw mut XS_RING as *mut DomainInterface;

    // Let's hardcode GetDomainPath for domain 0
    let payload = "0\0";
    let msg: SockMsg = SockMsg {
        ty: SockMsgType::GetDomainPath,
        req_id: 1,
        tx_id: 0,
        len: payload.len() as u32,
    };

    // Push the request
    let ptr = &msg as *const SockMsg as *const u8;
    unsafe {
        for &byte in core::slice::from_raw_parts(ptr, core::mem::size_of::<SockMsg>()).iter() {
            loop {
                let prod_idx = core::ptr::read_volatile(&(*iface).req_prod);
                let cons_idx = core::ptr::read_volatile(&(*iface).req_cons);
                // Ensure that there is space to write byte
                if prod_idx.wrapping_sub(cons_idx) < 1024 {
                    let idx = (prod_idx as usize) & (1024 - 1);
                    core::ptr::write_volatile(&mut (*iface).req[idx], byte);
                    // Ensure that we write before updating the prod
                    core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
                    core::ptr::write_volatile(&mut (*iface).req_prod, prod_idx.wrapping_add(1));
                    break;
                }

                // Wait for space
                core::hint::spin_loop();
            }
        }

        for &byte in payload.as_bytes() {
            loop {
                let prod_idx = core::ptr::read_volatile(&(*iface).req_prod);
                let cons_idx = core::ptr::read_volatile(&(*iface).req_cons);
                // Ensure that there is space to write byte
                if prod_idx.wrapping_sub(cons_idx) < 1024 {
                    let idx = (prod_idx as usize) & (1024 - 1);
                    core::ptr::write_volatile(&mut (*iface).req[idx], byte);
                    // Ensure that we write before updating the prod
                    core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
                    core::ptr::write_volatile(&mut (*iface).req_prod, prod_idx.wrapping_add(1));
                    break;
                }

                // Wait for space
                core::hint::spin_loop();
            }
        }

        // Notify Xenstore
        let evtchnop_send: usize = 4;
        let port = XS_EVTCHN;
        hypercall2::<{ Hypercall::EventChannelOp as usize }>(
            evtchnop_send,
            &port as *const u32 as usize,
        );
    }
}

fn xs_read_byte() -> Option<u8> {
    let iface = &raw mut XS_RING as *mut DomainInterface;

    unsafe {
        let rsp_prod = core::ptr::read_volatile(&(*iface).rsp_prod);
        let rsp_cons = core::ptr::read_volatile(&(*iface).rsp_cons);
        // There is something to read if in_prod != in_cons
        if rsp_prod != rsp_cons {
            // in_buf is 1024 bytes
            let idx = (rsp_cons as usize) & (1024 - 1);
            let b = core::ptr::read_volatile(&(*iface).rsp[idx]);
            core::ptr::write_volatile(&mut (*iface).rsp_cons, rsp_cons.wrapping_add(1));
            Some(b)
        } else {
            None
        }
    }
}

pub fn read(buf: &mut [u8]) -> usize {
    let mut msg = [0u8; core::mem::size_of::<SockMsg>()];
    for byte in msg.iter_mut() {
        // TODO: use something that cannot run indefinitely. Here it is
        // for testing that we can exchange with xenstored
        loop {
            if let Some(b) = xs_read_byte() {
                *byte = b;
                break;
            }

            core::hint::spin_loop();
        }
    }

    let header = unsafe { core::ptr::read(msg.as_ptr() as *const SockMsg) };
    let size = if (header.len as usize) < buf.len() {
        header.len as usize
    } else {
        buf.len()
    };

    for byte in buf.iter_mut().take(size) {
        // TODO: use something that cannot run indefinitely. Here it is
        // for testing that we can exchange with xenstored
        loop {
            if let Some(b) = xs_read_byte() {
                *byte = b;
                break;
            }
            core::hint::spin_loop();
        }
    }

    size
}
