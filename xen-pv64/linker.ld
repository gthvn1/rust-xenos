OUTPUT_FORMAT("elf64-x86-64")
OUTPUT_ARCH(i386:x86-64)
ENTRY(_elf_start)

PHDRS
{
    text PT_LOAD FLAGS(7);  /* RWX: single load segment for simplicity */
    note PT_NOTE FLAGS(4);  /* R  : Xen reads ELF notes from here     */
}

SECTIONS
{
    . = 0x0;       /* PV guests load at virtual address 0 */

    _start = .;

    /* Entry point section must come first so _elf_start lands at 0x0 */
    .text : {
        KEEP(*(.text.head))
        *(.text .text.*)
    } :text

    .rodata : {
        *(.rodata .rodata.*)
    } :text

    .data : {
        . = ALIGN(4096);
        *(.data .data.*)
    } :text

    /* Xen scans the PT_NOTE segment to find guest type and hypercall page */
    .note : {
        KEEP(*(.note .note.*))
    } :note :text

    /* NOLOAD: reserve virtual space but write nothing to the file */
    .bss (NOLOAD) : {
        . = ALIGN(4096);
        *(.bss .bss.*)
        *(COMMON)
    } :text

    _end = .;
}
