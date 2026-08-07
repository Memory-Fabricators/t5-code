#![no_std]
#![feature(core_io)]
#![feature(alloc_io)]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod core;
