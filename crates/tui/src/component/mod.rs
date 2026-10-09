#[path = "trait.rs"]
mod component_trait;

mod node;
mod snapshot;

pub use component_trait::Component;
pub use node::ComponentNode;
pub use snapshot::{ChildPlacement, LayoutNode, LayoutSnapshot};
