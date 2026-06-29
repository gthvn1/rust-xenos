use crate::hypercall::{Hypercall, hypercall3};

// RTFS: https://xenbits.xen.org/gitweb/?p=xen.git;a=tree
// - xen/include/public/io/xs_wire.h
// - docs/misc/xenstore.txt
// - docs/misc/xenstore-ring.txt

#[allow(dead_code)]
#[repr(align(4096))]
struct XsPage([u8; 4096]);

static mut XS_PAGE: XsPage = XsPage([0; 4096]);

// We need to map the page in our page table
pub fn init(xs_mfn: u64) {
    let virt = &raw const XS_PAGE as *const _ as usize;
    let pte: usize = ((xs_mfn << 12) as usize) | 0x3; /* P | RW */
    let uvmf_invlpg: usize = 2; // use UVMF_INVLPG to tell Xen to flush TLB

    unsafe {
        hypercall3::<{ Hypercall::UpdateVaMapping as usize }>(virt, pte, uvmf_invlpg);
    }
}
