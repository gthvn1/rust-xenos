#[allow(dead_code)]
#[repr(align(4096))]
struct Page([u8; 4096]);

#[unsafe(no_mangle)]
static mut hypercall_page: Page = Page([0; 4096]);

#[repr(usize)]
pub enum Hypercall {
    UpdateVaMapping = 14,
    ConsoleIo = 18,
    SchedOp = 29,
    EventChannelOp = 32,
}

pub unsafe fn hypercall2<const N: usize>(rdi: usize, rsi: usize) -> usize {
    let ret: usize;
    unsafe {
        core::arch::asm!(
            "call hypercall_page + {offset}",
            offset = const (N * 32),
            in("rdi") rdi,
            in("rsi") rsi,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
        );
    }
    ret
}

pub unsafe fn hypercall3<const N: usize>(rdi: usize, rsi: usize, rdx: usize) -> usize {
    let ret: usize;
    unsafe {
        core::arch::asm!(
            "call hypercall_page + {offset}",
            offset = const (N * 32),
            in("rdi") rdi,
            in("rsi") rsi,
            in("rdx") rdx,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
        );
    }
    ret
}
