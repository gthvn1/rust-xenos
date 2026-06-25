- We are writing a Xen PV64 guest in Rust for learning purposes.
  - We are considering PV64 only
- https://xenbits.xen.org/docs/xtf/index.html

## ELF Notes + assembly entry point

- When Xen loads the ELF it reads a `.note.Xen` section to understand what kind of guest
  this is. For PV64 we will need 7 notes.
- Then it jumps to `_elf_start` with `%rsi =` pointer to a `start_info` struct.

### ELF note format

- Each note in the `.note` section has this layout:
```
[namesz: u32]
[descsz: u32]
[type:   u32]
[name]
[desc]
```

### Expected

- `_elf_start`: Entry point, first byte of the binary
- `kernel_main`: Our Rust function entry point
- `boot_stack`: 4K BSS
- `hypercall_page`: 4K that Xen will fill
- `pv_start_info`: 8 bytes after the hypercall page
- `_end`: end of binary

```
❯ readelf -s target/x86_64-unknown-none/debug/xen-pv64

Symbol table '.symtab' contains 10 entries:
   Num:    Value          Size Type    Bind   Vis      Ndx Name
     0: 0000000000000000     0 NOTYPE  LOCAL  DEFAULT  UND
     1: 0000000000000000     0 FILE    LOCAL  DEFAULT  ABS 0qb72jqev7dm1qjt[...]
     2: 0000000000000000     0 FILE    LOCAL  DEFAULT  ABS compiler_builtin[...]
     3: 0000000000002000  4096 OBJECT  GLOBAL DEFAULT    6 boot_stack
     4: 0000000000003000  4096 OBJECT  GLOBAL DEFAULT    6 hypercall_page
     5: 0000000000000020     4 FUNC    GLOBAL DEFAULT    1 kernel_main
     6: 0000000000004000     8 OBJECT  GLOBAL DEFAULT    6 pv_start_info
     7: 0000000000000000     0 NOTYPE  GLOBAL DEFAULT    1 _elf_start
     8: 0000000000000000     0 NOTYPE  GLOBAL DEFAULT    1 _start
     9: 0000000000004008     0 NOTYPE  GLOBAL DEFAULT    6 _end
```

- And the notes:
```
❯ readelf -n target/x86_64-unknown-none/debug/xen-pv64

Displaying notes found in: .note
  Owner                Data size        Description
  Xen                  0x00000004       Unknown note type: (0x00000006)
   description data: 58 54 46 00
  Xen                  0x00000002       Unknown note type: (0x00000007)
   description data: 30 00
  Xen                  0x00000008       Unknown note type: (0x00000008)
   description data: 67 65 6e 65 72 69 63 00
  Xen                  0x00000008       NT_ARCH (architecture)
   description data: 00 30 00 00 00 00 00 00
  Xen                  0x00000008       Unknown note type: (0x00000005)
   description data: 78 65 6e 2d 33 2e 30 00
  Xen                  0x0000002a       Unknown note type: (0x0000000a)
   description data: 21 77 72 69 74 61 62 6c 65 5f 70 61 67 65 5f 74 61 62 6c 65 73 7c 70 61 65 5f 70 67 64 69 72 5f 61 62 6f 76 65 5f 34 67 62 00
  Xen                  0x00000004       Unknown note type: (0x00000009)
   description data: 79 65 73 00
```


## Print hello
- `HYPERVISOR_console_io`: print "Hello"
- Xen filled the `hypercall_page` with 128 stubs. Each stub is 32 bytes: it moves the
hypercall number into `eax` and executes `syscall`. To invoke hypercall N we need
to do: `call hypercall_page + (N * 32)`
- x86_64 hypercall ABI puts arguments in the same registers as linux syscalls:
  - arg1: rdi
  - arg2: rsi
  - arg3: rdx
  - return: rax
  - clobbered: rcx, r11

- For HYPERVISOR_console_io(CONSOLEIO_write, len, ptr):
  - rdi = 0 (CONSOLEIO_write)
  - rsi = byte length
  - rdx = pointer to the string

## Run it
- You need to copy the config file to Dom0
- From Dom0:
```sh
# xl create -p xen-pv64.cfg
Parsing config from xen-pv64.cfg
# -> from another terminal you can open a console: xl console xen-pv64
# xl unpause xen-pv64
```
- You should see the message on the console.
