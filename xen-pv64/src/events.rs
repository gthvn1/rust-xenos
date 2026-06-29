use crate::hypercall::{Hypercall, hypercall2, hypercall3};

// xen/include/public/xen.h

// 'evtchn_upcall_pending' is written non-zero by Xen to indicate
// a pending notification for a particular VCPU. It is then cleared
// by the guest OS /before/ checking for pending work, thus avoiding
// a set-and-check race. Note that the mask is only accessed by Xen
// on the CPU that is currently hosting the VCPU. This means that the
// pending and mask flags can be updated by the guest without special
// synchronisation (i.e., no need for the x86 LOCK prefix).
// This may seem suboptimal because if the pending flag is set by
// a different CPU then an IPI may be scheduled even when the mask
// is set. However, note:
//  1. The task of 'interrupt holdoff' is covered by the per-event-
//     channel mask bits. A 'noisy' event that is continually being
//     triggered can be masked at source at this very precise
//     granularity.
//  2. The main purpose of the per-VCPU mask is therefore to restrict
//     reentrant execution: whether for concurrency control, or to
//     prevent unbounded stack usage. Whatever the purpose, we expect
//     that the mask will be asserted only for short periods at a time,
//     and so the likelihood of a 'spurious' IPI is suitably small.
// The mask is read before making an event upcall to the guest: a
// non-zero mask therefore guarantees that the VCPU will not receive
// an upcall activation. The mask is cleared when the VCPU requests
// to block: this avoids wakeup-waiting races.
#[allow(dead_code)]
#[repr(C)]
struct VcpuInfo {
    evtchn_upcall_pending: u8, // 1 byte
    evtchn_upcall_mask: u8,    // 1 byte
    _pad: [u8; 6],             // 6 bytes alignment padding
    evtchn_pending_sel: u64,   // 8 bytes
    _arch: [u64; 2],           // 16 bytes (arch_vcpu_info we don't use)
    _time: [u64; 4],           // 32 bytes vcpu_time_info we don't use)
                               // 64 bytes
}
const _: () = assert!(core::mem::size_of::<VcpuInfo>() == 64);

// A domain can create "event channels" on which it can send and receive
// asynchronous event notifications. There are three classes of event that
// are delivered by this mechanism:
//  1. Bi-directional inter- and intra-domain connections. Domains must
//     arrange out-of-band to set up a connection (usually by allocating
//     an unbound 'listener' port and avertising that via a storage service
//     such as xenstore).
//  2. Physical interrupts. A domain with suitable hardware-access
//     privileges can bind an event-channel port to a physical interrupt
//     source.
//  3. Virtual interrupts ('events'). A domain can bind an event-channel
//     port to a virtual interrupt source, such as the virtual-timer
//     device or the emergency console.
//
// Event channels are addressed by a "port index". Each channel is
// associated with two bits of information:
//  1. PENDING -- notifies the domain that there is a pending notification
//     to be processed. This bit is cleared by the guest.
//  2. MASK -- if this bit is clear then a 0->1 transition of PENDING
//     will cause an asynchronous upcall to be scheduled. This bit is only
//     updated by the guest. It is read-only within Xen. If a channel
//     becomes pending while the channel is masked then the 'edge' is lost
//     (i.e., when the channel is unmasked, the guest must manually handle
//     pending notifications as no upcall will be scheduled by Xen).
//
// To expedite scanning of pending notifications, any 0->1 pending
// transition on an unmasked channel causes a corresponding bit in a
// per-vcpu selector word to be set. Each bit in the selector covers a
// 'C long' in the PENDING bitfield array.
#[allow(dead_code)]
#[repr(C)]
struct SharedInfo {
    vcpu_info: [VcpuInfo; 32], // 64 * 32 = 2048 bytes
    evtchn_pending: [u64; 64], // 8 * 64 = 512 bytes
    evtchn_mask: [u64; 64],    // 8 * 64 = 512 bytes
                               // We don't need anything else for now
}

#[allow(dead_code)]
#[repr(align(4096))]
struct InfoPage([u8; 4096]);

// SHARED_INFO page is just a raw byte array wrapper
static mut SHARED_INFO: InfoPage = InfoPage([0; 4096]);
// So keep a pointer to acces its fields
static mut SHI_PTR: *mut SharedInfo = &raw mut SHARED_INFO as *mut SharedInfo;

pub enum Event {
    Port(u32), // fired port number
    Timeout,
}

// We need to map the page in our page table
pub fn init(shared_info_maddr: u64) {
    let virt = &raw const SHARED_INFO as *const _ as usize;
    let pte: usize = (shared_info_maddr as usize) | 0x3; /* P | RW */
    let uvmf_invlpg: usize = 2; // use UVMF_INVLPG to tell Xen to flush TLB

    unsafe {
        hypercall3::<{ Hypercall::UpdateVaMapping as usize }>(virt, pte, uvmf_invlpg);
    }
}

// We want to use HYPERVISOR_sched_op
// -> We will use SCHEDOP_poll
#[repr(C)]
struct SchedPoll {
    ports: *const u32, // pointer to array of port numbers to watch
    nr_ports: u32,
    _pad: u32,
    timeout: u64, // nanoseconds, 0 = block forever
}

pub struct EventPoller {
    ports: [u32; 16],
    next: usize, // next free index
}

impl EventPoller {
    pub const fn new() -> Self {
        Self {
            ports: [0u32; 16],
            next: 0,
        }
    }

    pub fn add_port(&mut self, port: u32) -> Result<(), ()> {
        if self.next >= self.ports.len() {
            Err(())
        } else {
            self.ports[self.next] = port;
            self.next += 1;
            Ok(())
        }
    }

    #[allow(dead_code)]
    pub fn remove_port(&mut self, port: u32) -> Result<(), ()> {
        // First we need to find where it is located and if we find it we swap
        // with the last one (it it is not already the last one).
        for idx in 0..self.next {
            if self.ports[idx] == port {
                if idx != self.next - 1 {
                    // It is not the last item so just permut with last item
                    self.ports[idx] = self.ports[self.next - 1];
                }
                self.next -= 1;
                return Ok(());
            }
        }

        Err(())
    }

    pub fn wait_event(&self) -> Event {
        let schedop_poll_policy: usize = 3;

        // _time[2] == vcpu_time_info.system_time: nanoseconds since boot (absolute).
        // SCHEDOP_poll timeout is also absolute nanoseconds, so we add our desired
        // delay to the current time.
        let now = unsafe { core::ptr::read_volatile(&raw mut (*SHI_PTR).vcpu_info[0]._time[2]) };

        let poll = SchedPoll {
            ports: self.ports.as_ptr(),
            nr_ports: self.next as u32, // next is also indicating the number of ports
            _pad: 0,
            timeout: now + 5_000_000_000, // now + 5 seconds
        };

        unsafe {
            // TODO: manage several ports, currently we know that we have one port only and
            // it is set. We should iterate all ports.
            let port = self.ports[0];

            // Clear upcall_pending BEFORE calling SCHEDOP_poll so that if a new
            // event arrives between the clear and the hypercall, Xen will re-set
            // it and the poll returns immediately instead of blocking forever.
            let up_ptr = &raw mut (*SHI_PTR).vcpu_info[0].evtchn_upcall_pending;
            core::ptr::write_volatile(up_ptr, 0);

            hypercall2::<{ Hypercall::SchedOp as usize }>(
                schedop_poll_policy,
                &poll as *const _ as usize,
            );

            // With a masked port, Xen sets evtchn_pending but does NOT set
            // evtchn_pending_sel. Check evtchn_pending directly for our port.
            let word_idx = (port / 64) as usize;
            let bit_idx = (port % 64) as usize;
            let pending_ptr = &raw mut (*SHI_PTR).evtchn_pending[word_idx];
            let pending_val = core::ptr::read_volatile(pending_ptr);

            if pending_val & (1u64 << bit_idx) != 0 {
                core::ptr::write_volatile(pending_ptr, pending_val & !(1u64 << bit_idx));
                Event::Port(port)
            } else {
                Event::Timeout
            }
        }
    }
}
