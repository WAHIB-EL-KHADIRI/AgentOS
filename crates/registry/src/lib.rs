#![forbid(unsafe_code)]

pub mod registry;

pub use registry::{Registry, RegistryError, RegistryResult, ServiceDescriptor, ServiceHealth};
