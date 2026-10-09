use bi_tui::protocol::ComponentResult;
use bi_tui::runtime::ComponentRegistry;

fn main() -> ComponentResult<()> {
    let mut registry = ComponentRegistry::new();

    let original = registry.begin_create()?;

    assert!(registry.is_pending(original));
    assert!(!registry.is_active(original));

    assert_eq!(registry.commit(original)?, None);
    assert!(registry.is_active(original));

    // 第一次替换失败。
    let failed = registry.begin_replace(original)?;
    registry.abort(failed)?;

    assert!(registry.is_active(original));
    assert!(!registry.is_pending(failed));

    // 第二次替换获得新的 generation。
    let replacement = registry.begin_replace(original)?;

    assert_ne!(failed.generation, replacement.generation);

    let previous = registry.commit(replacement)?;

    assert_eq!(previous, Some(original));
    assert!(!registry.is_active(original));
    assert!(registry.is_active(replacement));

    registry.unregister(replacement)?;

    assert!(!registry.is_active(replacement));

    Ok(())
}
