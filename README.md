# hwcaps-loader

hwcaps-loader is as a monkey-patch solution for dealing with optimized binaries in Linux distributions.

Programs which are normally placed at `/usr/bin` or `/usr/libexec` should be replaced by symlinks which lead to `/usr/bin/hwcaps-loader`.

When the symlink is executed, `/usr/bin/hwcaps-loader` will determine which program the system attempted to execute,
then execute its equivalent at `/usr/hwcaps/($featureset)/(...)`, depending on the host cpu's capabilities.

**This is currently an incomplete proof of concept!** <u>Rust nightly is required</u> and no execution or featureset check occurs yet.

In order to reduce the space footprint, hwcaps-loader does not link against Rust's stdlib.
