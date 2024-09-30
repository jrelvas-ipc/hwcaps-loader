# Supported Targets and Platforms

The following targets are currently supported and tested:
- x86_64-unknown-linux-gnu 
- x86_64-unknown-linux-musl
- x86_64-unknown-none*

The following targets *should* work, but may be unstable:
- i686-unknown-linux-gnu
- i686-unknown-linux-musl
- i586-unknown-linux-gnu
- i586-unknown-linux-musl

* Requires Rust Nightly and unstable features

aarch64, riscv and other architectures are currently not supported.

Linux syscalls are called directly through Rust, with no libc abstraction, so porting to other Unix platforms may require some effort.
However, all the syscalls used are Unix standard.

Build requirements:
- Rust 1.81.0 Toolchain (or newer)
- GLIBC/MUSL headers and libraries (if using one of those targets)
- A computer with an energy source
