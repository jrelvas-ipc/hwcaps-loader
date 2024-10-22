# INFORMATION FOR PACKAGING AND DISTRIBUTORS

## Building

### hwcaps_loader

The default rustc and linker options should be sane and don't *require* any
changes.*

`hwcaps-loader` comes with support for three different ABI types, each with 
different properties and advantages...

`x86_64-unknown-linux-gnu` -
Build with glibc ABI and dynamic linking.

- File Size: `~12.9 kB`
- Speed: `340.8 µs ± 56.3 µs`
- Runtime Dependencies: `glibc`
- Build Dependencies: `glibc` + linker
- Requires Rust Nightly: No
- Recommended if uniformity with other binaries and file size is preferred.

`x86_64-unknown-linux-musl` -
Build with musl ABI and static linking.

- File Size: `~22.1 kB`
- Speed: `163.7 µs ± 30.1 µs`
- Runtime Dependencies: None!
- Build Dependencies: `musl` + linker
- Requires Rust Nightly: No
- Recommended if launch latency is preferred.

`x86_64-unknown-none` -
Build without libc, raw rust entry point.

- File Size: `~9.7 kB`
- Speed: `149.1 µs ± 28.6 µs`
- Runtime Dependencies: None!
- Build Dependencies: linker
- Requires Rust Nightly: **Yes**
- Recommended if Rust Nightly is available and relocation model can be static.

The GNU target is recommended during development and testing, as that's probably what you're used to.

Otherwise, MUSL is recommended due to it being significantly faster and having no runtime dependencies. 
If your distribution can build with the Rust Nightly toolchain, consider using the "none" ABI! It's well tested and should be as stable as MUSL. 

**\* Note:** if Rust Nightly is available, it's highly recommended to run `cargo build` with the following arguments:
```
-Z build-std=core,panic_abort -Z build-std-features=panic_immediate_abort
```
This will remove unnecessary panic formatting logic and make binaries slightly smaller and faster as a result.

Testing Metodology:
```
OS: Fedora Linux 42 (Workstation Edition Prerelease)
Kernel: Linux 6.12.0-0.rc0.20240920gitbaeb9a7d8b60.7.fc42.x86_64
CPU: 45W TDP Intel Core i7-13800H @ 2.50GHz (6P+8E 20T)
Memory: 2 x 32GB DDR5-5600 @ 5200 MT/s (Intel Total Memory Encryption)
Rustc: 1.83.0-nightly (9e394f551 2024-09-25)
PPD Profile: Performance
Command: hyperfine --shell=none --warmup 1000 --setup "sleep 3" -M 1000
```

### empty_binary

The `empty_binary` subcrate is included for debugging and benchmarking `hwcaps-loader`. You can build it with:
```
cargo build -p empty_binary --profile release --target x86_64-unknown-none
```
**Warning:** `empty_binary` only supports the `none` target.

## File Tree

A `hwcaps-loader` package should provide these files:
```
/usr/bin/hwcaps-loader -> The actual binary which is responsible for detecting the 
                          CPU's feature level and executing the appropriate program
/usr/hwcaps-loader/-> The directory where optimized binaries loaded 
                          by hwcaps-loader are stored
```

A package which wishes to provide the optimized binaries `foo` and `bar` should provide these files:
```
/usr/bin/foo (symlink /usr/bin/hwcaps-loader) -> A placeholder symlink, where the foo
                                                 binary would normally be present in
                                                 (used to call hwcaps-loader)

/usr/libexec/bar (symlink /usr/bin/hwcaps-loader) -> A placeholder symlink, where the bar
                                                     binary would normally be present in
                                                     (used to call hwcaps-loader)                                       

/usr/hwcaps-loader/{$fl[0..N]}/bin/foo  -> foo binaries for feature levels $fl[0..N] 
/usr/hwcaps-loader/{$fl[0..N]}/libexec/bar  -> bar binaries for feature levels $fl[0..N]
```
Where `$fl[0..N]` are feature levels recognized by `hwcaps-loader` which the package wishes to provide
(YOU DON'T NEED TO PROVIDE EVERY ONE):

- `i386`
- `i486`
- `i586`
- `i686`
- `x86-64-v1`
- `x86-64-v2`
- `x86-64-v3`
- `x86-64-v4`

(Future versions of `hwcaps-loader` may support more feature levels from different architectures)

## Errors

Provided there's no spurious IO errors, hwcaps-loader should never error unless 
the user (or a misbehaving program) intentionally passes an incorrect `argv0` (command path) to `hwcaps-loader`,
or attempts to execute it directly, without going through a symlink.

`hwcaps-loader` uses long, specific exit codes in an attempt to differentiate from exit codes
given by other programs and aid with debugging, however, due to its nature, it might be
difficult to differentiate them from the target program. When in doubt, run `strace`.
Here's a list of possible codes and their meanings:

- `100` - `RUST_PANIC`:  
Rust Panic occured. This should be impossible. If it happens, then it's a nasty bug.
Use the devel profile to print out panic messages.
- `200` - `SELF_EXECUTION`:  
`execve()` was called on `hwcaps-loader` directly instead of one its symlinks, which would
result in recursion. `hwcaps-loader` should *never* be a part of this mechanism.
- `210` - `COMMAND_PATH_INVALID`:  
the `argv0` passed to hwcaps-loader has no null terminator by index 4096, making it an
invalid path. Generally doesn't happen unless a misbehaving program attempts to run.
- `220` - `PROC_PATH_IO_ERROR`:  
An IO error occured while attempting to read `/self/proc/exe`. This generally only happens if
the system is missing support for this magic link or if it's buggy.  
It could also be a sign of faulty sandboxing/containment.
- `221` - `PROC_PATH_INVALID`:  
The path returned by `/self/proc/exe` is invalid. The `hwcaps-loader` binary should
always be in `/usr/bin/`.
- `230` - `PATH_RESOLUTION_IO_ERROR`:  
An IO error occured while attempting to use FS syscalls to resolve the absolute path.
Generally only happens if invalid values are passed to `hwcaps-loader`, the system is buggy,
or there's a problem with the filesystem.
- `240` - `TARGET_PATH_INVALID`:  
Target binaries being executed through `hwcaps-loader` must have `/usr` as an ancestor. 
- `241` - `TARGET_PATH_TOO_LARGE`:  
The target path is too large and doesn't fit in 4096 bytes.
- `242` - `TARGET_EXECUTION_ERROR`:  
An unknown IO error occured while attempting to `execve()` the target path. If this
occurs, something is wrong with your packaging or the filesystem is borked.
- `243` - `TARGET_NO_VIABLE_BINARIES`:  
`hwcaps-loader` exhausted all possible target paths, and none of them existed. If this
occurs, something is wrong with your packaging or the filesystem is borked.
