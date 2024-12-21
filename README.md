# hwcaps-loader

## ⚠️ WARNING: This project is a work-in-progress.

`hwcaps-loader` is a solution for dealing with optimized binaries in Linux distributions.

---

Newer CPU variants bring new instructions and additional registers. The compiler can take advantage of those capabilities to generate more performant code. In particular, using vectorized code (SIMD processing in X86_64 and other architectures) can give a big performance benefit.

`x86-64-v2` and `x86-64-v3` processors are exceedingly common these days, and `x86-64-v4` processors are also being sold, so providing optimized binaries is increasingly more important.

Typically, Linux distributions only ship a single version of every program... one with wide support for different hardware variants. This effectively means that code cannot be compiled in a way that requires newer optional CPU features. Unfortunately, this means that newer hardware often misses out on extra performance and lower latency.

Most programs and libraries already dynamically take advantage of CPU intrinsics, because they are linked to a glibc, which embeds multiple optimized variants of commonly functions for memory copying or string processing and selects the most appropriate one at runtime. Some other "number crunching" libraries and programs implement similar logic internally. Nevertheless, most programs and libraries *in their own code* do not support dynamic selection of optimized functions.

Dealing with optimized dynamic libraries is relatively simple. They are loaded by the dynamic linker, which allows multiple versions to be installed and will simply load the most appropriate version based on hardware capabilities. [This is implemented through the glibc-hwcaps mechanism](https://antlarr.io/2021/03/hackweek-20-glibc-hwcaps-in-opensuse/).

But what about binary (executable) files? There's no "loader" for those, and you can't have multiple binaries in the same path, so you must add some kind of indirection.

This is where `hwcaps-loader` comes in. `hwcaps-loader` is a very small program which only has a single purpose: execute the best binary supported by the machine.
Distro packages which benefit from feature level optimizations can opt into this system by providing binaries for each desired feature level and creating a symlink to `/usr/bin/hwcaps-loader` in the path which the binary would traditionally go in.

This provides a very simple way for distributions to experiment with optimized binaries in select packages!

- Only a single, tiny (<20kB) `hwcaps-loader` binary is required to provide the loading mechanism for *every* desired program. It's also independent, having no sort of runtime/install dependencies (not even on libc, if needed!).
- It's an opt-in mechanism, so it requires very little infrastructure changes and won't introduce danger of breaking unrelated packages.
- It's written in low-level Rust (no libstd, minimal amount of crates), so the amount of latency added to program execution is extremely small (100-1000μs), especially compared to other proposed "loading mechanisms".

---

The following alternative proposals have been considered:

- Only have a single binary installed at any time: this is the most straightforward solution, but it *breaks* the operating system's portability. If the hardware changes and it has a lower feature level, programs which were optimized will simply stop working.
- Use a bash/script wrapper to pick the best binary: this alternative fixes the issues above, but also adds a script for *every* binary which opts into the system. There also concerns with performance and launch latency, especially if there's any `$PATH` modifications.
- Systemd+OverlayFS solution: this is the best approach long-term, providing minimal latency and indirection, but would also require a significant amount of development time and effort.
