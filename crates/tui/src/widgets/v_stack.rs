use crate::component::{ChildPlacement, Component, ComponentNode, LayoutSnapshot};
use crate::protocol::{ClipRect, ComponentError, ComponentResult, LayoutContext, Offset};

use super::StackAlign;

/// 纵向子项的高度，单位为终端行。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum StackHeight {
    /// 按完整内容高度分配；有高度上限时按子节点顺序裁减。
    #[default]
    Auto,
    Fixed(u16),
    /// 按权重分享剩余高度。没有高度上限时按内容高度处理。
    Fill(u16),
}

/// 纵向排列子节点，支持间距、行高分配和横向对齐。
/// 配置按子节点位置对应；默认 Auto 高度、父宽度和 Start 对齐。
#[derive(Debug, Default, Clone)]
pub struct VStack {
    heights: Vec<StackHeight>,
    widths: Vec<u16>,
    gap: u16,
    align: StackAlign,
    entries: Option<Vec<super::StackEntry>>,
}

impl VStack {
    pub fn with_entries(mut self, entries: impl IntoIterator<Item = super::StackEntry>) -> Self {
        self.entries = Some(entries.into_iter().collect());
        self
    }
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_heights(mut self, heights: impl IntoIterator<Item = StackHeight>) -> Self {
        self.heights = heights.into_iter().collect();
        self
    }

    /// 子项的宽度上限；缺失项使用父宽度，超出父宽度的值会压缩。
    pub fn with_widths(mut self, widths: impl IntoIterator<Item = u16>) -> Self {
        self.widths = widths.into_iter().collect();
        self
    }

    pub fn with_gap(mut self, gap: u16) -> Self {
        self.gap = gap;
        self
    }

    pub fn with_align(mut self, align: StackAlign) -> Self {
        self.align = align;
        self
    }
    fn flex_layout(
        &self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
        entries: Vec<super::StackEntry>,
    ) -> ComponentResult<LayoutSnapshot> {
        let indexes: Vec<_> = (0..children.len())
            .filter(|i| {
                entries
                    .get(*i)
                    .copied()
                    .unwrap_or_default()
                    .is_visible(context)
            })
            .collect();
        let specs: Vec<_> = indexes
            .iter()
            .map(|i| entries.get(*i).copied().unwrap_or_default())
            .collect();
        let mut nodes = Vec::new();
        for i in &indexes {
            let width = if self.align == StackAlign::Stretch {
                context.width
            } else {
                self.widths
                    .get(*i)
                    .copied()
                    .unwrap_or(context.width)
                    .min(context.width)
            };
            nodes.push(children[*i].layout(&LayoutContext {
                width,
                available_height: None,
                ..*context
            })?);
        }
        let intrinsic: Vec<_> = nodes.iter().map(|n| n.snapshot.height).collect();
        let sizes = super::stack::allocate(&specs, &intrinsic, context.available_height, self.gap)?;
        let total = sizes
            .iter()
            .fold(
                indexes
                    .len()
                    .saturating_sub(1)
                    .checked_mul(usize::from(self.gap)),
                |sum, size| sum.and_then(|v| v.checked_add(*size)),
            )
            .ok_or_else(|| ComponentError::InvalidLayout {
                reason: "VStack height overflow".into(),
            })?;
        i64::try_from(total).map_err(|_| ComponentError::InvalidLayout {
            reason: "VStack height exceeds i64".into(),
        })?;
        let height = context
            .available_height
            .map_or(total, |limit| total.min(usize::from(limit)));
        let mut row = 0usize;
        let mut placements = Vec::new();
        for ((index, mut node), size) in indexes.into_iter().zip(nodes).zip(sizes) {
            if let Ok(limit) = u16::try_from(size) {
                node = children[index].layout(&LayoutContext {
                    width: node.snapshot.width,
                    available_height: Some(limit),
                    ..*context
                })?;
            }
            let extra = context.width.saturating_sub(node.snapshot.width);
            let column = i32::from(match self.align {
                StackAlign::Start | StackAlign::Stretch => 0,
                StackAlign::Center => extra / 2,
                StackAlign::End => extra,
            });
            let y = row.min(height) as i64;
            placements.push(ChildPlacement {
                offset: Offset { column, row: y },
                clip: Some(ClipRect {
                    column,
                    row: y,
                    width: node.snapshot.width,
                    height: size.min(height.saturating_sub(row)),
                }),
                node,
            });
            row = row
                .saturating_add(size)
                .saturating_add(usize::from(self.gap));
        }
        Ok(LayoutSnapshot {
            width: context.width,
            height,
            children: placements,
            ..LayoutSnapshot::default()
        })
    }
}

impl Component for VStack {
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        let error = || ComponentError::InvalidLayout {
            reason: "VStack height overflow".into(),
        };
        if let Some(entries) = &self.entries {
            return self.flex_layout(context, children, entries.clone());
        }
        let gap_total = children
            .len()
            .saturating_sub(1)
            .checked_mul(usize::from(self.gap))
            .ok_or_else(error)?;
        let mut nodes = Vec::with_capacity(children.len());
        let mut sizes = Vec::with_capacity(children.len());
        let specs: Vec<_> = (0..children.len())
            .map(|i| self.heights.get(i).copied().unwrap_or_default())
            .collect();
        for (index, child) in children.iter_mut().enumerate() {
            let width = if self.align == StackAlign::Stretch {
                context.width
            } else {
                self.widths
                    .get(index)
                    .copied()
                    .unwrap_or(context.width)
                    .min(context.width)
            };
            let node = child.layout(&LayoutContext {
                width,
                available_height: None,
                ..*context
            })?;
            let size = match specs[index] {
                StackHeight::Fixed(value) => usize::from(value),
                StackHeight::Fill(0) => 0,
                _ => node.snapshot.height,
            };
            sizes.push(size);
            nodes.push(node);
        }
        if let Some(limit) = context.available_height {
            let mut remaining = usize::from(limit).saturating_sub(gap_total);
            let mut weight = 0u128;
            for (index, spec) in specs.iter().enumerate() {
                match spec {
                    StackHeight::Fill(value) => {
                        sizes[index] = 0;
                        weight += u128::from(*value);
                    }
                    _ => {
                        sizes[index] = sizes[index].min(remaining);
                        remaining -= sizes[index];
                    }
                }
            }
            if let Some(weight) = std::num::NonZeroU128::new(weight) {
                let mut assigned = 0usize;
                let mut remainders = Vec::new();
                for (index, spec) in specs.iter().enumerate() {
                    if let StackHeight::Fill(value) = spec
                        && *value > 0
                    {
                        let numerator = remaining as u128 * u128::from(*value);
                        sizes[index] = (numerator / weight.get()) as usize;
                        assigned += sizes[index];
                        remainders.push((index, numerator % weight.get()));
                    }
                }
                remainders.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                for (index, _) in remainders.into_iter().take(remaining - assigned) {
                    sizes[index] += 1;
                }
            }
        }
        // 确定分配高度后再布局，滚动/编辑组件和嵌套 Stack 能收到实际视口高度。
        for (index, child) in children.iter_mut().enumerate() {
            let limit = if context.available_height.is_some() {
                Some(sizes[index] as u16)
            } else {
                match specs[index] {
                    StackHeight::Fixed(value) => Some(value),
                    StackHeight::Fill(0) => Some(0),
                    _ => None,
                }
            };
            if let Some(limit) = limit {
                nodes[index] = child.layout(&LayoutContext {
                    width: nodes[index].snapshot.width,
                    available_height: Some(limit),
                    ..*context
                })?;
            }
        }
        let total = sizes.iter().try_fold(gap_total, |sum, size| {
            sum.checked_add(*size).ok_or_else(error)
        })?;
        let height = context
            .available_height
            .map_or(total, |limit| total.min(usize::from(limit)));
        i64::try_from(total).map_err(|_| error())?;
        let mut row = 0usize;
        let mut placements = Vec::with_capacity(nodes.len());
        for (node, size) in nodes.into_iter().zip(sizes) {
            let extra = context.width.saturating_sub(node.snapshot.width);
            let column = i32::from(match self.align {
                StackAlign::Start | StackAlign::Stretch => 0,
                StackAlign::Center => extra / 2,
                StackAlign::End => extra,
            });
            let y = row.min(height) as i64;
            placements.push(ChildPlacement {
                offset: Offset { column, row: y },
                clip: Some(ClipRect {
                    column,
                    row: y,
                    width: node.snapshot.width,
                    height: size.min(height.saturating_sub(row)),
                }),
                node,
            });
            row = row
                .checked_add(size)
                .and_then(|v| v.checked_add(usize::from(self.gap)))
                .ok_or_else(error)?;
        }
        Ok(LayoutSnapshot {
            width: context.width,
            height,
            children: placements,
            ..LayoutSnapshot::default()
        })
    }
}
