use crate::protocol::{ComponentError, ComponentResult, LayoutContext};

/// 可选弹性尺寸配置；basis=None 使用内容尺寸。尺寸单位随 Stack 主轴变化。
#[derive(Debug, Clone, Copy)]
pub struct StackEntry {
    pub basis: Option<u16>,
    pub grow: u16,
    pub shrink: u16,
    pub min: u16,
    pub max: Option<u16>,
    /// 父布局宽度小于此值时隐藏此项，不布局且不占间距。
    pub visible_from_width: u16,
}
impl Default for StackEntry {
    fn default() -> Self {
        Self {
            basis: None,
            grow: 0,
            shrink: 1,
            min: 0,
            max: None,
            visible_from_width: 0,
        }
    }
}
impl StackEntry {
    pub fn fixed(size: u16) -> Self {
        Self {
            basis: Some(size),
            shrink: 0,
            ..Self::default()
        }
    }
    pub fn fill(weight: u16) -> Self {
        Self {
            basis: Some(0),
            grow: weight,
            ..Self::default()
        }
    }
    pub fn is_visible(&self, context: &LayoutContext) -> bool {
        context.width >= self.visible_from_width
    }
}

pub(super) fn allocate(
    entries: &[StackEntry],
    intrinsic: &[usize],
    available: Option<u16>,
    gap: u16,
) -> ComponentResult<Vec<usize>> {
    for entry in entries {
        if entry.max.is_some_and(|max| max < entry.min) {
            return Err(ComponentError::InvalidLayout {
                reason: "stack maximum is below minimum".into(),
            });
        }
    }
    let mut sizes: Vec<_> = entries
        .iter()
        .zip(intrinsic)
        .map(|(entry, intrinsic)| {
            usize::from(
                entry
                    .basis
                    .unwrap_or((*intrinsic).min(usize::from(u16::MAX)) as u16),
            )
            .max(usize::from(entry.min))
            .min(entry.max.map_or(usize::MAX, usize::from))
        })
        .collect();
    // 无限文档中的 auto 尺寸不截到 u16。
    if available.is_none() {
        for (i, entry) in entries.iter().enumerate() {
            if entry.basis.is_none() {
                sizes[i] = intrinsic[i]
                    .max(usize::from(entry.min))
                    .min(entry.max.map_or(usize::MAX, usize::from));
            }
        }
        return Ok(sizes);
    }
    let target = usize::from(available.unwrap()).saturating_sub(
        entries
            .len()
            .saturating_sub(1)
            .saturating_mul(usize::from(gap)),
    );
    let total: usize = sizes.iter().sum();
    let growing = total < target;
    let mut remaining = total.abs_diff(target);
    while remaining > 0 {
        let mut candidates = Vec::new();
        for (i, entry) in entries.iter().enumerate() {
            let capacity = if growing {
                entry
                    .max
                    .map_or(target, usize::from)
                    .saturating_sub(sizes[i])
            } else {
                sizes[i].saturating_sub(usize::from(entry.min))
            };
            let weight = if growing {
                u128::from(entry.grow)
            } else {
                u128::from(entry.shrink) * sizes[i].max(1) as u128
            };
            if capacity > 0 && weight > 0 {
                candidates.push((i, capacity, weight));
            }
        }
        let sum: u128 = candidates.iter().map(|(_, _, w)| w).sum();
        let Some(sum) = std::num::NonZeroU128::new(sum) else {
            break;
        };
        let budget = remaining;
        let mut deltas = Vec::new();
        for (i, capacity, weight) in candidates {
            let numerator = budget as u128 * weight;
            deltas.push((
                i,
                capacity,
                (numerator / sum.get()) as usize,
                numerator % sum.get(),
            ));
        }
        deltas.sort_by(|a, b| b.3.cmp(&a.3).then(a.0.cmp(&b.0)));
        let floor_sum: usize = deltas.iter().map(|d| d.2).sum();
        for (rank, (i, capacity, delta, _)) in deltas.into_iter().enumerate() {
            let delta = (delta + usize::from(rank < budget - floor_sum))
                .min(capacity)
                .min(remaining);
            if growing {
                sizes[i] += delta;
            } else {
                sizes[i] -= delta;
            }
            remaining -= delta;
        }
    }
    Ok(sizes)
}
