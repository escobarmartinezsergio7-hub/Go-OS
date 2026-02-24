//! POLYVAL backends

#[cfg_attr(not(target_pointer_width = "64"), path = "backend/soft32.rs")]
#[cfg_attr(target_pointer_width = "64", path = "backend/soft64.rs")]
mod soft;

// Force software backend for UEFI
pub use crate::backend::soft::Polyval;
