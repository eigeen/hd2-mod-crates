//! Compatibility exports for callers that still use the former browser-oriented namespace.

pub use crate::operations::*;

pub mod equipment {
    pub use crate::operations::equipment::*;
}

pub mod equipment_graph {
    pub use crate::operations::equipment_graph::*;
}

pub mod head_armor_merge {
    pub use crate::operations::head_armor_merge::*;
}

pub mod mapping_preview {
    pub use crate::operations::mapping_preview::*;
}

pub mod migration {
    pub use crate::operations::migration::*;
}

pub mod patch_merge {
    pub use crate::operations::patch_merge::*;
}

pub mod repatch {
    pub use crate::operations::repatch::*;
}

pub mod unified_migration {
    pub use crate::operations::unified_migration::*;
}
