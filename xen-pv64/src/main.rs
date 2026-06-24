#![no_std]
#![no_main]

core::arch::global_asm!(include_str!("boot.s"), options(att_syntax));

/* Xen writes hypercall stubs here (one 32-byte stub per hypercall number) */
#[repr(align(4096))]
struct Page([u8; 4096]);

#[unsafe(no_mangle)]
static mut hypercall_page: Page = Page([0; 4096]);

/* One page of stack for early boot */
#[unsafe(no_mangle)]
static mut boot_stack: [u8; 4096] = [0; 4096];

/* start_info pointer saved by the entry point */
#[unsafe(no_mangle)]
static mut pv_start_info: u64 = 0;

#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    loop {}
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
