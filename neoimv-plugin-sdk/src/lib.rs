pub mod ffi_types;
pub mod macros;
pub mod types;

pub use ffi_types::*;
pub use types::*;
pub use macros::{NeoimvPlugin, NeoimvDecoder, PluginInfoDef};

pub mod prelude {
    pub use crate::ffi_types::*;
    pub use crate::types::*;
    pub use crate::macros::{NeoimvPlugin, NeoimvDecoder, PluginInfoDef};
    pub use crate::declare_plugin;
}
