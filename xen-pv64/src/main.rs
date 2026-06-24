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

fn console_write(s: &str) {
    let ptr = s.as_ptr();
    let len = s.len();
    unsafe {
        core::arch::asm!(
            "call hypercall_page + {offset}",
            offset = const (18 * 32),  /* __HYPERVISOR_console_io * 32 */
            in("rdi") 0usize,          /* CONSOLEIO_write */
            in("rsi") len,
            in("rdx") ptr,
            lateout("rax") _,
            out("rcx") _,              /* clobbered by syscall inside stub */
            out("r11") _,
            options(nostack),
        );
    }
}

fn shutdown() -> ! {
    let reason: u32 = 0; /* SHUTDOWN_poweroff */
    unsafe {
        core::arch::asm!(
            "call hypercall_page + {offset}",
            offset = const (29 * 32),   /* __HYPERVISOR_sched_op * 32 */
            in("rdi") 2usize,           /* SCHEDOP_shutdown */
            in("rsi") &reason as *const u32 as usize,
            lateout("rax") _,
            out("rcx") _,
            out("r11") _,
        );
    }
    loop {} /* should never reach here */
}

#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    console_write("Hello from Rust!\n");
    shutdown();
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
