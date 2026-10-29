use std::collections::BTreeSet;
use taskbarx_core::{animated_position, easing, EASING_NAMES};

const FAMILIES: &[&str] = &[
    "quad", "cubic", "quart", "quint", "sine", "expo", "circ", "back", "elastic", "bounce",
];
const MODES: &[&str] = &["in", "out", "inout", "outin"];

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-12,
        "expected {expected:.17}, got {actual:.17}"
    );
}

#[test]
fn advertised_names_are_exactly_the_42_lower_case_upstream_selections() {
    let mut expected = BTreeSet::from(["none".to_owned(), "linear".to_owned()]);
    for family in FAMILIES {
        for mode in MODES {
            expected.insert(format!("{family}ease{mode}"));
        }
    }
    let actual: BTreeSet<_> = EASING_NAMES.iter().map(|name| (*name).to_owned()).collect();
    assert_eq!(EASING_NAMES.len(), 42);
    assert_eq!(actual.len(), 42);
    assert_eq!(actual, expected);
    assert!(EASING_NAMES.contains(&"cubiceaseinout"));
}

#[test]
fn every_curve_has_exact_endpoints_and_clamps_finite_input() {
    for &name in EASING_NAMES {
        for progress in [-f64::MAX, -100.0, -f64::MIN_POSITIVE, -0.0, 0.0] {
            assert_eq!(easing(name, progress), Ok(0.0), "{name} at {progress}");
        }
        for progress in [1.0, 1.0000000000000002, 100.0, f64::MAX] {
            assert_eq!(easing(name, progress), Ok(1.0), "{name} at {progress}");
        }
    }
}

#[test]
fn non_finite_inputs_are_errors_including_for_none() {
    for &name in EASING_NAMES {
        for progress in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(
                easing(name, progress).is_err(),
                "{name} accepted {progress}"
            );
        }
    }
}

#[test]
fn unknown_names_are_rejected_even_on_instantaneous_or_completed_frames() {
    for name in [
        "",
        "Linear",
        "None",
        "CubicEaseInOut",
        "cubic",
        "easein",
        "easeout",
        "quadease",
        "quadeaseinoutin",
        "quadeaseeasein",
        "cubeasein",
        "quadeaseIn",
        " quadeasein",
        "quadeasein ",
        "linear\0",
        "sine-ease-in",
        "未知",
    ] {
        for progress in [0.0, 0.25, 1.0, -1.0, 2.0] {
            assert!(easing(name, progress).is_err(), "accepted {name:?}");
        }
        for (from, to, elapsed, duration) in [
            (0, 100, 0, 100),
            (0, 100, 25, 100),
            (0, 100, 100, 100),
            (0, 100, u64::MAX, 100),
            (0, 100, 0, 0),
            (0, 100, u64::MAX, 0),
            (100, 100, 0, 100),
        ] {
            assert!(animated_position(from, to, elapsed, duration, name).is_err());
        }
    }
}

#[test]
fn quarter_progress_matches_all_40_upstream_curve_variants() {
    // Golden values from Easings.vb with time=.25, min=0, change=1, duration=1.
    // Columns are EaseIn, EaseOut, EaseInOut, EaseOutIn, respectively.
    let expected = [
        ("quad", [0.0625, 0.4375, 0.125, 0.375]),
        ("cubic", [0.015625, 0.578125, 0.0625, 0.4375]),
        ("quart", [0.00390625, 0.68359375, 0.03125, 0.46875]),
        ("quint", [0.0009765625, 0.7626953125, 0.015625, 0.484375]),
        (
            "sine",
            [
                0.07612046748871326,
                0.3826834323650898,
                0.35355339059327373,
                0.35355339059327373,
            ],
        ),
        (
            "expo",
            [0.005524271728019903, 0.8232233047033631, 0.015625, 0.484375],
        ),
        (
            "circ",
            [
                0.031754163448145745,
                0.6614378277661477,
                0.0669872981077807,
                0.4330127018922193,
            ],
        ),
        (
            "back",
            [-0.0641365625, 0.8174096875, -0.09968184375, 0.54384875],
        ),
        (
            "elastic",
            [
                -0.005524271728019903,
                0.9116116523516815,
                0.011969444423734025,
                0.5078125,
            ],
        ),
        ("bounce", [0.02734375, 0.47265625, 0.1171875, 0.3828125]),
    ];
    for (family, values) in expected {
        for (mode, expected) in MODES.iter().zip(values) {
            let name = format!("{family}ease{mode}");
            close(easing(&name, 0.25).unwrap(), expected);
        }
    }
    assert_eq!(easing("linear", 0.25), Ok(0.25));
    assert_eq!(easing("none", 0.25), Ok(1.0));
}

#[test]
fn none_is_a_normalized_step_but_animation_is_always_instantaneous() {
    assert_eq!(easing("none", 0.0), Ok(0.0));
    assert_eq!(easing("none", f64::from_bits(1)), Ok(1.0));
    for (from, to) in [(0, 100), (100, -100), (i32::MIN, i32::MAX)] {
        for elapsed in [0, 1, 50, u64::MAX] {
            for duration in [0, 1, 100, u32::MAX] {
                assert_eq!(
                    animated_position(from, to, elapsed, duration, "none"),
                    Ok(to)
                );
            }
        }
    }
}

#[test]
fn upstream_sine_inout_quirk_is_preserved() {
    for index in 0..=100 {
        let progress = f64::from(index) / 100.0;
        close(
            easing("sineeaseinout", progress).unwrap(),
            easing("sineeaseoutin", progress).unwrap(),
        );
    }
    // Conventional sine-in-out would be about .146447 here, not .353553.
    close(easing("sineeaseinout", 0.25).unwrap(), 0.35355339059327373);
    close(easing("sineeaseinout", 0.75).unwrap(), 0.6464466094067263);
}

#[test]
fn compound_curves_have_exact_midpoint_and_time_reflection_symmetry() {
    for family in FAMILIES {
        for mode in ["inout", "outin"] {
            let name = format!("{family}ease{mode}");
            assert_eq!(easing(&name, 0.5), Ok(0.5), "{name}");
            for progress in [0.125, 0.25, 0.375, 0.45] {
                close(
                    easing(&name, progress).unwrap(),
                    1.0 - easing(&name, 1.0 - progress).unwrap(),
                );
            }
        }
    }
}

#[test]
fn out_in_composition_uses_out_first_then_in() {
    for family in FAMILIES {
        let combined = format!("{family}easeoutin");
        for t in [0.01, 0.125, 0.25, 0.49] {
            close(
                easing(&combined, t).unwrap(),
                0.5 * easing(&format!("{family}easeout"), t * 2.0).unwrap(),
            );
            close(
                easing(&combined, 0.5 + t).unwrap(),
                0.5 + 0.5 * easing(&format!("{family}easein"), t * 2.0).unwrap(),
            );
        }
    }
}

#[test]
fn back_and_elastic_overshoot_is_not_clamped() {
    assert!(easing("backeasein", 0.25).unwrap() < 0.0);
    assert!(easing("backeaseout", 0.75).unwrap() > 1.0);
    assert!(easing("backeaseinout", 0.25).unwrap() < 0.0);
    assert!(easing("backeaseinout", 0.75).unwrap() > 1.0);
    assert!(easing("backeaseoutin", 0.25).unwrap() > 0.5);
    assert!(easing("backeaseoutin", 0.75).unwrap() < 0.5);
    close(easing("elasticeasein", 0.6).unwrap(), -0.03125);
    close(easing("elasticeaseout", 0.4).unwrap(), 1.03125);
    assert!(easing("elasticeaseinout", 0.4).unwrap() < 0.0);
    assert!(easing("elasticeaseinout", 0.6).unwrap() > 1.0);
    assert!(easing("elasticeaseoutin", 0.2).unwrap() > 0.5);
    assert!(easing("elasticeaseoutin", 0.8).unwrap() < 0.5);
}

#[test]
fn bounce_piecewise_boundaries_and_valleys_match_upstream() {
    for (progress, expected) in [
        (0.0, 0.0),
        (1.0 / 2.75, 1.0),
        (1.5 / 2.75, 0.75),
        (2.0 / 2.75, 1.0),
        (2.25 / 2.75, 0.9375),
        (2.5 / 2.75, 1.0),
        (2.625 / 2.75, 0.984375),
        (1.0, 1.0),
    ] {
        close(easing("bounceeaseout", progress).unwrap(), expected);
        close(
            easing("bounceeasein", 1.0 - progress).unwrap(),
            1.0 - expected,
        );
    }
}

#[test]
fn all_curves_remain_finite_throughout_domain_and_near_endpoints() {
    for &name in EASING_NAMES {
        let extra = [
            f64::from_bits(1),
            f64::MIN_POSITIVE,
            f64::EPSILON,
            1.0 - f64::EPSILON,
        ];
        for progress in (0..=4096).map(|step| f64::from(step) / 4096.0).chain(extra) {
            let value = easing(name, progress).unwrap();
            assert!(value.is_finite(), "{name} at {progress}: {value}");
            if !name.starts_with("back") && !name.starts_with("elastic") {
                assert!(
                    (0.0..=1.0).contains(&value),
                    "{name} at {progress}: {value}"
                );
            }
        }
    }
}

#[test]
fn regular_curves_are_monotonic_except_the_upstream_bounce_and_overshoot_families() {
    for &name in EASING_NAMES {
        if name.starts_with("back") || name.starts_with("elastic") || name.starts_with("bounce") {
            continue;
        }
        let mut previous = 0.0;
        for step in 0..=1024 {
            let value = easing(name, f64::from(step) / 1024.0).unwrap();
            assert!(value >= previous, "{name} regressed at step {step}");
            previous = value;
        }
    }
}

#[test]
fn every_animation_starts_and_finishes_exactly_for_either_direction() {
    for &name in EASING_NAMES {
        for (from, to) in [
            (37, 831),
            (831, 37),
            (-2700, -31),
            (31, -2700),
            (i32::MIN, i32::MAX),
        ] {
            for duration in [1, 200, u32::MAX] {
                let start = if name == "none" { to } else { from };
                assert_eq!(
                    animated_position(from, to, 0, duration, name),
                    Ok(start),
                    "{name}"
                );
                for elapsed in [u64::from(duration), u64::from(duration) + 1, u64::MAX] {
                    assert_eq!(
                        animated_position(from, to, elapsed, duration, name),
                        Ok(to),
                        "{name}"
                    );
                }
            }
        }
    }
}

#[test]
fn zero_duration_is_instant_for_every_selection() {
    for &name in EASING_NAMES {
        for elapsed in [0, 1, 1000, u64::MAX] {
            assert_eq!(animated_position(-20, 700, elapsed, 0, name), Ok(700));
            assert_eq!(animated_position(700, -20, elapsed, 0, name), Ok(-20));
        }
    }
}

#[test]
fn identical_positions_do_not_drift_even_with_overshooting_curves() {
    for &name in EASING_NAMES {
        for position in [i32::MIN, -1, 0, 1, i32::MAX] {
            for elapsed in [0, 1, 25, 50, 75, 100, u64::MAX] {
                assert_eq!(
                    animated_position(position, position, elapsed, 100, name),
                    Ok(position)
                );
            }
        }
    }
}

#[test]
fn linear_interpolation_moves_both_directions_and_across_zero() {
    for (elapsed, forward, backward) in [
        (0, -100, 300),
        (25, 0, 200),
        (50, 100, 100),
        (75, 200, 0),
        (100, 300, -100),
    ] {
        assert_eq!(
            animated_position(-100, 300, elapsed, 100, "linear"),
            Ok(forward)
        );
        assert_eq!(
            animated_position(300, -100, elapsed, 100, "linear"),
            Ok(backward)
        );
    }
}

#[test]
fn cubic_intermediate_and_reverse_positions_match_the_signed_delta() {
    for (elapsed, forward, reverse) in [
        (0, 0, 1024),
        (25, 64, 960),
        (50, 512, 512),
        (75, 960, 64),
        (100, 1024, 0),
    ] {
        assert_eq!(
            animated_position(0, 1024, elapsed, 100, "cubiceaseinout"),
            Ok(forward)
        );
        assert_eq!(
            animated_position(1024, 0, elapsed, 100, "cubiceaseinout"),
            Ok(reverse)
        );
    }
    assert_eq!(animated_position(100, -100, 25, 100, "cubiceasein"), Ok(97));
    assert_eq!(
        animated_position(-100, 100, 25, 100, "cubiceasein"),
        Ok(-97)
    );
}

#[test]
fn integer_positions_round_nearest_with_halfway_away_from_zero() {
    for (from, to, expected) in [(0, 1, 1), (0, -1, -1), (4, 5, 5), (-4, -5, -5)] {
        assert_eq!(animated_position(from, to, 50, 100, "linear"), Ok(expected));
    }
    assert_eq!(animated_position(0, 1, 49, 100, "linear"), Ok(0));
    assert_eq!(animated_position(0, -1, 49, 100, "linear"), Ok(0));
}

#[test]
fn huge_deltas_and_elapsed_times_never_wrap() {
    assert_eq!(
        animated_position(i32::MIN, i32::MAX, 50, 100, "linear"),
        Ok(-1)
    );
    assert_eq!(
        animated_position(i32::MAX, i32::MIN, 50, 100, "linear"),
        Ok(-1)
    );
    assert_eq!(
        animated_position(i32::MIN, i32::MIN + 10, 50, 100, "linear"),
        Ok(i32::MIN + 5)
    );
    assert_eq!(
        animated_position(i32::MAX, i32::MAX - 10, 50, 100, "linear"),
        Ok(i32::MAX - 5)
    );
    // For this delta/duration pair each millisecond is exactly one pixel.
    assert_eq!(
        animated_position(i32::MIN, i32::MAX, 1, u32::MAX, "linear"),
        Ok(i32::MIN + 1)
    );
    assert_eq!(
        animated_position(
            i32::MIN,
            i32::MAX,
            u64::from(u32::MAX) - 1,
            u32::MAX,
            "linear"
        ),
        Ok(i32::MAX - 1)
    );
    for &name in EASING_NAMES {
        for elapsed in [
            1,
            2,
            100,
            u64::from(u32::MAX) / 2,
            u64::from(u32::MAX) - 1,
            u64::MAX,
        ] {
            assert!(animated_position(i32::MIN, i32::MAX, elapsed, u32::MAX, name).is_ok());
            assert!(animated_position(i32::MAX, i32::MIN, elapsed, u32::MAX, name).is_ok());
        }
    }
}

#[test]
fn overshooting_positions_survive_except_at_integer_limits() {
    assert!(animated_position(100, 200, 75, 100, "backeaseout").unwrap() > 200);
    assert!(animated_position(200, 100, 75, 100, "backeaseout").unwrap() < 100);
    assert!(animated_position(100, 200, 25, 100, "backeasein").unwrap() < 100);
    assert!(animated_position(200, 100, 25, 100, "backeasein").unwrap() > 200);
    for (name, elapsed, rising, falling) in [
        ("backeasein", 25, i32::MIN, i32::MAX),
        ("backeaseout", 75, i32::MAX, i32::MIN),
        ("elasticeasein", 60, i32::MIN, i32::MAX),
        ("elasticeaseout", 40, i32::MAX, i32::MIN),
    ] {
        assert_eq!(
            animated_position(i32::MIN, i32::MAX, elapsed, 100, name),
            Ok(rising)
        );
        assert_eq!(
            animated_position(i32::MAX, i32::MIN, elapsed, 100, name),
            Ok(falling)
        );
    }
}

#[test]
fn callers_can_clamp_every_curve_to_geometry_without_overlapping_controls() {
    use taskbarx_core::{center_target, legal_position_range, AxisLayout};

    let layout = AxisLayout {
        bar_start: -1920,
        bar_end: 0,
        free_start: -1800,
        free_end: -200,
        parent_start: -1810,
        content_offset: 10,
        content_len: 400,
    };
    let (min, max) = legal_position_range(&layout).unwrap();
    let to = center_target(&layout, i32::MAX, false).unwrap();
    assert_eq!(to, max);
    for &name in EASING_NAMES {
        for elapsed in 0..=1000 {
            let frame = animated_position(min, to, elapsed, 1000, name)
                .unwrap()
                .clamp(min, max);
            let occupied_start = i64::from(layout.parent_start)
                + i64::from(frame)
                + i64::from(layout.content_offset);
            let occupied_end = occupied_start + i64::from(layout.content_len);
            assert!(occupied_start >= i64::from(layout.free_start));
            assert!(occupied_end <= i64::from(layout.free_end));
        }
    }
}
