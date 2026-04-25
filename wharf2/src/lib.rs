pub mod errors;

// These modules are public to avoid adding #[allow(unused)] everywhere
// Once these are used by other modules, they should be made private
pub mod magic;
pub mod protos;

mod binaries;
