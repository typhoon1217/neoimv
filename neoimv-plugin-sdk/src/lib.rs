pub mod ffi_types;
pub mod macros;
pub mod types;

pub use ffi_types::*;
pub use macros::{NeoimvDecoder, NeoimvPlugin, PluginInfoDef};
pub use types::*;

pub mod prelude {
    pub use crate::declare_plugin;
    pub use crate::ffi_types::*;
    pub use crate::macros::{NeoimvDecoder, NeoimvPlugin, PluginInfoDef};
    pub use crate::types::*;
}
