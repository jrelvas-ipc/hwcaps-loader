# hwcaps-loader

## ⚠️ WARNING: This project is a work-in-progress.

`hwcaps-loader` is a monkey-patch solution for dealing with optimized binaries in Linux distributions.

---

Typically, Linux distributions only ship a single version of every program... the one which is supported by the most users.

Unfortunately, this means that (relatively) newer hardware often misses out on extra performance and lower latency! By taking advantage of newer instructions, especially SIMD ones, in X86_64 and other architectures, you can get extra performance out of a program!

`X86-64-v2` and `x86-64-v3` processors are exceedingly common these days, so providing optimized binaries is increasingly more important.

Some programs and libraries are able to dynamically take advantage of CPU intrinsics, however, the vast majority of them cannot, even if the compiler is able to vectorize code, so they need to be built against newer feature levels.  A few distros have decided to tackle this issue.

Dealing with optimized dynamic libraries is relatively simple. Since all of them go through the dynamic linker, it can simply load the most appropriate version based on hardware capabilities. [This is implemented through the glibc-hwcaps mechanism](https://antlarr.io/2021/03/hackweek-20-glibc-hwcaps-in-opensuse/).

---

But what about binary (executable) files? There's no "loader" for those, and you can't have multiple binaries in the same path, so you must add some kind of indirection. The following proposals have been made, but they all have their drawbacks:

- Only have a single binary installed at any time: This is the most straightforward solution, but it *breaks* the operating system's portability. If the hardware changes and it has a lower feature level, programs which were optimized will simply stop working.
- Use a bash/script wrapper to pick the best binary: This alternative fixes the issues above, but also adds a script for *every* binary which opts into the system. There also concerns with performance and launch latency, especially if there's any `$PATH` modifications.
- Systemd+OverlayFS solution: This is the best approach long-term, providing minimal latency and indirection, but would also require a significant amount of development time and effort.

This is where `hwcaps-loader` comes in. `hwcaps-loader` is a very small program which only has a single purpose: execute the best binary supported by the machine. 
Distro packages which benefit from feature level optimizations can opt into this system by providing binaries for each desired feature level and creating a symlink to `/usr/bin/hwcaps-loader` in the path which the binary would traditionally go in. 

This provides a very simple way for distributions to experiment with optimized binaries in select packages!

- Only a single, tiny (<20kB) `hwcaps-loader` binary is required to provide the loading mechanism for *every* desired program. It's also independent, having no sort of runtime/install dependencies (not even on libc, if needed!).
- It's an opt-in mechanism, so it requires very little infrastructure changes and won't introduce danger of breaking unrelated packages.
- It's written in low-level Rust (no libstd, minimal amount of crates), so the amount of latency added to program execution is extremely small (100-1000μs), especially compared to other proposed "loading mechanisms".