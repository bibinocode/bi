mod flatten;
mod transform;
mod validate;

pub use flatten::{FlatNode, flatten_layout, hit_test};
pub use transform::{ResolvedPlacement, intersect_rect, resolve_child};

pub use validate::validate_layout;
