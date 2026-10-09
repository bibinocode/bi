use crate::component::{ChildPlacement, Component, ComponentNode, LayoutSnapshot};
use crate::protocol::{ClipRect, ComponentError, ComponentResult, LayoutContext, Offset};

/// 横向子项的宽度。Fixed 按子节点顺序优先分配，Fill 分享剩余宽度。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackWidth {
    Fixed(u16),
    /// 权重为零时不分配宽度。
    Fill(u16),
}

/// 子项的交叉轴位置：HStack 中为纵向，VStack 中为横向。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum StackAlign {
    #[default]
    Start,
    Center,
    End,
    Stretch,
}

/// 横向容器。宽度配置按当前子节点顺序对应，未配置的子项默认为 Fill(1)。
/// 高度取最高子项并受父高度限制；每个子项单独裁剪，保留其身份和焦点。
#[derive(Debug, Default, Clone)]
pub struct HStack {
    widths: Vec<StackWidth>,
    gap: u16,
    align: StackAlign,
    entries: Option<Vec<super::StackEntry>>,
}

impl HStack {
    /// 使用弹性配置替代 with_widths 的固定优先分配规则。
    pub fn with_entries(mut self, entries: impl IntoIterator<Item = super::StackEntry>) -> Self {
        self.entries = Some(entries.into_iter().collect());
        self
    }
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_widths(mut self, widths: impl IntoIterator<Item = StackWidth>) -> Self {
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

    fn allocate(&self, width: u16, count: usize) -> Vec<u16> {
        // 间距也占宽度；终端太窄时，靠后的间距与子项被裁掉。
        let gap_total = count
            .saturating_sub(1)
            .saturating_mul(usize::from(self.gap));
        let mut remaining = usize::from(width).saturating_sub(gap_total);
        let specs: Vec<_> = (0..count)
            .map(|index| {
                self.widths
                    .get(index)
                    .copied()
                    .unwrap_or(StackWidth::Fill(1))
            })
            .collect();
        let mut sizes = vec![0; count];
        let mut total_weight = 0u128;
        for (index, spec) in specs.iter().enumerate() {
            match spec {
                StackWidth::Fixed(value) => {
                    sizes[index] = remaining.min(usize::from(*value)) as u16;
                    remaining -= usize::from(sizes[index]);
                }
                StackWidth::Fill(weight) => total_weight += u128::from(*weight),
            }
        }
        if total_weight == 0 {
            return sizes;
        }
        let mut remainders = Vec::new();
        let mut assigned = 0usize;
        for (index, spec) in specs.iter().enumerate() {
            if let StackWidth::Fill(weight) = spec
                && *weight > 0
            {
                let numerator = remaining as u128 * u128::from(*weight);
                sizes[index] = (numerator / total_weight) as u16;
                assigned += usize::from(sizes[index]);
                remainders.push((index, numerator % total_weight));
            }
        }
        // 最大余数法，余数相同时按子节点顺序，避免列宽随遍历顺序偏斜。
        remainders.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (index, _) in remainders.into_iter().take(remaining - assigned) {
            sizes[index] += 1;
        }
        sizes
    }
}

impl Component for HStack {
    fn layout(
        &mut self,
        context: &LayoutContext,
        children: &mut [ComponentNode],
    ) -> ComponentResult<LayoutSnapshot> {
        let mut indexes: Vec<_> = (0..children.len()).collect();
        let widths = if let Some(entries) = &self.entries {
            indexes.retain(|i| {
                entries
                    .get(*i)
                    .copied()
                    .unwrap_or_default()
                    .is_visible(context)
            });
            let mut intrinsic = Vec::new();
            for i in &indexes {
                let measured = children[*i].layout(&LayoutContext {
                    available_height: None,
                    ..*context
                })?;
                let width = measured
                    .snapshot
                    .lines
                    .iter()
                    .map(|line| {
                        crate::utils::text::display_width(
                            &line
                                .spans
                                .iter()
                                .map(|s| s.text.as_str())
                                .collect::<String>(),
                        )
                    })
                    .collect::<ComponentResult<Vec<_>>>()?
                    .into_iter()
                    .max()
                    .unwrap_or(usize::from(measured.snapshot.width));
                intrinsic.push(width);
            }
            let specs: Vec<_> = indexes
                .iter()
                .map(|i| entries.get(*i).copied().unwrap_or_default())
                .collect();
            super::stack::allocate(&specs, &intrinsic, Some(context.width), self.gap)?
                .into_iter()
                .map(|v| v.min(usize::from(u16::MAX)) as u16)
                .collect()
        } else {
            self.allocate(context.width, children.len())
        };
        let mut placements = Vec::with_capacity(children.len());
        let mut column = 0usize;
        let mut natural_height = 0usize;
        for (index, width) in indexes.iter().zip(widths) {
            let child = &mut children[*index];
            let node = child.layout(&LayoutContext {
                width: width.min(context.width),
                available_height: None,
                ..*context
            })?;
            // 零宽子项仍布局，以清理其缓存，但不让它撑高容器。
            if width > 0 {
                natural_height = natural_height.max(node.snapshot.height);
            }
            let x = column.min(usize::from(context.width)) as i32;
            placements.push(ChildPlacement {
                offset: Offset { column: x, row: 0 },
                clip: Some(ClipRect {
                    column: x,
                    row: 0,
                    width: width.min(context.width.saturating_sub(x as u16)),
                    height: 0,
                }),
                node,
            });
            column = column
                .saturating_add(usize::from(width))
                .saturating_add(usize::from(self.gap));
        }
        let height = context.available_height.map_or(natural_height, |limit| {
            natural_height.min(usize::from(limit))
        });
        i64::try_from(height).map_err(|_| ComponentError::InvalidLayout {
            reason: "HStack height exceeds i64".into(),
        })?;
        for (index, placement) in indexes.iter().zip(&mut placements) {
            if self.align == StackAlign::Stretch
                && let Ok(limit) = u16::try_from(height)
            {
                placement.node = children[*index].layout(&LayoutContext {
                    width: placement.node.snapshot.width,
                    available_height: Some(limit),
                    ..*context
                })?;
                placement.node.snapshot.height = height;
            }
            let extra = height.saturating_sub(placement.node.snapshot.height);
            placement.offset.row = match self.align {
                StackAlign::Start | StackAlign::Stretch => 0,
                StackAlign::Center => (extra / 2) as i64,
                StackAlign::End => extra as i64,
            };
            if let Some(clip) = &mut placement.clip {
                clip.height = height;
            }
        }
        Ok(LayoutSnapshot {
            width: context.width,
            height,
            children: placements,
            ..LayoutSnapshot::default()
        })
    }
}
