#[path = "arch_generic.rs"]
mod arch_fallback;

#[cfg_attr(target_arch = "x86", path = "arch_x86.rs")]
#[cfg_attr(target_arch = "x86_64", path = "arch_x86.rs")]
mod arch_generic;

pub use arch_generic::*;
