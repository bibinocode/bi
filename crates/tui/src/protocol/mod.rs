mod content;
mod context;
mod error;
mod event;
mod geometry;
mod host;
mod identity;
mod response;
mod state;
mod style;

pub use content::{Cursor, ImagePlacement, Line, Span};
pub use context::{Capabilities, ImageProtocol, LayoutContext, ScreenMode};
pub use error::{ComponentError, ComponentResult};
pub use geometry::{ClipRect, Offset, Position, Size};
pub use identity::{ComponentHandle, ComponentId, Generation};
pub use response::{EventResponse, PointerCapture};
pub use state::StateSnapshot;
pub use style::{Color, Style};

pub use event::{
    ComponentEvent, Key, KeyEvent, KeyKind, Modifiers, MouseButton, PointerEvent, PointerKind,
};

pub use host::{ComponentHost, MessageSender, QueuedMessage, ResourceCleanup, ResourceId};
