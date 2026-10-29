#[test]
fn core_and_configuration_easing_names_agree() {
    let core: std::collections::BTreeSet<_> = taskbarx_core::EASING_NAMES.iter().copied().collect();
    let config: std::collections::BTreeSet<_> =
        taskbarx_config::EASING_NAMES.iter().copied().collect();
    assert_eq!(core, config);
    for name in core {
        assert!(
            taskbarx_core::animated_position(0, 10, 50, 100, name).is_ok(),
            "{name}"
        );
        assert!(taskbarx_core::easing(name, 0.5).is_ok(), "{name}");
    }
}

#[test]
fn default_settings_match_runtime_contract() {
    let s = taskbarx_config::Settings::default();
    s.validate().unwrap();
    assert_eq!(s.schema_version, 1);
    assert!(s.center && s.primary && s.secondary && !s.center_between);
    assert_eq!((s.primary_offset, s.secondary_offset), (0, 0));
    assert_eq!(s.animation, "cubiceaseinout");
    assert_eq!((s.animation_ms, s.poll_ms), (300, 400));
    assert_eq!(s.style, "default");
    assert_eq!(s.color, [0, 0, 0, 128]);
}
