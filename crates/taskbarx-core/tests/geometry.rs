use taskbarx_core::{
    center_target, center_target_with_overlap, legal_position_range, requested_center, AxisLayout,
};

#[test]
fn overlap_is_detected_before_clamping_and_fallback_is_legal_not_zero() {
    let layout = AxisLayout {
        bar_start: -1000,
        bar_end: 0,
        free_start: -900,
        free_end: -100,
        parent_start: -950,
        content_offset: 10,
        content_len: 200,
    };
    assert_eq!(legal_position_range(&layout), Some((40, 640)));
    assert_eq!(requested_center(&layout, 301, false), Some(641));
    assert_eq!(center_target(&layout, 301, false), Some(640));
    assert_eq!(
        center_target_with_overlap(&layout, 301, false, true),
        Some(40)
    );
    assert_eq!(
        center_target_with_overlap(&layout, 300, false, true),
        Some(640)
    );
    for shift in [-1000, 0, 1000] {
        let translated = translate(layout, shift);
        for between in [false, true] {
            for offset in [i32::MIN, -100, 0, 100, i32::MAX] {
                assert_eq!(
                    center_target_with_overlap(&translated, offset, between, false),
                    center_target(&translated, offset, between)
                );
                let position =
                    center_target_with_overlap(&translated, offset, between, true).unwrap();
                let (start, end) = occupied(&translated, position);
                assert!(
                    start >= i64::from(translated.free_start)
                        && end <= i64::from(translated.free_end)
                );
            }
        }
    }
    assert_eq!(
        center_target_with_overlap(
            &AxisLayout {
                content_len: 1000,
                ..layout
            },
            i32::MAX,
            false,
            true
        ),
        None
    );
}

#[test]
fn overlap_fallback_preserves_negative_parent_relative_positions_and_wide_arithmetic() {
    let layout = primary();
    assert_eq!(
        center_target_with_overlap(&layout, i32::MAX, false, true),
        Some(-12)
    );
    let extreme = AxisLayout {
        bar_start: i32::MIN,
        bar_end: i32::MAX,
        free_start: i32::MIN,
        free_end: i32::MAX,
        parent_start: i32::MAX,
        content_offset: i32::MAX,
        content_len: 1,
    };
    let target = center_target_with_overlap(&extreme, i32::MAX, false, true).unwrap();
    let range = legal_position_range(&extreme).unwrap();
    assert!((range.0..=range.1).contains(&target));
}

fn primary() -> AxisLayout {
    AxisLayout {
        bar_start: 0,
        bar_end: 1920,
        free_start: 160,
        free_end: 1680,
        parent_start: 160,
        content_offset: 12,
        content_len: 480,
    }
}

fn occupied(layout: &AxisLayout, position: i32) -> (i64, i64) {
    let start =
        i64::from(layout.parent_start) + i64::from(position) + i64::from(layout.content_offset);
    (start, start + i64::from(layout.content_len))
}

fn translate(layout: AxisLayout, delta: i32) -> AxisLayout {
    AxisLayout {
        bar_start: layout.bar_start + delta,
        bar_end: layout.bar_end + delta,
        free_start: layout.free_start + delta,
        free_end: layout.free_end + delta,
        parent_start: layout.parent_start + delta,
        ..layout
    }
}

#[test]
fn primary_horizontal_centers_entire_content_in_parent_coordinates() {
    let layout = primary();
    assert_eq!(legal_position_range(&layout), Some((-12, 1028)));
    assert_eq!(center_target(&layout, 0, false), Some(548));
    assert_eq!(occupied(&layout, 548), (720, 1200));
    assert_eq!(center_target(&layout, 0, true), Some(508));
    assert_eq!(occupied(&layout, 508), (680, 1160));
}

#[test]
fn secondary_horizontal_uses_its_own_parent_and_reserved_controls() {
    let layout = AxisLayout {
        bar_start: 1920,
        bar_end: 3840,
        free_start: 1968,
        free_end: 3744,
        parent_start: 1968,
        content_offset: 8,
        content_len: 320,
    };
    assert_eq!(legal_position_range(&layout), Some((-8, 1448)));
    assert_eq!(center_target(&layout, 0, false), Some(744));
    assert_eq!(center_target(&layout, 0, true), Some(720));
    assert_eq!(center_target(&layout, -25, true), Some(695));

    let negative_monitor = translate(layout, -3840);
    assert_eq!(negative_monitor.bar_start, -1920);
    assert_eq!(negative_monitor.bar_end, 0);
    assert_eq!(legal_position_range(&negative_monitor), Some((-8, 1448)));
    assert_eq!(center_target(&negative_monitor, 0, false), Some(744));
    assert_eq!(center_target(&negative_monitor, 0, true), Some(720));
}

#[test]
fn vertical_taskbars_use_identical_axis_math_including_monitors_above_primary() {
    let vertical = AxisLayout {
        bar_start: 0,
        bar_end: 1080,
        free_start: 48,
        free_end: 1008,
        parent_start: 48,
        content_offset: 4,
        content_len: 240,
    };
    for delta in [-2160, -1080, 0, 1080] {
        let layout = translate(vertical, delta);
        assert_eq!(legal_position_range(&layout), Some((-4, 716)));
        assert_eq!(center_target(&layout, 0, false), Some(368));
        assert_eq!(center_target(&layout, 0, true), Some(356));
        assert_eq!(center_target(&layout, 16, true), Some(372));
    }
}

#[test]
fn crossing_screen_origin_does_not_use_absolute_coordinates() {
    let layout = AxisLayout {
        bar_start: -500,
        bar_end: 500,
        free_start: -400,
        free_end: 400,
        parent_start: -300,
        content_offset: 0,
        content_len: 200,
    };
    assert_eq!(legal_position_range(&layout), Some((-100, 500)));
    for between in [false, true] {
        assert_eq!(center_target(&layout, 0, between), Some(200));
        assert_eq!(occupied(&layout, 200), (-100, 100));
    }
}

#[test]
fn different_reserved_widths_shift_between_center_but_not_bar_center() {
    for (free_start, free_end, target) in [(100, 900, 400), (200, 900, 450), (100, 800, 350)] {
        let layout = AxisLayout {
            bar_start: 0,
            bar_end: 1000,
            free_start,
            free_end,
            parent_start: 0,
            content_offset: 0,
            content_len: 200,
        };
        assert_eq!(center_target(&layout, 0, false), Some(400));
        assert_eq!(center_target(&layout, 0, true), Some(target));
    }
}

#[test]
fn signed_offsets_and_both_edges_are_clamped() {
    let layout = primary();
    for (offset, whole, between) in [
        (0, 548, 508),
        (40, 588, 548),
        (-40, 508, 468),
        (i32::MIN, -12, -12),
        (i32::MAX, 1028, 1028),
    ] {
        assert_eq!(center_target(&layout, offset, false), Some(whole));
        assert_eq!(center_target(&layout, offset, true), Some(between));
    }
}

#[test]
fn offset_is_applied_before_clamping_an_obstructed_center() {
    let layout = AxisLayout {
        bar_start: 0,
        bar_end: 1000,
        free_start: 600,
        free_end: 1000,
        parent_start: 0,
        content_offset: 0,
        content_len: 200,
    };
    assert_eq!(legal_position_range(&layout), Some((600, 800)));
    assert_eq!(center_target(&layout, 0, false), Some(600));
    assert_eq!(center_target(&layout, 50, false), Some(600));
    assert_eq!(center_target(&layout, 300, false), Some(700));
    assert_eq!(center_target(&layout, 500, false), Some(800));
    assert_eq!(center_target(&layout, 0, true), Some(700));

    let opposite_edge = AxisLayout {
        free_start: 0,
        free_end: 300,
        ..layout
    };
    assert_eq!(center_target(&opposite_edge, -50, false), Some(100));
    assert_eq!(center_target(&opposite_edge, 0, true), Some(50));
}

#[test]
fn leading_margin_is_not_counted_as_occupied_content() {
    let layout = AxisLayout {
        bar_start: 0,
        bar_end: 1000,
        free_start: 0,
        free_end: 1000,
        parent_start: 0,
        content_offset: 30,
        content_len: 240,
    };
    assert_eq!(legal_position_range(&layout), Some((-30, 730)));
    assert_eq!(center_target(&layout, 0, false), Some(350));
    assert_eq!(occupied(&layout, 350), (380, 620));
    // Signed content offsets are valid; only the measured occupied span matters.
    let negative_margin = AxisLayout {
        content_offset: -30,
        ..layout
    };
    assert_eq!(legal_position_range(&negative_margin), Some((30, 790)));
    assert_eq!(center_target(&negative_margin, 0, false), Some(410));
    assert_eq!(occupied(&negative_margin, 410), (380, 620));
}

#[test]
fn full_last_button_is_used_in_center_and_fit_checks() {
    // Three 48-pixel buttons with two 8-pixel gaps occupy 160 pixels, not merely
    // the 112-pixel distance to the last button's leading edge.
    let layout = AxisLayout {
        bar_start: 0,
        bar_end: 400,
        free_start: 40,
        free_end: 360,
        parent_start: 40,
        content_offset: 6,
        content_len: 3 * 48 + 2 * 8,
    };
    assert_eq!(center_target(&layout, 0, false), Some(74));
    assert_eq!(occupied(&layout, 74), (120, 280));
    assert_eq!(
        legal_position_range(&AxisLayout {
            free_end: 199,
            ..layout
        }),
        None
    );
}

#[test]
fn exact_fit_has_one_legal_position_even_with_offsets() {
    let layout = AxisLayout {
        bar_start: -50,
        bar_end: 700,
        free_start: 100,
        free_end: 300,
        parent_start: 60,
        content_offset: 10,
        content_len: 200,
    };
    assert_eq!(legal_position_range(&layout), Some((30, 30)));
    for between in [false, true] {
        for offset in [i32::MIN, -10, 0, 10, i32::MAX] {
            assert_eq!(center_target(&layout, offset, between), Some(30));
        }
    }
}

#[test]
fn odd_sizes_round_toward_span_start_without_losing_translation_invariance() {
    let layout = AxisLayout {
        bar_start: 0,
        bar_end: 1001,
        free_start: 30,
        free_end: 900,
        parent_start: 20,
        content_offset: 3,
        content_len: 100,
    };
    for shift in [-2000, -1001, -1, 0, 1, 1001, 2000] {
        let translated = translate(layout, shift);
        assert_eq!(center_target(&translated, 0, false), Some(427));
        assert_eq!(center_target(&translated, 0, true), Some(392));
    }
}

#[test]
fn changing_parent_origin_changes_only_parent_relative_coordinates() {
    let layout = primary();
    let changed_parent = AxisLayout {
        parent_start: 500,
        ..layout
    };
    let expected_shift = layout.parent_start - changed_parent.parent_start;
    for between in [false, true] {
        let original = center_target(&layout, 17, between).unwrap();
        let changed = center_target(&changed_parent, 17, between).unwrap();
        assert_eq!(changed, original + expected_shift);
        assert_eq!(
            occupied(&layout, original),
            occupied(&changed_parent, changed)
        );
    }
}

#[test]
fn empty_reversed_outside_or_overfull_layouts_are_rejected() {
    let base = primary();
    let invalid = [
        AxisLayout {
            content_len: 0,
            ..base
        },
        AxisLayout {
            content_len: -1,
            ..base
        },
        AxisLayout {
            content_len: i32::MIN,
            ..base
        },
        AxisLayout {
            content_len: 1521,
            ..base
        },
        AxisLayout {
            content_len: i32::MAX,
            ..base
        },
        AxisLayout { bar_end: 0, ..base },
        AxisLayout {
            bar_end: -1,
            ..base
        },
        AxisLayout {
            free_end: 160,
            ..base
        },
        AxisLayout {
            free_end: 159,
            ..base
        },
        AxisLayout {
            free_start: -1,
            ..base
        },
        AxisLayout {
            free_end: 1921,
            ..base
        },
    ];
    for layout in invalid {
        assert_eq!(legal_position_range(&layout), None, "{layout:?}");
        for between in [false, true] {
            for offset in [i32::MIN, 0, i32::MAX] {
                assert_eq!(center_target(&layout, offset, between), None, "{layout:?}");
            }
        }
    }
}

#[test]
fn wide_screen_spans_and_huge_offsets_do_not_overflow() {
    let layout = AxisLayout {
        bar_start: i32::MIN,
        bar_end: i32::MAX,
        free_start: i32::MIN,
        free_end: i32::MAX,
        parent_start: 0,
        content_offset: 0,
        content_len: 1,
    };
    assert_eq!(
        legal_position_range(&layout),
        Some((i32::MIN, i32::MAX - 1))
    );
    for between in [false, true] {
        assert_eq!(center_target(&layout, 0, between), Some(-1));
        assert_eq!(center_target(&layout, i32::MIN, between), Some(i32::MIN));
        assert_eq!(
            center_target(&layout, i32::MAX, between),
            Some(i32::MAX - 1)
        );
    }
    let huge_origin = AxisLayout {
        parent_start: i32::MAX,
        content_offset: i32::MAX,
        ..layout
    };
    assert_eq!(
        legal_position_range(&huge_origin),
        Some((i32::MIN, i32::MIN))
    );
    assert_eq!(center_target(&huge_origin, i32::MAX, false), Some(i32::MIN));
}

#[test]
fn partially_representable_legal_intervals_keep_their_valid_portion() {
    let layout = AxisLayout {
        bar_start: i32::MIN,
        bar_end: i32::MAX,
        free_start: i32::MIN,
        free_end: i32::MAX,
        parent_start: -100,
        content_offset: 0,
        content_len: 1,
    };
    assert_eq!(
        legal_position_range(&layout),
        Some((i32::MIN + 100, i32::MAX))
    );
    assert_eq!(center_target(&layout, i32::MAX, true), Some(i32::MAX));
    let other_side = AxisLayout {
        parent_start: 100,
        ..layout
    };
    assert_eq!(
        legal_position_range(&other_side),
        Some((i32::MIN, i32::MAX - 101))
    );
    assert_eq!(center_target(&other_side, i32::MIN, true), Some(i32::MIN));
}

#[test]
fn wholly_unrepresentable_positions_return_none_instead_of_wrapping() {
    let too_positive = AxisLayout {
        bar_start: i32::MAX - 100,
        bar_end: i32::MAX,
        free_start: i32::MAX - 100,
        free_end: i32::MAX,
        parent_start: i32::MIN,
        content_offset: -100,
        content_len: 100,
    };
    let too_negative = AxisLayout {
        bar_start: i32::MIN,
        bar_end: i32::MIN + 100,
        free_start: i32::MIN,
        free_end: i32::MIN + 100,
        parent_start: i32::MAX,
        content_offset: 100,
        content_len: 100,
    };
    for layout in [too_positive, too_negative] {
        assert_eq!(legal_position_range(&layout), None);
        assert_eq!(center_target(&layout, 0, false), None);
        assert_eq!(center_target(&layout, i32::MAX, true), None);
    }
}

#[test]
fn large_content_margin_plus_length_is_widened_before_arithmetic() {
    let layout = AxisLayout {
        bar_start: 0,
        bar_end: i32::MAX,
        free_start: 0,
        free_end: i32::MAX,
        parent_start: 0,
        content_offset: i32::MAX,
        content_len: i32::MAX,
    };
    assert_eq!(legal_position_range(&layout), Some((-i32::MAX, -i32::MAX)));
    assert_eq!(center_target(&layout, 0, true), Some(-i32::MAX));
    assert_eq!(occupied(&layout, -i32::MAX), (0, i64::from(i32::MAX)));
}

#[test]
fn exhaustive_small_layouts_match_brute_force_occupied_span_constraints() {
    for bar_start in [-11, -1, 0, 7] {
        for bar_len in 1..=8 {
            for reserved_start in 0..bar_len {
                for reserved_end in 0..(bar_len - reserved_start) {
                    for content_len in 1..=9 {
                        for parent_start in [-8, 0, 9] {
                            for content_offset in [-3, 0, 4] {
                                let layout = AxisLayout {
                                    bar_start,
                                    bar_end: bar_start + bar_len,
                                    free_start: bar_start + reserved_start,
                                    free_end: bar_start + bar_len - reserved_end,
                                    parent_start,
                                    content_offset,
                                    content_len,
                                };
                                // These small inputs guarantee every legal position is in [-40, 40].
                                let legal: Vec<_> = (-40..=40)
                                    .filter(|&position| {
                                        let (start, end) = occupied(&layout, position);
                                        start >= i64::from(layout.free_start)
                                            && end <= i64::from(layout.free_end)
                                    })
                                    .collect();
                                let expected = legal.first().copied().zip(legal.last().copied());
                                assert_eq!(legal_position_range(&layout), expected, "{layout:?}");
                                for between in [false, true] {
                                    for offset in [-50, -1, 0, 1, 50] {
                                        let target = center_target(&layout, offset, between);
                                        match expected {
                                            None => assert_eq!(target, None),
                                            Some((min, max)) => {
                                                let target = target.unwrap();
                                                assert!((min..=max).contains(&target));
                                                let (start, end) = occupied(&layout, target);
                                                assert!(start >= i64::from(layout.free_start));
                                                assert!(end <= i64::from(layout.free_end));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
