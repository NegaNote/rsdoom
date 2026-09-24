#![allow(clippy::unwrap_used)]

use rsdoom::argparse::{CliArgs, Warp};
use rstest::rstest;
use std::fs;
use std::path::PathBuf;

fn temporary_file(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("rsdoom-argparse-{name}-{}", std::process::id()));
    fs::write(&path, []).unwrap();
    path
}

#[test]
fn existing_iwad_path_is_preserved_including_spaces() {
    let path = temporary_file("with spaces.wad");
    let mut args = vec!["-iwad".into(), path.display().to_string()];
    let parsed = CliArgs::parse_args(&mut args).unwrap();
    assert_eq!(parsed.iwad_path, path);
    assert!(args.is_empty());
    fs::remove_file(path).unwrap();
}

#[test]
fn file_collects_existing_paths_in_order() {
    let first = temporary_file("first.wad");
    let second = temporary_file("second.wad");
    let mut args = vec![
        "-file".into(),
        first.display().to_string(),
        second.display().to_string(),
    ];
    let parsed = CliArgs::parse_args(&mut args).unwrap();
    assert_eq!(parsed.pwad_paths, vec![first.clone(), second.clone()]);
    fs::remove_file(first).unwrap();
    fs::remove_file(second).unwrap();
}

#[test]
fn missing_paths_and_flags_have_dedicated_errors() {
    let mut args = vec!["-iwad".into()];
    assert_eq!(
        CliArgs::parse_args(&mut args),
        Err("Missing path after -iwad".to_string())
    );

    let mut args = vec!["-iwad".into(), "-file".into()];
    assert_eq!(
        CliArgs::parse_args(&mut args),
        Err("IWAD file not found: -file".to_string())
    );
}

#[rstest]
#[case("1", 1)]
#[case("5", 5)]
fn accepts_valid_skill_boundaries(#[case] value: &str, #[case] expected: u8) {
    let mut args = vec!["-skill".into(), value.into()];
    assert_eq!(
        CliArgs::parse_args(&mut args).unwrap().skill,
        Some(expected)
    );
}

#[rstest]
#[case("6")]
#[case("255")]
#[case("-1")]
#[case("0")]
#[case("256")]
fn rejects_invalid_skill_boundaries(#[case] value: &str) {
    let mut args = vec!["-skill".into(), value.into()];
    assert!(CliArgs::parse_args(&mut args).is_err());
}

#[rstest]
#[case(&["1"], Warp::Map(1))]
#[case(&["65535"], Warp::Map(u16::MAX))]
#[case(&["1", "1"], Warp::Episode(1, 1))]
#[case(&["65535", "65535"], Warp::Episode(u16::MAX, u16::MAX))]
fn accepts_warp_boundaries(#[case] values: &[&str], #[case] expected: Warp) {
    let mut args = vec!["-warp".into()];
    args.extend(values.iter().map(ToString::to_string));
    assert_eq!(CliArgs::parse_args(&mut args).unwrap().warp, Some(expected));
}

#[test]
fn warp_rejects_extra_values() {
    let mut args = vec!["-warp".into(), "1".into(), "2".into(), "3".into()];
    assert!(CliArgs::parse_args(&mut args).is_err());
}

#[test]
fn help_takes_precedence_without_mutating_arguments() {
    let mut args = vec!["-skill".into(), "invalid".into(), "--help".into()];
    let original = args.clone();
    assert!(CliArgs::parse_args(&mut args).is_err());
    assert_eq!(args, original);
}
