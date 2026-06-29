#![no_std]
#![no_main]

mod console;
mod events;
mod hypercall;
mod xenstore;

use console::{ConsoleWriter, PvConsoleWriter as PCW};
use core::fmt::Write;
use hypercall::Hypercall;

use crate::events::{Event, EventPoller};

core::arch::global_asm!(include_str!("boot.s"), options(att_syntax));

#[unsafe(no_mangle)]
static mut START_INFO_PTR: u64 = 0; // Set by reading %rsi from boot before calling kernel_main

// boot_stack doesn't need 4K alignment
#[unsafe(no_mangle)]
static mut boot_stack: [u8; 4096] = [0; 4096];

// x86_64 PV START_INFO: must match Xen ABI exactly.
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
    console_mfn: u64,    // offset 72
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

fn start_info() -> *const StartInfo {
    // As START_INFO_PTR is initialized before calling kernel_main it is "safe"
    // to call it everywhere.
    unsafe { START_INFO_PTR as *const StartInfo }
}

#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    let shared_info_maddr = unsafe { (*start_info()).shared_info };
    let console_mfn = unsafe { (*start_info()).console_mfn };
    let xs_mfn = unsafe { (*start_info()).store_mfn };
    let console_evtchn = unsafe { (*start_info()).console_evtchn };
    let xs_evtchn = unsafe { (*start_info()).store_evtchn };

    // Console writer probably won't work if Xen is not compiled with the correct
    // flag. But let it there as an example of how to do it. In case of debug it could
    // be usefull to use it for early message.
    let _ = write!(ConsoleWriter, "Hello via HYPERVISOR_console_io\r\n");

    // Init pv console is only required for PvConsoleWriter
    console::init_pv_console(console_mfn, console_evtchn);
    // Init xenstore
    xenstore::init(xs_mfn, xs_evtchn);
    // Setup events
    let mut event = EventPoller::new();
    events::init(shared_info_maddr);
    event.add_port(xs_evtchn).unwrap();
    event.add_port(console_evtchn).unwrap();

    // Ready to print some stuff
    let _ = write!(PCW, "xenos: PV guest started\r\n");
    let _ = write!(
        PCW,
        "xenos: memory {} MiB\r\n",
        unsafe { (*start_info()).nr_pages } * 4096 / 1024 / 1024
    );
    let _ = write!(
        PCW,
        "xenos: console evtchn={}, store evtchn={}\r\n",
        console_evtchn, xs_evtchn
    );

    // Testing the PV console by writing something and read user input
    let _ = write!(
        PCW,
        "xenos: console self-test: enter something (blocking wait)..."
    );

    // Read something from pv console
    let mut buf = [0u8; 64];
    let bytes_read = console::pv_console_read_line(&mut buf);

    let _ = write!(
        PCW,
        "\r\nxenos: console self-test: read {} bytes\r\n",
        bytes_read
    );
    let input = core::str::from_utf8(&buf[0..bytes_read]).unwrap_or("???");
    let _ = write!(PCW, "xenos: console self-test: {}\r\n", input);

    // Now the main loop
    let _ = write!(PCW, "xenos: entering event loop\r\n");
    let _ = write!(PCW, "xenos: waiting for console input (5s timeout)\r\n");

    let mut line_buf = [0u8; 64];
    let mut line_len: usize;

    // For testing we write a req and read the response outside the loop
    xenstore::write();
    line_len = xenstore::read(&mut line_buf);
    let _ = write!(PCW, "xenos: xenstore: read {} bytes\r\n", line_len);
    let s = core::str::from_utf8(&line_buf[..line_len]).unwrap_or("?");
    let _ = write!(PCW, "xenos: xenstore: {}\r\n", s);

    line_len = 0;
    let mut done = false;
    let mut unknown_port_count = 0u32;

    while !done {
        match event.wait_event() {
            Event::Port(p) if p == console_evtchn => {
                while let Some(b) = console::pv_console_read_byte() {
                    // Echo the character if it is printable
                    if b.is_ascii_graphic() || b == b' ' {
                        let _ = write!(PCW, "{}", b as char);
                    }

                    // If we have an end of line we are done, otherwise keep reading
                    // and add the charater to line_buf. It is for debugging so no need
                    // to check the size for now...
                    if b == b'\r' || b == b'\n' {
                        let s = core::str::from_utf8(&line_buf[..line_len]).unwrap_or("?");
                        let _ = write!(PCW, "\r\nGot: {}\r\n", s);
                        done = true;
                        break;
                    } else if line_len < line_buf.len() {
                        line_buf[line_len] = b;
                        line_len += 1;
                    }
                }
            }
            Event::Port(_) => {
                unknown_port_count += 1;
            }
            Event::Timeout => {
                let _ = write!(PCW, "\r\nxenos: idle timeout, shutting down\r\n");
                done = true;
            }
        }
    }

    let _ = write!(PCW, "xenos: debug: unknown_port={}\r\n", unknown_port_count);
    let _ = write!(PCW, "xenos: powering off\r\n");
    shutdown();
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    // I'm testing the guest on a release build of XCP-ng and
    // HYPERVISOR_console_io are blocked (compiled without CONFIG_VERBOSE_DEBUG).
    // So we are using the pvconsole to have panic.
    let console_mfn = unsafe { (*start_info()).console_mfn };
    let console_evtchn = unsafe { (*start_info()).console_evtchn };

    // Init pv console is only required for PvConsoleWriter
    console::init_pv_console(console_mfn, console_evtchn);

    let _ = write!(PCW, "PANIC: {}\r\n", info);
    shutdown();
}
