mod support;

use std::collections::BTreeSet;
use std::fs;
use std::process::Command;
use std::sync::{Arc, Barrier};

use support::TestDirectory;
use taskbarx_config::{
    default_path, from_json, load, save_atomic, to_json, Settings, EASING_NAMES, SUPPORTED_STYLES,
};

fn full_settings() -> Settings {
    Settings {
        schema_version: 1,
        center: false,
        primary: false,
        secondary: false,
        center_between: true,
        primary_offset: -123,
        secondary_offset: 456,
        animation: "elasticeaseoutin".into(),
        animation_ms: 876,
        poll_ms: 250,
        style: "acrylic".into(),
        color: [12, 34, 56, 255],
    }
}

#[test]
fn fixed_defaults_and_json_keys() {
    let expected = serde_json::json!({
        "schema_version": 1,
        "center": true,
        "primary": true,
        "secondary": true,
        "center_between": false,
        "primary_offset": 0,
        "secondary_offset": 0,
        "animation": "cubiceaseinout",
        "animation_ms": 300,
        "poll_ms": 400,
        "style": "default",
        "color": [0, 0, 0, 128]
    });
    let defaults = Settings::default();
    assert_eq!(serde_json::to_value(&defaults).unwrap(), expected);
    assert_eq!(from_json("{}").unwrap(), defaults);
    assert_eq!(from_json(&to_json(&defaults).unwrap()).unwrap(), defaults);
    defaults.validate().unwrap();
    // Exercise the required public derives as well as the validated helpers.
    assert_eq!(defaults.clone(), defaults);
    assert!(format!("{defaults:?}").contains("Settings"));
    assert_eq!(
        serde_json::from_value::<Settings>(expected).unwrap(),
        defaults
    );
}

#[test]
fn full_round_trip_and_individually_omitted_fields() {
    let settings = full_settings();
    let text = to_json(&settings).unwrap();
    assert!(text.ends_with('\n'));
    assert_eq!(from_json(&text).unwrap(), settings);
    let full = serde_json::to_value(&settings).unwrap();
    let defaults = serde_json::to_value(Settings::default()).unwrap();
    for key in full.as_object().unwrap().keys() {
        let mut omitted = full.clone();
        omitted.as_object_mut().unwrap().remove(key);
        let parsed = from_json(&omitted.to_string()).unwrap();
        let mut expected = full.clone();
        expected[key] = defaults[key].clone();
        assert_eq!(serde_json::to_value(parsed).unwrap(), expected, "{key}");
    }
}

#[test]
fn every_supported_easing_and_style_is_exact_and_round_trips() {
    let mut expected = BTreeSet::from(["none".to_owned(), "linear".to_owned()]);
    for family in [
        "quad", "cubic", "quart", "quint", "sine", "expo", "circ", "back", "elastic", "bounce",
    ] {
        for direction in ["easein", "easeout", "easeinout", "easeoutin"] {
            expected.insert(format!("{family}{direction}"));
        }
    }
    assert_eq!(EASING_NAMES.len(), 42);
    assert_eq!(
        EASING_NAMES
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<BTreeSet<_>>(),
        expected
    );
    assert_eq!(
        SUPPORTED_STYLES,
        ["default", "transparent", "blur", "acrylic", "opaque"]
    );
    for animation in EASING_NAMES {
        for style in SUPPORTED_STYLES {
            let settings = Settings {
                animation: (*animation).into(),
                style: (*style).into(),
                ..Settings::default()
            };
            settings.validate().unwrap();
            assert_eq!(from_json(&to_json(&settings).unwrap()).unwrap(), settings);
        }
    }
}

#[test]
fn selectors_remain_independent_of_centering_and_styling() {
    for flags in 0..16 {
        let settings = Settings {
            center: flags & 1 != 0,
            primary: flags & 2 != 0,
            secondary: flags & 4 != 0,
            center_between: flags & 8 != 0,
            style: "opaque".into(),
            ..Settings::default()
        };
        assert_eq!(from_json(&to_json(&settings).unwrap()).unwrap(), settings);
    }
}

#[test]
fn numeric_boundaries_and_zero_duration_are_preserved() {
    for offset in [-10_000, 0, 10_000] {
        for animation_ms in [0, 10_000] {
            for poll_ms in [50, 10_000] {
                let settings = Settings {
                    primary_offset: offset,
                    secondary_offset: -offset,
                    animation_ms,
                    poll_ms,
                    color: [0, 255, 0, 255],
                    ..Settings::default()
                };
                assert_eq!(from_json(&to_json(&settings).unwrap()).unwrap(), settings);
            }
        }
    }
}

fn invalid_settings() -> Vec<Settings> {
    let mut values = Vec::new();
    for schema_version in [0, 2, u32::MAX] {
        values.push(Settings {
            schema_version,
            ..Settings::default()
        });
    }
    for offset in [-10_001, 10_001, i32::MIN, i32::MAX] {
        values.push(Settings {
            primary_offset: offset,
            ..Settings::default()
        });
        values.push(Settings {
            secondary_offset: offset,
            ..Settings::default()
        });
    }
    for animation_ms in [10_001, u32::MAX] {
        values.push(Settings {
            animation_ms,
            ..Settings::default()
        });
    }
    for poll_ms in [0, 49, 10_001, u32::MAX] {
        values.push(Settings {
            poll_ms,
            ..Settings::default()
        });
    }
    for animation in ["", "CubicEaseInOut", "linear ", " none", "invalid"] {
        values.push(Settings {
            animation: animation.into(),
            ..Settings::default()
        });
    }
    for style in ["", "Transparent", "gradient", "default ", "5"] {
        values.push(Settings {
            style: style.into(),
            ..Settings::default()
        });
    }
    values
}

#[test]
fn semantic_validation_applies_to_parse_serialize_load_and_save() {
    let directory = TestDirectory::new();
    let destination = directory.path().join("settings.json");
    let invalid_input = directory.path().join("invalid.json");
    save_atomic(&destination, &full_settings()).unwrap();
    let previous = fs::read(&destination).unwrap();
    for settings in invalid_settings() {
        assert!(settings.validate().is_err(), "{settings:?}");
        assert!(to_json(&settings).is_err(), "{settings:?}");
        let text = serde_json::to_string(&settings).unwrap();
        assert!(from_json(&text).is_err(), "{settings:?}");
        fs::write(&invalid_input, text).unwrap();
        assert!(load(&invalid_input).is_err(), "{settings:?}");
        assert!(save_atomic(&destination, &settings).is_err());
        assert_eq!(fs::read(&destination).unwrap(), previous);
    }
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    let missing = directory.path().join("not-created").join("settings.json");
    assert!(save_atomic(&missing, &invalid_settings()[0]).is_err());
    assert!(!missing.parent().unwrap().exists());
}

#[test]
fn unknown_fields_duplicate_fields_and_bad_types_are_rejected() {
    for input in [
        r#"{"unknown":true}"#,
        r#"{"schemaVersion":1}"#,
        r#"{"center":true,"center":false}"#,
        r#"{"schema_version":1,"schema_version":1}"#,
        r#"{"schema_version":-1}"#,
        r#"{"schema_version":1.0}"#,
        r#"{"schema_version":4294967296}"#,
        r#"{"primary_offset":2147483648}"#,
        r#"{"secondary_offset":-2147483649}"#,
        r#"{"primary_offset":0.5}"#,
        r#"{"animation_ms":-1}"#,
        r#"{"poll_ms":"400"}"#,
        r#"{"center":1}"#,
        r#"{"primary":"true"}"#,
        r#"{"secondary":null}"#,
        r#"{"center_between":[]}"#,
        r#"{"animation":null}"#,
        r#"{"style":5}"#,
        "",
        "null",
        "true",
        "[]",
        "42",
        "\"settings\"",
        "{} {}",
        "{",
        "{\"center\":true,}",
        r#"[1,true,true,true,false,0,0,"linear",300,400,"default",[0,0,0,128]]"#,
    ] {
        assert!(from_json(input).is_err(), "accepted {input}");
    }
}

#[test]
fn malformed_rgba_arrays_and_components_are_rejected() {
    for color in [
        "[]",
        "[0,0,0]",
        "[0,0,0,0,0]",
        "null",
        "{}",
        "\"#000000\"",
        "[-1,0,0,0]",
        "[0,-1,0,0]",
        "[0,0,-1,0]",
        "[0,0,0,-1]",
        "[256,0,0,0]",
        "[0,256,0,0]",
        "[0,0,256,0]",
        "[0,0,0,256]",
        "[0,0,0,0.5]",
        "[0,0,0,1.0]",
        "[0,0,0,null]",
        "[0,0,0,true]",
        "[0,0,0,\"128\"]",
    ] {
        let text = format!("{{\"color\":{color}}}");
        assert!(from_json(&text).is_err(), "accepted {text}");
    }
    for color in [[0; 4], [255; 4], [0, 1, 254, 255]] {
        let settings = Settings {
            color,
            ..Settings::default()
        };
        assert_eq!(
            from_json(&to_json(&settings).unwrap()).unwrap().color,
            color
        );
    }
}

#[test]
fn size_limit_is_bytes_accepts_exactly_64_kib_and_bounds_file_reads() {
    let directory = TestDirectory::new();
    let path = directory.path().join("settings.json");
    let exact = format!("{}{{}}", " ".repeat(65536 - 2));
    assert_eq!(from_json(&exact).unwrap(), Settings::default());
    fs::write(&path, &exact).unwrap();
    assert_eq!(load(&path).unwrap(), Settings::default());
    let oversized = format!("{exact} ");
    assert!(from_json(&oversized).unwrap_err().contains("64 KiB"));
    fs::write(&path, &oversized).unwrap();
    assert!(load(&path).unwrap_err().contains("64 KiB"));
    assert_eq!(fs::read_to_string(&path).unwrap(), oversized);
    let unicode = format!("{{\"animation\":\"{}\"}}", "é".repeat(32768));
    assert!(unicode.chars().count() < 65536);
    assert!(from_json(&unicode).unwrap_err().contains("64 KiB"));
    fs::write(&path, vec![b' '; 1024 * 1024]).unwrap();
    assert!(load(&path).unwrap_err().contains("64 KiB"));
}

#[test]
fn missing_file_does_not_write_but_corrupt_existing_files_are_errors() {
    let directory = TestDirectory::new();
    let missing = directory.path().join("absent").join("settings.json");
    assert_eq!(load(&missing).unwrap(), Settings::default());
    assert!(!missing.parent().unwrap().exists());
    let path = directory.path().join("settings.json");
    assert_eq!(load(&path).unwrap(), Settings::default());
    assert!(!path.exists());
    for bytes in [
        b"".as_slice(),
        b"{broken}",
        b"\xff\xfe",
        b"{\"color\":[0]}",
        b"{\"unknown\":true}",
        b"{\"schema_version\":2}",
    ] {
        fs::write(&path, bytes).unwrap();
        let error = load(&path).unwrap_err();
        assert!(error.contains("settings.json"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    assert!(load(directory.path()).is_err());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn save_creates_parents_replaces_existing_settings_and_leaves_no_temp_files() {
    let directory = TestDirectory::new();
    let path = directory
        .path()
        .join("new")
        .join("nested")
        .join("配置-🦀.json");
    save_atomic(&path, &Settings::default()).unwrap();
    assert_eq!(load(&path).unwrap(), Settings::default());
    save_atomic(&path, &full_settings()).unwrap();
    assert_eq!(load(&path).unwrap(), full_settings());
    // Replacement also permits a shorter document without trailing old bytes.
    save_atomic(&path, &Settings::default()).unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        to_json(&Settings::default()).unwrap()
    );
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn replacement_and_parent_creation_failures_are_non_destructive() {
    let directory = TestDirectory::new();
    let destination = directory.path().join("settings.json");
    fs::create_dir(&destination).unwrap();
    let contents = destination.join("keep.txt");
    fs::write(&contents, b"untouched directory").unwrap();
    let unrelated = directory.path().join(".taskbarx-settings-unrelated.tmp");
    fs::write(&unrelated, b"another operation").unwrap();
    assert!(save_atomic(&destination, &Settings::default())
        .unwrap_err()
        .contains("replace"));
    assert_eq!(fs::read(&contents).unwrap(), b"untouched directory");
    assert_eq!(fs::read(&unrelated).unwrap(), b"another operation");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    assert!(save_atomic(&unrelated.join("settings.json"), &Settings::default()).is_err());
    assert_eq!(fs::read(&unrelated).unwrap(), b"another operation");
    assert!(save_atomic(std::path::Path::new(""), &Settings::default()).is_err());
}

#[cfg(windows)]
#[test]
fn windows_sharing_violation_keeps_previous_valid_settings_and_cleans_temp() {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    let directory = TestDirectory::new();
    let path = directory.path().join("settings.json");
    save_atomic(&path, &Settings::default()).unwrap();
    let previous = fs::read(&path).unwrap();
    // Permit reads but not deletion/renaming: an actual Windows replacement
    // failure after the new temporary file has been written and synced.
    let locked = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    let error = save_atomic(&path, &full_settings()).unwrap_err();
    assert!(error.contains("replace"), "{error}");
    assert_eq!(fs::read(&path).unwrap(), previous);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    drop(locked);
    assert_eq!(load(&path).unwrap(), Settings::default());
    save_atomic(&path, &full_settings()).unwrap();
    assert_eq!(load(&path).unwrap(), full_settings());
}

#[cfg(windows)]
#[test]
fn windows_unreadable_existing_settings_are_an_error_not_defaults() {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    let directory = TestDirectory::new();
    let path = directory.path().join("settings.json");
    save_atomic(&path, &full_settings()).unwrap();
    let previous = fs::read(&path).unwrap();
    let locked = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&path)
        .unwrap();
    assert!(load(&path).unwrap_err().contains("could not open"));
    drop(locked);
    assert_eq!(fs::read(&path).unwrap(), previous);
    assert_eq!(load(&path).unwrap(), full_settings());
}

#[cfg(windows)]
#[test]
fn windows_readonly_destination_is_not_deleted_on_replacement_failure() {
    let directory = TestDirectory::new();
    let path = directory.path().join("settings.json");
    save_atomic(&path, &Settings::default()).unwrap();
    let previous = fs::read(&path).unwrap();
    let original = fs::metadata(&path).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&path, readonly).unwrap();
    let result = save_atomic(&path, &full_settings());
    // Restore permissions even if an assertion fails, so cleanup is possible.
    fs::set_permissions(&path, original).unwrap();
    assert!(result.is_err());
    assert_eq!(fs::read(&path).unwrap(), previous);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn dangling_symlink_is_not_a_missing_configuration() {
    let directory = TestDirectory::new();
    let path = directory.path().join("settings.json");
    std::os::unix::fs::symlink(directory.path().join("absent.json"), &path).unwrap();
    assert!(load(&path).is_err());
    assert!(fs::symlink_metadata(&path)
        .unwrap()
        .file_type()
        .is_symlink());
}

#[test]
fn concurrent_saves_and_loads_never_observe_partial_or_missing_settings() {
    let directory = TestDirectory::new();
    let path = directory.path().join("settings.json");
    let initial = Settings {
        primary_offset: -1,
        ..full_settings()
    };
    save_atomic(&path, &initial).unwrap();
    let barrier = Arc::new(Barrier::new(5));
    std::thread::scope(|scope| {
        for writer in 0..4 {
            let path = &path;
            let barrier = Arc::clone(&barrier);
            scope.spawn(move || {
                barrier.wait();
                for iteration in 0..20 {
                    let offset = writer * 100 + iteration;
                    let settings = Settings {
                        primary_offset: offset,
                        secondary_offset: -offset,
                        ..full_settings()
                    };
                    save_atomic(path, &settings).unwrap();
                }
            });
        }
        barrier.wait();
        for _ in 0..200 {
            let settings = load(&path).unwrap();
            assert_eq!(settings.style, "acrylic");
            assert_eq!(settings.animation, "elasticeaseoutin");
            assert_eq!(settings.color, full_settings().color);
            assert!(!settings.center); // Missing-file defaults must not appear.
            if settings.primary_offset != -1 {
                assert_eq!(settings.secondary_offset, -settings.primary_offset);
            }
        }
    });
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    load(&path).unwrap().validate().unwrap();
}

#[test]
fn default_path_is_side_effect_free_and_appdata_errors_are_actionable() {
    // Run environment-sensitive assertions in child processes rather than
    // mutating this test runner's environment or reading real APPDATA settings.
    const CASE: &str = "TASKBARX_CONFIG_TEST_APPDATA_CASE";
    if let Ok(case) = std::env::var(CASE) {
        if case == "valid" {
            let appdata = std::path::PathBuf::from(std::env::var_os("APPDATA").unwrap());
            assert!(!appdata.exists());
            assert_eq!(
                default_path().unwrap(),
                appdata.join("TaskbarX-Rust").join("settings.json")
            );
            assert!(!appdata.exists());
        } else {
            let error = default_path().unwrap_err();
            assert!(error.contains("APPDATA"));
            assert!(error.contains("explicit settings path"));
        }
        return;
    }
    let directory = TestDirectory::new();
    for case in ["valid", "missing", "empty", "relative"] {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "default_path_is_side_effect_free_and_appdata_errors_are_actionable",
                "--nocapture",
            ])
            .env(CASE, case);
        match case {
            "valid" => {
                command.env("APPDATA", directory.path().join("roaming"));
            }
            "missing" => {
                command.env_remove("APPDATA");
            }
            "empty" => {
                command.env("APPDATA", "");
            }
            "relative" => {
                command.env("APPDATA", "relative-roaming");
            }
            _ => unreachable!(),
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{case}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}
