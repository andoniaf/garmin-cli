#![recursion_limit = "256"]

pub mod cli;
pub mod client;
pub mod config;
pub mod db;
pub mod error;
pub mod models;
pub mod storage;
pub mod sync;
pub mod workout;

pub use error::{GarminError, Result};
pub use storage::{ParquetStore, Storage, SyncDb};
pub use sync::{SyncEngine, SyncOptions, SyncStats};
