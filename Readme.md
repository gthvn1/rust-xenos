- We are writing XenRT in Rust for learning purpose.
  - We are considering PV64 only
- https://xenbits.xen.org/docs/xtf/index.html

## ELF Notes + assembly entry point

- When Xen loads the ELF it reads a `.note.Xen` section to understand what kind of guest
  this is. For PV64 we will need 7 notes.
- Then it jumps to _elf_start with `%rsi =` pointer to a `start_info` struct.

### ELF note format

- Each note in `.note` section has this layout:
```
[namesz: u32]
[descsz: u32]
[type:   u32]
[name]
[desc]
```
