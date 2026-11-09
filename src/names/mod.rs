//! The names check: names in code, data keys and documents against the
//! Quranic vocabulary naming standard. A name that breaks a rule is reported
//! with the name to use instead and the rule behind it.

mod check;
pub mod cli;
mod config;
mod files;
mod layout;
mod python;
mod report;
mod rules;
mod scan;
mod terms;
mod words;
