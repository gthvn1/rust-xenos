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

    // See the message in panic if you don't see the message in xl dmesg.
    let _ = write!(ConsoleWriter, "Hello via HYPERVISOR_console_io\r\n");

    let _ = write!(PvConsoleWriter, "Hello via PV console!\r\n");
    let _ = write!(PvConsoleWriter, "Please enter something: ");

    // Read something from pv console
    let mut buf = [0u8; 64];
    let bytes_read = console::pv_console_read_line(&mut buf);

    let _ = write!(PvConsoleWriter, "\r\nwe read {} bytes\r\n", bytes_read);
    let input = core::str::from_utf8(&buf[0..bytes_read]).unwrap_or("???");
    let _ = write!(PvConsoleWriter, "{}\r\n", input);

    shutdown();
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    // I'm testing the guest on a release build of XCP-ng and
    // HYPERVISOR_console_io are blocked (compiled without CONFIG_VERBOSE_DEBUG).
    // So we are using the pvconsole to have panic.
    console::init_pv_console(); // can be called even if already called in main
    let _ = write!(PvConsoleWriter, "PANIC: {}\r\n", info);
    shutdown();
}
