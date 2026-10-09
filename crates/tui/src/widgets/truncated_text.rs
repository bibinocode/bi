use crate::{
    component::{Component, ComponentNode, LayoutSnapshot},
    protocol::{ComponentError, ComponentResult, LayoutContext, Line, Span, Style},
    utils::text::truncate_text,
};

/// 只显示第一行，超出列宽时使用省略号。内边距交由 BoxContainer。
#[derive(Debug, Clone)]
pub struct TruncatedText {
    text: String,
    style: Style,
}

impl TruncatedText {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: Style::default(),
        }
    }
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
}

impl Component for TruncatedText {
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        if !children.is_empty() {
            return Err(ComponentError::InvalidLayout {
                reason: "TruncatedText does not accept children".into(),
            });
        }
        if context.width == 0 || context.available_height == Some(0) {
            return Ok(LayoutSnapshot::empty(context.width));
        }
        let normalized = self.text.replace("\r\n", "\n");
        let text = truncate_text(
            normalized.split('\n').next().unwrap_or_default(),
            context.width,
            "…",
        )?;
        Ok(LayoutSnapshot::from_lines(
            context.width,
            vec![Line {
                spans: vec![Span::styled(text, self.style)],
            }],
        ))
    }
}
