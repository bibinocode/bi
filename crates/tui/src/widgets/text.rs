use crate::component::{Component, ComponentNode, LayoutSnapshot};
use crate::protocol::{ComponentError, ComponentResult, LayoutContext, Line, Span, Style};
use crate::utils::text::wrap_text;

/// 支持自动换行的纯文本组件。
#[derive(Debug, Clone)]
pub struct Text {
    text: String,
    style: Style,
}

impl Text {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: Style::default(),
        }
    }

    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn style(&self) -> Style {
        self.style
    }

    /// 修改文本；运行时接入后，调用方需要请求重绘。
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    /// 修改样式；运行时接入后，调用方需要请求重绘。
    pub fn set_style(&mut self, style: Style) {
        self.style = style;
    }
}

impl Component for Text {
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        if !children.is_empty() {
            return Err(ComponentError::InvalidLayout {
                reason: "Text does not accept child components".into(),
            });
        }

        // 保留原始文本，每次按当前宽度重新换行。
        let mut wrapped = wrap_text(&self.text, context.width)?;

        if let Some(limit) = context.available_height {
            wrapped.truncate(usize::from(limit));
        }

        let lines = wrapped
            .into_iter()
            .map(|text| Line {
                spans: vec![Span::styled(text, self.style)],
            })
            .collect();

        Ok(LayoutSnapshot::from_lines(context.width, lines))
    }
}
