use crate::{
    component::{Component, ComponentNode, LayoutSnapshot},
    protocol::{ComponentError, ComponentResult, ImagePlacement, LayoutContext, Position, Size},
    widgets::Text,
};
use std::sync::Arc;

/// 编码图片组件；没有图片能力时显示替代文本，不执行文件 IO。
pub struct Image {
    id: String,
    mime: String,
    data: Arc<[u8]>,
    size: Size,
    alt: String,
}
impl Image {
    pub fn new(
        id: impl Into<String>,
        data: impl Into<Arc<[u8]>>,
        size: Size,
    ) -> ComponentResult<Self> {
        let data = data.into();
        let format = image::guess_format(&data).map_err(|e| ComponentError::InvalidContent {
            reason: e.to_string(),
        })?;
        let mime = format.to_mime_type().to_owned();
        if size.is_empty() {
            return Err(ComponentError::InvalidContent {
                reason: "image cell size must be nonzero".into(),
            });
        }
        Ok(Self {
            id: id.into(),
            mime,
            data,
            size,
            alt: "[图片]".into(),
        })
    }
    pub fn with_alt(mut self, alt: impl Into<String>) -> Self {
        self.alt = alt.into();
        self
    }
}
impl Component for Image {
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        if !children.is_empty() {
            return Err(ComponentError::InvalidLayout {
                reason: "Image does not accept children".into(),
            });
        }
        if context.capabilities.images.is_none() {
            return Text::new(&self.alt).layout(context, children);
        }
        let width = self.size.width.min(context.width);
        let height = context
            .available_height
            .map_or(self.size.height, |limit| limit.min(self.size.height));
        let mut snapshot = LayoutSnapshot {
            width: context.width,
            height: usize::from(height),
            ..LayoutSnapshot::default()
        };
        if width > 0 && height > 0 {
            snapshot.images.push(ImagePlacement {
                content_id: self.id.clone(),
                mime_type: self.mime.clone(),
                data: Arc::clone(&self.data),
                position: Position { column: 0, row: 0 },
                size: Size { width, height },
            });
        }
        Ok(snapshot)
    }
}
