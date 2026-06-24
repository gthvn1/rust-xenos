/*
 * Xen PV64 ELF notes + entry point
 *
 * ELF note layout (each field is 4 bytes unless noted):
 *   namesz  - byte length of name (including null)
 *   descsz  - byte length of desc (including null for strings)
 *   type    - XEN_ELFNOTE_* constant
 *   name    - "Xen\0" (4 bytes, already 4-aligned)
 *   desc    - the value, followed by .align 4 if not already aligned
 */
.section .note.Xen, "a", @note

/* XEN_ELFNOTE_GUEST_OS = 6 — identify the OS, any string is fine */
.align 4
.long 4                     /* namesz: "Xen\0" = 4 */
.long 4                     /* descsz: "XTF\0" = 4 */
.long 6
.asciz "Xen"
.asciz "XTF"

/* XEN_ELFNOTE_GUEST_VERSION = 7 */
.align 4
.long 4                     /* namesz */
.long 2                     /* descsz: "0\0" = 2 */
.long 7
.asciz "Xen"
.asciz "0"
.align 4                    /* pad descsz 2 -> 4 */

/* XEN_ELFNOTE_LOADER = 8 — must be "generic" for PV */
.align 4
.long 4                     /* namesz */
.long 8                     /* descsz: "generic\0" = 8 */
.long 8
.asciz "Xen"
.asciz "generic"

/* XEN_ELFNOTE_HYPERCALL_PAGE = 2 — address Xen will fill with stubs */
.align 4
.long 4                     /* namesz */
.long 8                     /* descsz: 64-bit address */
.long 2
.asciz "Xen"
.quad hypercall_page

/* XEN_ELFNOTE_XEN_VERSION = 5 — must be exactly "xen-3.0" */
.align 4
.long 4                     /* namesz */
.long 8                     /* descsz: "xen-3.0\0" = 8 */
.long 5
.asciz "Xen"
.asciz "xen-3.0"

/* XEN_ELFNOTE_FEATURES = 10 — require non-writable page tables (PV safety) */
.align 4
.long 4                     /* namesz */
.long 42                    /* descsz: 41 chars + null = 42 */
.long 10
.asciz "Xen"
.asciz "!writable_page_tables|pae_pgdir_above_4gb"
.align 4                    /* pad descsz 42 -> 44 */

/* XEN_ELFNOTE_PAE_MODE = 9 */
.align 4
.long 4                     /* namesz */
.long 4                     /* descsz: "yes\0" = 4 */
.long 9
.asciz "Xen"
.asciz "yes"


/*
 * PV64 entry point.
 * Xen jumps here in 64-bit mode with:
 *   %rsi = pointer to start_info struct
 * Stack is not set up yet — we must do it ourselves.
 */
.section .text.head, "ax", @progbits
.global _elf_start
_elf_start:
    leaq  pv_start_info(%rip), %rax   /* load address of pv_start_info */
    movq  %rsi, (%rax)                /* store start_info pointer there */
    leaq  boot_stack(%rip), %rsp
    addq  $4096, %rsp                  /* point to top of boot stack */
    call  kernel_main
1:  hlt                                /* kernel_main must never return */
    jmp   1b
