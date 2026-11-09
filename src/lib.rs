//! Two offline checks for software that handles the Quran.
//!
//! - [`data`] reads the data a project stores and publishes and finds what
//!   another project would read wrongly: an unstated ayah numbering, a riwayah
//!   code no one else can resolve.
//! - [`names`] checks the names in code against the Quranic vocabulary naming
//!   standard.

mod args;
mod glob;
pub mod data;
pub mod names;
