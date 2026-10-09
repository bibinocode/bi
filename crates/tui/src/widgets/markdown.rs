use crate::{
    component::{Component, ComponentNode, LayoutSnapshot},
    protocol::{Color, ComponentError, ComponentResult, LayoutContext, Line, Span, Style},
    utils::styled_text::wrap_lines,
};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::sync::OnceLock;
use syntect::{easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet};

/// CommonMark/GFM 文档，结构化样式与代码语法高亮。原始 HTML 按文本显示，不执行。
pub struct Markdown {
    text: String,
}
impl Markdown {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }
    pub fn text(&self) -> &str {
        &self.text
    }
}

fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect::<String>()
        .replace('\t', "    ")
}
fn push_text(lines: &mut Vec<Line>, text: &str, style: Style, link: Option<String>) {
    for (index, part) in clean(text).split('\n').enumerate() {
        if index > 0 {
            lines.push(Line::default());
        }
        if lines.is_empty() {
            lines.push(Line::default());
        }
        if !part.is_empty() {
            lines.last_mut().unwrap().spans.push(Span {
                text: part.into(),
                style,
                hyperlink: link.clone(),
            });
        }
    }
}
fn break_line(lines: &mut Vec<Line>) {
    if lines.last().is_some_and(|l| !l.spans.is_empty()) {
        lines.push(Line::default());
    }
}
fn highlight(code: &str, language: &str) -> Vec<Line> {
    static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
    static THEMES: OnceLock<ThemeSet> = OnceLock::new();
    let syntaxes = SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines);
    let themes = THEMES.get_or_init(ThemeSet::load_defaults);
    let syntax = syntaxes
        .find_syntax_by_token(language)
        .unwrap_or_else(|| syntaxes.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, &themes.themes["base16-ocean.dark"]);
    clean(code)
        .split_inclusive('\n')
        .map(|line| {
            let spans = highlighter
                .highlight_line(line, syntaxes)
                .map(|tokens| {
                    tokens
                        .into_iter()
                        .filter_map(|(style, text)| {
                            let text = text.trim_end_matches('\n');
                            (!text.is_empty()).then(|| {
                                Span::styled(
                                    text,
                                    Style {
                                        foreground: Some(Color::Rgb(
                                            style.foreground.r,
                                            style.foreground.g,
                                            style.foreground.b,
                                        )),
                                        bold: style
                                            .font_style
                                            .contains(syntect::highlighting::FontStyle::BOLD),
                                        italic: style
                                            .font_style
                                            .contains(syntect::highlighting::FontStyle::ITALIC),
                                        underline: style
                                            .font_style
                                            .contains(syntect::highlighting::FontStyle::UNDERLINE),
                                        ..Style::default()
                                    },
                                )
                            })
                        })
                        .collect()
                })
                .unwrap_or_else(|_| vec![Span::plain(line.trim_end_matches('\n'))]);
            Line { spans }
        })
        .collect()
}
impl Component for Markdown {
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        if !children.is_empty() {
            return Err(ComponentError::InvalidLayout {
                reason: "Markdown does not accept children".into(),
            });
        }
        let mut lines = vec![Line::default()];
        let mut style = Style::default();
        let mut styles = Vec::new();
        let mut link = None;
        let mut links = Vec::new();
        let mut lists: Vec<Option<u64>> = Vec::new();
        let mut quote_depth = 0usize;
        let mut code: Option<(String, String)> = None;
        for event in Parser::new_ext(&self.text, Options::all()) {
            if let Some((language, text)) = &mut code {
                match event {
                    Event::End(TagEnd::CodeBlock) => {
                        let highlighted = highlight(text, language);
                        lines.extend(highlighted);
                        lines.push(Line::default());
                        code = None;
                    }
                    Event::Text(value) => text.push_str(&value),
                    _ => {}
                }
                continue;
            }
            match event {
                Event::Start(tag) => match tag {
                    Tag::Heading { .. } => {
                        break_line(&mut lines);
                        styles.push(style);
                        style.bold = true;
                        style.foreground = Some(Color::Indexed(6));
                    }
                    Tag::Emphasis => {
                        styles.push(style);
                        style.italic = true;
                    }
                    Tag::Strong => {
                        styles.push(style);
                        style.bold = true;
                    }
                    Tag::Strikethrough => {
                        styles.push(style);
                        style.crossed_out = true;
                    }
                    Tag::Link { dest_url, .. } => {
                        styles.push(style);
                        style.underline = true;
                        links.push(link.take());
                        link = Some(clean(&dest_url).replace('\n', ""));
                    }
                    Tag::Image { .. } => {
                        push_text(&mut lines, "[图片：", style, None);
                    }
                    Tag::CodeBlock(kind) => {
                        break_line(&mut lines);
                        let language = match kind {
                            CodeBlockKind::Fenced(info) => {
                                info.split_whitespace().next().unwrap_or("").to_owned()
                            }
                            _ => String::new(),
                        };
                        code = Some((language, String::new()));
                    }
                    Tag::Paragraph => {
                        if quote_depth > 0 {
                            break_line(&mut lines);
                            push_text(
                                &mut lines,
                                &"│ ".repeat(quote_depth),
                                Style { dim: true, ..style },
                                None,
                            );
                        } else if lists.is_empty() {
                            break_line(&mut lines);
                        }
                    }
                    Tag::Table(_) | Tag::TableRow => break_line(&mut lines),
                    Tag::BlockQuote(_) => {
                        break_line(&mut lines);
                        quote_depth += 1;
                    }
                    Tag::List(start) => {
                        break_line(&mut lines);
                        lists.push(start);
                    }
                    Tag::Item => {
                        break_line(&mut lines);
                        let prefix = match lists.last_mut() {
                            Some(Some(n)) => {
                                let prefix = format!("{n}. ");
                                *n += 1;
                                prefix
                            }
                            _ => "• ".into(),
                        };
                        push_text(
                            &mut lines,
                            &format!("{}{prefix}", "  ".repeat(lists.len().saturating_sub(1))),
                            style,
                            None,
                        );
                    }
                    _ => {}
                },
                Event::End(tag) => match tag {
                    TagEnd::Heading(_)
                    | TagEnd::Emphasis
                    | TagEnd::Strong
                    | TagEnd::Strikethrough => {
                        style = styles.pop().unwrap_or_default();
                        if matches!(tag, TagEnd::Heading(_)) {
                            break_line(&mut lines);
                        }
                    }
                    TagEnd::Link => {
                        style = styles.pop().unwrap_or_default();
                        link = links.pop().flatten();
                    }
                    TagEnd::Image => push_text(&mut lines, "]", style, None),
                    TagEnd::List(_) => {
                        lists.pop();
                        break_line(&mut lines);
                    }
                    TagEnd::BlockQuote(_) => {
                        quote_depth = quote_depth.saturating_sub(1);
                        break_line(&mut lines);
                    }
                    TagEnd::Paragraph | TagEnd::Item | TagEnd::TableRow => break_line(&mut lines),
                    TagEnd::TableCell => push_text(&mut lines, " │ ", style, None),
                    _ => {}
                },
                Event::Text(value) | Event::Html(value) | Event::InlineHtml(value) => {
                    push_text(&mut lines, &value, style, link.clone())
                }
                Event::Code(value) => push_text(
                    &mut lines,
                    &value,
                    Style {
                        foreground: Some(Color::Indexed(3)),
                        ..style
                    },
                    link.clone(),
                ),
                Event::SoftBreak => push_text(&mut lines, " ", style, link.clone()),
                Event::HardBreak => lines.push(Line::default()),
                Event::Rule => {
                    break_line(&mut lines);
                    push_text(
                        &mut lines,
                        &"─".repeat(usize::from(context.width)),
                        Style { dim: true, ..style },
                        None,
                    );
                    break_line(&mut lines);
                }
                Event::TaskListMarker(done) => {
                    push_text(&mut lines, if done { "[x] " } else { "[ ] " }, style, None)
                }
                Event::InlineMath(value) | Event::DisplayMath(value) => push_text(
                    &mut lines,
                    &crate::utils::latex::render_latex(&value).unwrap_or_else(|| value.to_string()),
                    Style {
                        italic: true,
                        ..style
                    },
                    None,
                ),
                Event::FootnoteReference(value) => {
                    push_text(&mut lines, &format!("[{value}]"), style, None)
                }
            }
        }
        while lines.last().is_some_and(|l| l.spans.is_empty()) {
            lines.pop();
        }
        let mut lines = wrap_lines(&lines, context.width)?;
        if let Some(limit) = context.available_height {
            lines.truncate(usize::from(limit));
        }
        Ok(LayoutSnapshot::from_lines(context.width, lines))
    }
}
