use std::hash::{Hash, Hasher};

#[cfg(target_pointer_width = "32")]
pub(crate) type fsize = f32;

#[cfg(target_pointer_width = "64")]
pub(crate) type fsize = f64;

/// Used for fsize::from_bits(usize as target_usize)
#[cfg(target_pointer_width = "32")]
pub(crate) type target_usize = u32;

/// Used for fsize::from_bits(usize as target_usize)
#[cfg(target_pointer_width = "64")]
pub(crate) type target_usize = u64;

// #[cfg(target_pointer_width = "32")]
// pub(crate) type uhalf = u16;
//
// #[cfg(target_pointer_width = "64")]
// pub(crate) type uhalf = u32;
