#![no_std]
#![no_main]

mod console;
mod events;
mod hypercall;

use console::{ConsoleWriter, PvConsoleWriter};
use core::fmt::Write;
use hypercall::Hypercall;

use crate::events::Event;

core::arch::global_asm!(include_str!("boot.s"), options(att_syntax));

// Set by reading %rsi from boot
#[unsafe(no_mangle)]
static mut pv_start_info: u64 = 0;

// boot_stack doesn't need 4K alignment
#[unsafe(no_mangle)]
static mut boot_stack: [u8; 4096] = [0; 4096];

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

#[allow(clippy::empty_loop)]
fn shutdown() -> ! {
    let reason: u32 = 0; // SHUTDOWN_poweroff
    let schedop_shutdown: usize = 2;
    unsafe {
        hypercall::hypercall2::<{ Hypercall::SchedOp as usize }>(
            schedop_shutdown,
            &reason as *const u32 as usize,
        );
    }

    panic!("unreachable")
}

#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    let si = unsafe { pv_start_info as *const StartInfo };

    let shared_info_maddr = unsafe { (*si).shared_info };
    let console_mfn = unsafe { (*si).console_mfn };
    let console_evtchn = unsafe { (*si).console_evtchn };

    events::init(shared_info_maddr);
    events::unmask_port(console_evtchn);

    // Init pv console is only required for PvConsoleWriter
    console::init_pv_console(console_mfn, console_evtchn);

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

    let _ = write!(PvConsoleWriter, "\r\nEnter wait loop\r\n");

    loop {
        match events::wait_event() {
            Event::Port(p) if p == console_evtchn => {
                // drain the input ring and echo bytes back
                while let Some(b) = console::pv_console_read_byte() {
                    let _ = write!(PvConsoleWriter, "echo: <{}>\r\n", b);
                }
            }
            Event::Port(p) => {
                let _ = write!(PvConsoleWriter, "\r\nIgnore unknown port {:#x}\r\n", p);
            }
            Event::Spurious(x) => {
                let _ = write!(PvConsoleWriter, "\r\nSpurious {:#x} received\r\n", x);
            }
            Event::Timeout => {
                let _ = write!(PvConsoleWriter, "\r\nGot timeout\r\n");
                break;
            }
        }
    }

    // It is just for calling mask_port
    events::mask_port(console_evtchn);
    shutdown();
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    // I'm testing the guest on a release build of XCP-ng and
    // HYPERVISOR_console_io are blocked (compiled without CONFIG_VERBOSE_DEBUG).
    // So we are using the pvconsole to have panic.
    let si = unsafe { pv_start_info as *const StartInfo };
    let console_mfn = unsafe { (*si).console_mfn };
    let console_evtchn = unsafe { (*si).console_evtchn };

    // Init pv console is only required for PvConsoleWriter
    console::init_pv_console(console_mfn, console_evtchn);

    let _ = write!(PvConsoleWriter, "PANIC: {}\r\n", info);
    shutdown();
}
