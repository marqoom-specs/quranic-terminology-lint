//! The data check: Quran data in files, API schemas and SQL, read against the
//! Marqoom registries. It answers the questions a consumer of the data would
//! get wrong without asking: which riwayah, which ayah numbering, which codes.

mod check;
pub mod cli;
mod config;
mod detect;
mod findings;
mod registry;
mod report;
mod text;
mod values;
