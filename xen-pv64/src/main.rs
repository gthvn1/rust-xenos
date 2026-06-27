#![no_std]
#![no_main]

mod console;
mod hypercall;

use console::{ConsoleWriter, PvConsoleWriter};
use core::fmt::Write;
use hypercall::{Hypercall, hypercall2};

core::arch::global_asm!(include_str!("boot.s"), options(att_syntax));

// boot_stack doesn't need 4K alignment
#[unsafe(no_mangle)]
static mut boot_stack: [u8; 4096] = [0; 4096];

#[allow(clippy::empty_loop)]
fn shutdown() -> ! {
    let reason: u32 = 0; // SHUTDOWN_poweroff
    let schedop_shutdown: usize = 2;
    unsafe {
        hypercall2::<{ Hypercall::SchedOp as usize }>(
            schedop_shutdown,
            &reason as *const u32 as usize,
        );
    }

    panic!("unreachable")
}

#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    // Init pv console is only required for PvConsoleWriter
    console::init_pv_console();

    let _ = write!(ConsoleWriter, "Hello via HYPERVISOR_console_io\r\n");
    let _ = write!(PvConsoleWriter, "Hello via PV console!\r\n");

    shutdown();
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let _ = write!(ConsoleWriter, "\nPANIC: {}\r\n", info);
    shutdown();
}
