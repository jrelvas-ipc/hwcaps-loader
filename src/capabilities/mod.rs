#[cfg_attr(target_arch = "x86", path = "arch_x86.rs")]
#[cfg_attr(target_arch = "x86_64", path = "arch_x86.rs")]
mod arch;

pub use arch::get_max_feature_level;
pub use arch::format_arch_name;
pub use arch::arch_name_changed;
pub use arch::HWCAPS_CHARS;
