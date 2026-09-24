#![allow(clippy::unwrap_used)]

use super::*;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("test_wads")
        .join(name)
}

fn load_fixture(name: &str) -> Result<WadView, WadLoadingError> {
    load_wad(&fixture(name), WadType::Iwad)
}

fn load_fixture_for_test(name: &str) -> Option<WadView> {
    let result = load_fixture(name);
    assert!(result.is_ok(), "failed to load {name}: {result:?}");
    result.ok()
}

fn assert_lump_data(wad: &WadView, name: LumpName, expected: &[&[u8]]) {
    let lumps = wad
        .get_lumps_by_name(name)
        .iter()
        .map(|lump| lump.get_raw_data())
        .collect::<Vec<_>>();

    assert_eq!(lumps, expected);
}

mod wad_type {
    use super::*;
    use rstest::rstest;

    #[test]
    fn accepts_iwad() {
        let mut input = b"IWAD".as_ref();
        assert_eq!(is_valid_wad_type(&mut input), Ok(()));
        assert!(input.is_empty());
    }

    #[test]
    fn accepts_pwad() {
        let mut input = b"PWAD".as_ref();
        assert_eq!(is_valid_wad_type(&mut input), Ok(()));
        assert!(input.is_empty());
    }

    #[test]
    fn rejects_invalid_type() {
        let mut input = b"INVALID".as_ref();
        assert!(is_valid_wad_type(&mut input).is_err());
    }

    #[test]
    fn consumes_only_the_type_prefix() {
        let mut input = b"IWADtrailing".as_ref();
        assert_eq!(is_valid_wad_type(&mut input), Ok(()));
        assert_eq!(input, b"trailing");
    }

    #[rstest]
    #[case(0)]
    #[case(1)]
    #[case(2)]
    #[case(3)]
    fn rejects_each_truncated_type(#[case] length: usize) {
        let mut input = b"IWAD".get(..length).unwrap();
        assert!(is_valid_wad_type(&mut input).is_err());
    }
}

mod header {
    use super::*;
    use rstest::rstest;

    #[test]
    fn reads_lump_count_and_directory_offset() {
        let mut input = b"\x02\x00\x00\x00\x10\x00\x00\x00".as_ref();
        assert_eq!(
            get_num_lumps_and_info_table_offset(&mut input),
            Ok(HeaderInfo {
                num_lumps: 2,
                info_table_offset: 16
            })
        );
        assert!(input.is_empty());
    }

    #[test]
    fn reads_iwad_header() {
        let mut input = b"IWAD\x09\x00\x00\x00\x13\x00\x00\x00".as_ref();
        assert_eq!(
            get_header_info(&mut input),
            Ok(HeaderInfo {
                num_lumps: 9,
                info_table_offset: 19
            })
        );
        assert!(input.is_empty());
    }

    #[test]
    fn reads_pwad_header() {
        let mut input = b"PWAD\x05\x01\x00\x00\xE5\x02\x00\x00".as_ref();
        assert_eq!(
            get_header_info(&mut input),
            Ok(HeaderInfo {
                num_lumps: 261,
                info_table_offset: 741
            })
        );
        assert!(input.is_empty());
    }

    #[test]
    fn rejects_invalid_header() {
        let mut input = b"INVALID\x00\x00\x00\x00\x00".as_ref();
        assert!(get_header_info(&mut input).is_err());
    }

    #[test]
    fn rejects_empty_header() {
        let mut input = b"".as_ref();
        assert!(get_header_info(&mut input).is_err());
    }

    #[rstest]
    #[case(0)]
    #[case(1)]
    #[case(2)]
    #[case(3)]
    #[case(4)]
    #[case(5)]
    #[case(6)]
    #[case(7)]
    #[case(8)]
    #[case(9)]
    #[case(10)]
    #[case(11)]
    fn rejects_every_truncated_header_length(#[case] length: usize) {
        let bytes = vec![0; length];
        let mut input = bytes.as_slice();
        assert!(get_header_info(&mut input).is_err());
    }

    #[test]
    fn reads_maximum_header_values() {
        let mut input = b"PWAD\xff\xff\xff\xff\xff\xff\xff\xff".as_ref();
        assert_eq!(
            get_header_info(&mut input),
            Ok(HeaderInfo {
                num_lumps: u32::MAX,
                info_table_offset: u32::MAX
            })
        );
    }

    #[test]
    fn leaves_trailing_header_bytes_unconsumed() {
        let mut input = b"\x02\0\0\0\x10\0\0\0extra".as_ref();
        assert!(get_num_lumps_and_info_table_offset(&mut input).is_ok());
        assert_eq!(input, b"extra");
    }

    #[rstest]
    #[case(0)]
    #[case(1)]
    #[case(2)]
    #[case(3)]
    #[case(4)]
    #[case(5)]
    #[case(6)]
    #[case(7)]
    fn rejects_truncated_header_values(#[case] length: usize) {
        let bytes = vec![0; length];
        let mut input = bytes.as_slice();
        assert!(get_num_lumps_and_info_table_offset(&mut input).is_err());
    }
}

mod lump_info {
    use super::*;
    use rstest::rstest;

    #[test]
    fn reads_ascii_graphic_name() {
        let mut input = b"\x00\x00\x00\x00\x05\x00\x00\x00LUMPNAME".as_ref();
        assert_eq!(
            get_lump_info(&mut input),
            Ok(LumpInfo {
                offset: 0,
                size: 5,
                name: LumpName(*b"LUMPNAME")
            })
        );
        assert!(input.is_empty());
    }

    #[test]
    fn reads_null_padded_name() {
        let mut input = b"\x00\x00\x00\x00\x05\x00\x00\x00LUMP\0\0\0\0".as_ref();
        assert_eq!(
            get_lump_info(&mut input),
            Ok(LumpInfo {
                offset: 0,
                size: 5,
                name: LumpName(*b"LUMP\0\0\0\0")
            })
        );
        assert!(input.is_empty());
    }

    #[test]
    fn empty_name_rejected() {
        let mut input = b"\x00\x00\x00\x00\x05\x00\x00\x00\0\0\0\0\0\0\0\0".as_ref();
        assert!(get_lump_info(&mut input).is_err());
    }

    #[test]
    fn rejects_malformed_null_padding() {
        let mut input = b"\x00\x00\x00\x00\x05\x00\x00\x00LUMP\0\0\0X".as_ref();
        assert!(get_lump_info(&mut input).is_err());
    }

    #[test]
    fn accepts_ascii_control_name() {
        let mut input = b"\x00\x00\x00\x00\x05\x00\x00\x00LUMP\x01\0\0\0".as_ref();
        assert_eq!(
            get_lump_info(&mut input),
            Ok(LumpInfo {
                offset: 0,
                size: 5,
                name: LumpName(*b"LUMP\x01\0\0\0")
            })
        );
    }

    #[rstest]
    #[case(0)]
    #[case(1)]
    #[case(2)]
    #[case(3)]
    #[case(4)]
    #[case(5)]
    #[case(6)]
    #[case(7)]
    #[case(8)]
    #[case(9)]
    #[case(10)]
    #[case(11)]
    #[case(12)]
    #[case(13)]
    #[case(14)]
    #[case(15)]
    fn rejects_every_truncated_lump_info_length(#[case] length: usize) {
        let bytes = vec![0; length];
        let mut input = bytes.as_slice();
        assert!(get_lump_info(&mut input).is_err());
    }

    #[test]
    fn reads_maximum_offset_and_size() {
        let mut input = b"\xff\xff\xff\xff\xff\xff\xff\xffLUMPNAME".as_ref();
        assert_eq!(
            get_lump_info(&mut input),
            Ok(LumpInfo {
                offset: u32::MAX,
                size: u32::MAX,
                name: LumpName(*b"LUMPNAME")
            })
        );
    }

    #[test]
    fn consumes_only_one_lump_info_record() {
        let mut input = b"\0\0\0\0\0\0\0\0NAME\0\0\0\0tail".as_ref();
        assert!(get_lump_info(&mut input).is_ok());
        assert_eq!(input, b"tail");
    }
}

mod lump_name {
    use super::*;
    use rstest::rstest;

    #[test]
    fn converts_padded_name_to_string() {
        let name = LumpName::try_from(*b"PLAYPAL\0");
        assert_eq!(name, Ok(LumpName(*b"PLAYPAL\0")));
        let name = name.unwrap();
        assert_eq!(name.as_bytes(), b"PLAYPAL\0");
        assert_eq!(name.as_str(), "PLAYPAL");
        assert_eq!(name.to_string(), "PLAYPAL");
    }

    #[test]
    fn converts_eight_character_name_to_string() {
        let name = LumpName::try_from(*b"12345678");
        assert_eq!(name, Ok(LumpName(*b"12345678")));
        let name = name.unwrap();
        assert_eq!(name.as_str(), "12345678");
    }

    #[test]
    fn accepts_empty_string_only_if_empty_names_are_valid() {
        assert!(LumpName::from_str("").is_err());
    }

    #[test]
    fn rejects_names_longer_than_eight_characters() {
        assert!(LumpName::from_str("123456789").is_err());
    }

    #[test]
    fn rejects_non_ascii_names() {
        assert!(LumpName::from_str("NÄME").is_err());
    }

    #[test]
    fn accepts_ascii_bytes_before_padding_and_rejects_nonzero_after() {
        assert_eq!(
            LumpName::try_from(*b"BAD\x01\0\0\0\0"),
            Ok(LumpName(*b"BAD\x01\0\0\0\0"))
        );
        assert!(LumpName::try_from(*b"BAD\0GOOD").is_err());
    }

    #[test]
    fn rejects_zero_leading_and_non_ascii_bytes() {
        assert!(LumpName::try_from([0; 8]).is_err());
        assert!(LumpName::try_from(*b"A\0\0\0\0\0\0\xff").is_err());
        assert!(LumpName::try_from(*b"A\xff\0\0\0\0\0\0").is_err());
    }

    #[rstest]
    #[case(1)]
    #[case(2)]
    #[case(3)]
    #[case(4)]
    #[case(5)]
    #[case(6)]
    #[case(7)]
    fn accepts_null_at_each_nonleading_position(#[case] position: usize) {
        let mut bytes = [b'A'; 8];
        bytes.get_mut(position..).unwrap().fill(0);
        assert!(LumpName::try_from(bytes).is_ok());
    }

    #[rstest]
    #[case("A")]
    #[case("ABCDEFG")]
    #[case("ABCDEFGH")]
    fn from_str_accepts_boundary_lengths(#[case] value: &str) {
        assert!(LumpName::from_str(value).is_ok());
    }

    #[rstest]
    #[case("Ä")]
    #[case("A\0B")]
    fn from_str_rejects_non_ascii_and_embedded_nulls(#[case] value: &str) {
        assert!(LumpName::from_str(value).is_err());
    }

    #[test]
    fn displays_shortest_valid_name() {
        let name = LumpName::from_str("A").unwrap();
        assert_eq!(name.as_str(), "A");
        assert_eq!(name.to_string(), "A");
    }
}

mod map_helpers {
    use super::*;
    use rstest::rstest;

    fn name(value: &str) -> LumpName {
        LumpName::from_str(value).unwrap()
    }

    mod namespaces_and_boundaries {
        use super::*;
        use crate::wad::builder::WadBuilder;
        use std::time::{SystemTime, UNIX_EPOCH};

        fn load_builder(wad_type: WadType, names: &[&str]) -> WadView {
            let mut builder = WadBuilder::new(wad_type);
            for name in names {
                builder.add_empty_lump(name).unwrap();
            }
            let path = std::env::temp_dir().join(format!(
                "rsdoom-raw-test-{}-{}.wad",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            builder.write_to_file(&path).unwrap();
            let loaded = load_wad(&path, wad_type).unwrap();
            std::fs::remove_file(path).unwrap();
            loaded
        }

        #[test]
        fn assigns_each_namespace_only_between_markers() {
            let wad = load_builder(
                WadType::Pwad,
                &[
                    "S_START", "SPRITE", "S_END", "F_START", "FLAT", "F_END", "C_START", "COLOR",
                    "C_END", "B_START", "BLOCK", "B_END", "HI_START", "HIRES", "HI_END", "AFTER",
                ],
            );
            assert!(
                wad.get_lump_by_str_name_and_namespace("SPRITE", Namespace::Sprites)
                    .is_some()
            );
            assert!(
                wad.get_lump_by_str_name_and_namespace("FLAT", Namespace::Flats)
                    .is_some()
            );
            assert!(
                wad.get_lump_by_str_name_and_namespace("COLOR", Namespace::Colormaps)
                    .is_some()
            );
            assert!(
                wad.get_lump_by_str_name_and_namespace("BLOCK", Namespace::PrBoom)
                    .is_some()
            );
            assert!(
                wad.get_lump_by_str_name_and_namespace("HIRES", Namespace::HiRes)
                    .is_some()
            );
            assert!(
                wad.get_lump_by_str_name_and_namespace("AFTER", Namespace::Global)
                    .is_some()
            );
            assert!(
                wad.get_lump_by_str_name_and_namespace("S_START", Namespace::Global)
                    .is_some()
            );
        }

        #[test]
        fn preserves_requested_source_type() {
            let wad = load_builder(WadType::Pwad, &["DATA"]);
            assert_eq!(
                wad.lumps.first().map(|lump| lump.source_type),
                Some(WadType::Pwad)
            );
        }

        #[test]
        fn get_lump_lookup_handles_duplicates_and_invalid_inputs() {
            let wad = load_builder(WadType::Iwad, &["DUP", "DUP", "OTHER"]);
            assert_eq!(
                wad.get_lumps_by_name(LumpName::from_str("DUP").unwrap())
                    .len(),
                2
            );
            assert_eq!(wad.get_lump_index_by_str_name("DUP"), Some(0));
            assert!(wad.get_lump_by_str_name("TOO-LONG!").is_none());
            assert!(wad.get_lump_at(usize::MAX).is_none());
        }

        #[test]
        fn empty_wad_can_have_directory_at_eof() {
            let wad = load_builder(WadType::Iwad, &[]);
            assert!(wad.lumps.is_empty());
        }
    }

    #[rstest]
    #[case("E1M1")]
    #[case("MAP01")]
    #[case("MAP123")]
    fn classifies_supported_map_markers(#[case] marker: &str) {
        assert!(parse_map_marker(marker).is_ok());
        assert!(is_map_marker(name(marker)));
    }

    #[rstest]
    #[case("E1")]
    #[case("E1M")]
    #[case("MAP")]
    #[case("MAP01X")]
    #[case("MAP-1")]
    #[case("e1m1")]
    #[case("E1M1X")]
    fn rejects_malformed_map_markers(#[case] marker: &str) {
        assert!(parse_map_marker(marker).is_err());
    }

    #[rstest]
    #[case("THINGS")]
    #[case("LINEDEFS")]
    #[case("SIDEDEFS")]
    #[case("VERTEXES")]
    #[case("SEGS")]
    #[case("SSECTORS")]
    #[case("NODES")]
    #[case("SECTORS")]
    #[case("REJECT")]
    #[case("BLOCKMAP")]
    #[case("BEHAVIOR")]
    #[case("TEXTMAP")]
    fn recognizes_map_lumps(#[case] lump: &str) {
        assert!(is_map_lump(name(lump)));
    }

    #[rstest]
    #[case("THINGSX")]
    #[case("MAP01")]
    fn rejects_non_map_lumps(#[case] lump: &str) {
        assert!(!is_map_lump(name(lump)));
    }
}

mod wad_view {
    use super::*;

    fn test_wad() -> WadView {
        WadView {
            lumps: vec![
                Lump {
                    name: LumpName(*b"START\0\0\0"),
                    raw_data: b"start".to_vec(),
                    source_type: WadType::Iwad,
                    namespace: Namespace::Global,
                },
                Lump {
                    name: LumpName(*b"FIRST\0\0\0"),
                    raw_data: b"first".to_vec(),
                    source_type: WadType::Iwad,
                    namespace: Namespace::Global,
                },
                Lump {
                    name: LumpName(*b"END\0\0\0\0\0"),
                    raw_data: b"end".to_vec(),
                    source_type: WadType::Iwad,
                    namespace: Namespace::Global,
                },
                Lump {
                    name: LumpName(*b"SECOND\0\0"),
                    raw_data: b"second".to_vec(),
                    source_type: WadType::Iwad,
                    namespace: Namespace::Global,
                },
            ],
        }
    }

    #[test]
    fn finds_lumps_by_binary_and_string_name() {
        let wad = test_wad();
        let name = LumpName(*b"FIRST\0\0\0");

        assert_eq!(
            wad.get_lump_by_name(name).map(Lump::get_raw_data),
            Some(b"first".as_slice())
        );
        assert_eq!(
            wad.get_lump_by_str_name("FIRST").map(Lump::get_raw_data),
            Some(b"first".as_slice())
        );
        assert!(wad.get_lump_by_name(LumpName(*b"MISSING\0")).is_none());
        assert!(wad.get_lump_by_str_name("TOO-LONG!").is_none());
    }

    #[test]
    fn gets_lumps_by_index_with_bounds_checks() {
        let wad = test_wad();

        assert_eq!(
            wad.get_lump_at(0).map(Lump::get_raw_data),
            Some(b"start".as_slice())
        );
        assert_eq!(
            wad.get_lump_at(3).map(Lump::get_raw_data),
            Some(b"second".as_slice())
        );
        assert!(wad.get_lump_at(4).is_none());
    }

    #[test]
    fn gets_lumps_between_markers() {
        let wad = test_wad();
        let names = wad
            .get_lumps_between(LumpName(*b"START\0\0\0"), LumpName(*b"END\0\0\0\0\0"))
            .map(|lump| lump.name)
            .collect::<Vec<_>>();

        assert_eq!(names, vec![LumpName(*b"FIRST\0\0\0")]);
    }

    #[test]
    fn missing_marker_produces_empty_range() {
        let wad = test_wad();
        assert_eq!(
            wad.get_lumps_between(LumpName(*b"MISSING\0"), LumpName(*b"END\0\0\0\0\0"))
                .count(),
            0
        );
    }

    #[test]
    fn missing_end_marker_returns_remaining_lumps() {
        let wad = test_wad();
        let names = wad
            .get_lumps_between(LumpName(*b"START\0\0\0"), LumpName(*b"MISSING\0"))
            .map(|lump| lump.name)
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                LumpName(*b"FIRST\0\0\0"),
                LumpName(*b"END\0\0\0\0\0"),
                LumpName(*b"SECOND\0\0"),
            ]
        );
    }

    #[test]
    fn end_marker_before_start_marker_produces_empty_range() {
        let wad = test_wad();
        assert_eq!(
            wad.get_lumps_between(LumpName(*b"SECOND\0\0"), LumpName(*b"START\0\0\0"))
                .count(),
            0
        );
    }

    #[test]
    fn equal_markers_produce_empty_range() {
        let wad = test_wad();
        assert_eq!(
            wad.get_lumps_between(LumpName(*b"START\0\0\0"), LumpName(*b"START\0\0\0"))
                .count(),
            0
        );
    }
}

mod load_wad_success {
    use super::*;

    #[test]
    fn loads_empty_iwad() {
        let wad = load_fixture_for_test("valid_empty_iwad.wad").unwrap();
        assert!(wad.lumps.is_empty());
    }

    #[test]
    fn loads_empty_pwad() {
        let wad = load_fixture_for_test("valid_empty_pwad.wad").unwrap();
        assert!(wad.lumps.is_empty());
    }

    #[test]
    fn loads_single_lump() {
        let wad = load_fixture_for_test("valid_single_lump.wad").unwrap();
        assert_eq!(wad.lumps.len(), 1);
        assert_lump_data(&wad, LumpName(*b"HELLO\0\0\0"), &[b"HELLO WAD"]);
    }

    #[test]
    fn loads_multiple_lumps() {
        let wad = load_fixture_for_test("valid_multiple_lumps.wad").unwrap();
        assert_eq!(wad.lumps.len(), 3);
        assert_lump_data(&wad, LumpName(*b"LUMPONE\0"), &[b"ONE"]);
        assert_lump_data(&wad, LumpName(*b"LUMPTWO\0"), &[b"TWO-TWO"]);
        assert_lump_data(&wad, LumpName(*b"LUMPTHR\0"), &[b"THREE"]);
    }

    #[test]
    fn loads_empty_lump() {
        let wad = load_fixture_for_test("valid_empty_lump.wad").unwrap();
        assert_eq!(wad.lumps.len(), 1);
        assert_lump_data(&wad, LumpName(*b"EMPTY\0\0\0"), &[b""]);
    }

    #[test]
    fn loads_null_padded_name() {
        let wad = load_fixture_for_test("valid_null_padded_name.wad").unwrap();
        assert_lump_data(&wad, LumpName(*b"PLAYPAL\0"), &[b"palette"]);
    }

    #[test]
    fn duplicates_preserve_all_mentions() {
        let wad = load_fixture_for_test("duplicate_names.wad").unwrap();
        assert_lump_data(&wad, LumpName(*b"DUPL\0\0\0\0"), &[b"first", b"second"]);
    }

    #[test]
    fn loads_overlapping_lumps_independently() {
        let wad = load_fixture_for_test("overlapping_lumps.wad").unwrap();
        assert_eq!(wad.lumps.len(), 2);
        assert_lump_data(&wad, LumpName(*b"FIRST\0\0\0"), &[b"ABCDE"]);
        assert_lump_data(&wad, LumpName(*b"SECOND\0\0"), &[b"DEFGHI"]);
    }

    #[test]
    fn ignores_trailing_bytes_after_directory() {
        let wad = load_fixture_for_test("trailing_bytes.wad").unwrap();
        assert_eq!(wad.lumps.len(), 1);
        assert_lump_data(&wad, LumpName(*b"DATA\0\0\0\0"), &[b"DATA"]);
    }

    #[test]
    fn accepts_directory_beyond_eof_without_lumps() {
        let wad = load_fixture_for_test("directory_beyond_eof_empty.wad").unwrap();
        assert!(wad.lumps.is_empty());
    }

    #[test]
    fn loads_lump_with_gaps_before_payload_and_directory() {
        let wad = load_fixture_for_test("gapped_layout.wad").unwrap();
        assert_lump_data(&wad, LumpName(*b"GAPPED\0\0"), &[b"GAP"]);
    }

    #[test]
    fn loads_multiple_lumps_sharing_payload() {
        let wad = load_fixture_for_test("shared_lump_data.wad").unwrap();
        assert_lump_data(&wad, LumpName(*b"SAMEONE\0"), &[b"SHARED"]);
        assert_lump_data(&wad, LumpName(*b"SAMETWO\0"), &[b"SHARED"]);
    }

    #[test]
    fn loads_zero_sized_lump_beyond_eof() {
        let wad = load_fixture_for_test("zero_sized_lump_beyond_eof.wad").unwrap();
        assert_lump_data(&wad, LumpName(*b"ZERO\0\0\0\0"), &[b""]);
    }

    #[test]
    fn loads_lump_overlapping_header() {
        let wad = load_fixture_for_test("lump_overlaps_header.wad").unwrap();
        assert_lump_data(
            &wad,
            LumpName(*b"HEADER\0\0"),
            &[b"PWAD\x01\x00\x00\x00\x0c\x00\x00\x00"],
        );
    }

    #[test]
    fn loads_lump_overlapping_directory() {
        let wad = load_fixture_for_test("lump_overlaps_directory.wad").unwrap();
        assert_lump_data(
            &wad,
            LumpName(*b"DIRDATA\0"),
            &[b"\x0c\x00\x00\x00\x10\x00\x00\x00DIRDATA\x00"],
        );
    }

    #[test]
    fn loads_freedoom1() {
        assert!(load_fixture_for_test("../../freedoom1.wad").is_some());
    }

    #[test]
    fn loads_freedoom2() {
        assert!(load_fixture_for_test("../../freedoom2.wad").is_some());
    }
}

mod load_wad_errors {
    use super::*;

    #[test]
    fn rejects_invalid_magic() {
        assert!(matches!(
            load_fixture("invalid_magic.wad"),
            Err(WadLoadingError::InvalidHeader)
        ));
    }

    #[test]
    fn rejects_invalid_lump_name() {
        assert!(matches!(
            load_fixture("invalid_lump_name.wad"),
            Err(WadLoadingError::InvalidLumpName(_))
        ));
    }

    #[test]
    fn rejects_malformed_null_padded_name() {
        assert!(matches!(
            load_fixture("malformed_null_padded_name.wad"),
            Err(WadLoadingError::InvalidLumpName(_))
        ));
    }

    #[test]
    fn rejects_directory_beyond_eof_with_lumps() {
        assert!(matches!(
            load_fixture("directory_beyond_eof.wad"),
            Err(WadLoadingError::CouldntReadFile(_))
        ));
    }

    #[test]
    fn rejects_truncated_header() {
        assert!(matches!(
            load_fixture("truncated_header.wad"),
            Err(WadLoadingError::CouldntReadFile(_))
        ));
    }

    #[test]
    fn rejects_truncated_directory() {
        assert!(matches!(
            load_fixture("truncated_directory.wad"),
            Err(WadLoadingError::CouldntReadFile(_))
        ));
    }

    #[test]
    fn rejects_truncated_lump_data() {
        assert!(matches!(
            load_fixture("truncated_lump_data.wad"),
            Err(WadLoadingError::CouldntReadFile(_))
        ));
    }

    #[test]
    fn rejects_missing_file() {
        assert!(matches!(
            load_fixture("this_file_does_not_exist.wad"),
            Err(WadLoadingError::CouldntReadFile(_))
        ));
    }
}

mod patch_wads {
    use super::*;

    #[test]
    fn doom1_style_wad_patches_override_e1m1_data() {
        let iwad = load_fixture_for_test("iwad_doom1.wad");
        assert!(iwad.is_some());
        let mut wad = iwad.unwrap();
        let pwad = load_fixture_for_test("pwad_doom1_override.wad");
        assert!(pwad.is_some());
        let pwad = pwad.unwrap();

        patch_wad(&mut wad, &pwad);

        let e1m1_marker_pos = wad.get_lump_index_by_str_name("E1M1");
        assert!(e1m1_marker_pos.is_some());
        let e1m1_marker_pos = e1m1_marker_pos.unwrap();

        let e1m1_lump = wad.get_lump_at(e1m1_marker_pos + 1);
        assert_eq!(
            e1m1_lump.map(Lump::get_raw_data),
            Some(vec![0x02; 4].as_slice())
        );
    }

    #[test]
    fn doom2_two_maps_patch_overrides_first() {
        let iwad = load_fixture_for_test("iwad_two_maps.wad");
        assert!(iwad.is_some());
        let mut wad = iwad.unwrap();
        let pwad = load_fixture_for_test("pwad_map_override.wad");
        assert!(pwad.is_some());
        let pwad = pwad.unwrap();

        patch_wad(&mut wad, &pwad);

        let map01_marker_pos = wad.get_lump_index_by_str_name("MAP01");
        assert!(map01_marker_pos.is_some());
        let map01_marker_pos = map01_marker_pos.unwrap();

        let map01_lump = wad.get_lump_at(map01_marker_pos + 1);
        assert_eq!(
            map01_lump.map(Lump::get_raw_data),
            Some(vec![0x02; 4].as_slice())
        );
    }

    #[test]
    fn doom2_one_map_patch_overrides() {
        let iwad = load_fixture_for_test("iwad_one_map.wad");
        assert!(iwad.is_some());
        let mut wad = iwad.unwrap();
        let pwad = load_fixture_for_test("pwad_map_override.wad");
        assert!(pwad.is_some());
        let pwad = pwad.unwrap();

        patch_wad(&mut wad, &pwad);

        let map01_marker_pos = wad.get_lump_index_by_str_name("MAP01");
        assert!(map01_marker_pos.is_some());
        let map01_marker_pos = map01_marker_pos.unwrap();

        let map01_lump = wad.get_lump_at(map01_marker_pos + 1);
        assert_eq!(
            map01_lump.map(Lump::get_raw_data),
            Some(vec![0x02; 4].as_slice())
        );
    }

    #[test]
    fn sprite_namespace_overridden_global_not() {
        let iwad = load_fixture_for_test("iwad_sprites.wad");
        assert!(iwad.is_some());
        let iwad = iwad.unwrap();
        let mut wad = iwad.clone();
        let pwad = load_fixture_for_test("pwad_sprite_override.wad");
        assert!(pwad.is_some());
        let pwad = pwad.unwrap();

        patch_wad(&mut wad, &pwad);
        assert_eq!(wad.lumps.len(), 4);
        let patch_sprite_lump = pwad
            .get_lump_by_str_name_and_namespace("TROOPY", Namespace::Sprites)
            .unwrap();
        let patched_sprite_lump =
            wad.get_lump_by_str_name_and_namespace("TROOPY", Namespace::Sprites);
        assert!(patched_sprite_lump.is_some());
        let patched_sprite_lump = patched_sprite_lump.unwrap();
        assert_eq!(
            patched_sprite_lump.get_raw_data(),
            patch_sprite_lump.get_raw_data()
        );

        let iwad_playpal_lump = iwad
            .get_lump_by_str_name_and_namespace("PLAYPAL", Namespace::Global)
            .unwrap();
        let patched_playpal_lump =
            wad.get_lump_by_str_name_and_namespace("PLAYPAL", Namespace::Global);
        assert!(patched_playpal_lump.is_some());
        let patched_playpal_lump = patched_playpal_lump.unwrap();
        assert_eq!(
            patched_playpal_lump.get_raw_data(),
            iwad_playpal_lump.get_raw_data()
        );
    }

    #[test]
    fn patch_adds_second_map() {
        let iwad = load_fixture_for_test("iwad_one_map.wad");
        assert!(iwad.is_some());
        let mut wad = iwad.unwrap();
        let pwad = load_fixture_for_test("pwad_new_map.wad");
        assert!(pwad.is_some());
        let pwad = pwad.unwrap();

        patch_wad(&mut wad, &pwad);

        let map01_marker_pos = wad.get_lump_index_by_str_name("MAP01");
        assert!(map01_marker_pos.is_some());
        let map01_marker_pos = map01_marker_pos.unwrap();

        let map01_lump = wad.get_lump_at(map01_marker_pos + 1);
        assert_eq!(
            map01_lump.map(Lump::get_raw_data),
            Some(vec![0x01; 4].as_slice())
        );

        let map02_marker_pos = wad.get_lump_index_by_str_name("MAP02");
        assert!(map02_marker_pos.is_some());
        let map02_marker_pos = map02_marker_pos.unwrap();

        let map02_lump = wad.get_lump_at(map02_marker_pos + 1);
        assert_eq!(
            map02_lump.map(Lump::get_raw_data),
            Some(vec![0x02; 4].as_slice())
        );
    }

    #[test]
    fn multi_namespace_same_lump_name_different_namespaces() {
        let iwad = load_fixture_for_test("iwad_multi_namespace.wad");
        assert!(iwad.is_some());
        let iwad = iwad.unwrap();
        let mut wad = iwad.clone();
        let pwad = load_fixture_for_test("pwad_sprite_same_name.wad");
        assert!(pwad.is_some());
        let pwad = pwad.unwrap();

        // Before patch: IWAD has DOOM in both sprites and flats namespaces
        let iwad_sprites_doom = iwad.get_lump_by_str_name_and_namespace("DOOM", Namespace::Sprites);
        assert!(iwad_sprites_doom.is_some());
        let iwad_sprites_doom = iwad_sprites_doom.unwrap();
        assert_eq!(iwad_sprites_doom.get_raw_data(), vec![0x01; 8].as_slice());

        let iwad_flats_doom = iwad.get_lump_by_str_name_and_namespace("DOOM", Namespace::Flats);
        assert!(iwad_flats_doom.is_some());
        let iwad_flats_doom = iwad_flats_doom.unwrap();
        assert_eq!(iwad_flats_doom.get_raw_data(), vec![0x02; 8].as_slice());

        patch_wad(&mut wad, &pwad);

        // After patch: sprites DOOM is replaced, flats DOOM remains
        let pwad_sprites_doom = pwad.get_lump_by_str_name_and_namespace("DOOM", Namespace::Sprites);
        assert!(pwad_sprites_doom.is_some());
        let pwad_sprites_doom = pwad_sprites_doom.unwrap();

        let patched_sprites_doom =
            wad.get_lump_by_str_name_and_namespace("DOOM", Namespace::Sprites);
        assert!(patched_sprites_doom.is_some());
        let patched_sprites_doom = patched_sprites_doom.unwrap();
        assert_eq!(
            patched_sprites_doom.get_raw_data(),
            pwad_sprites_doom.get_raw_data()
        );

        // Flats namespace should still have original data
        let patched_flats_doom = wad.get_lump_by_str_name_and_namespace("DOOM", Namespace::Flats);
        assert!(patched_flats_doom.is_some());
        let patched_flats_doom = patched_flats_doom.unwrap();
        assert_eq!(
            patched_flats_doom.get_raw_data(),
            iwad_flats_doom.get_raw_data()
        );
    }

    #[test]
    fn marker_lumps_create_namespace_isolation() {
        let iwad = load_fixture_for_test("iwad_marked.wad");
        assert!(iwad.is_some());
        let iwad = iwad.unwrap();
        let mut wad = iwad.clone();
        let pwad = load_fixture_for_test("pwad_marked.wad");
        assert!(pwad.is_some());
        let pwad = pwad.unwrap();

        // Verify markers are properly set before patching
        let iwad_s_start = iwad.get_lump_by_str_name("S_START");
        assert!(iwad_s_start.is_some());
        let iwad_s_end = iwad.get_lump_by_str_name("S_END");
        assert!(iwad_s_end.is_some());

        patch_wad(&mut wad, &pwad);

        // After patching, markers should still exist
        let s_start = wad.get_lump_by_str_name("S_START");
        assert!(s_start.is_some());
        let s_end = wad.get_lump_by_str_name("S_END");
        assert!(s_end.is_some());
        let f_start = wad.get_lump_by_str_name("F_START");
        assert!(f_start.is_some());
        let f_end = wad.get_lump_by_str_name("F_END");
        assert!(f_end.is_some());
        let c_start = wad.get_lump_by_str_name("C_START");
        assert!(c_start.is_some());
        let c_end = wad.get_lump_by_str_name("C_END");
        assert!(c_end.is_some());
    }

    #[test]
    fn patch_preserves_original_lumps_outside_namespace() {
        let iwad = load_fixture_for_test("iwad_sprites.wad");
        assert!(iwad.is_some());
        let iwad = iwad.unwrap();
        let mut wad = iwad.clone();
        let pwad = load_fixture_for_test("pwad_sprite_override.wad");
        assert!(pwad.is_some());
        let pwad = pwad.unwrap();

        // Verify PLAYPAL exists in IWAD (in global namespace)
        let iwad_playpal = iwad.get_lump_by_str_name("PLAYPAL");
        assert!(iwad_playpal.is_some());
        let iwad_playpal_data = iwad_playpal.unwrap().get_raw_data();

        patch_wad(&mut wad, &pwad);

        // After patching, PLAYPAL should remain unchanged (not in PWAD sprite namespace)
        let patched_playpal = wad.get_lump_by_str_name("PLAYPAL");
        assert!(patched_playpal.is_some());
        assert_eq!(patched_playpal.unwrap().get_raw_data(), iwad_playpal_data);

        // Sprite namespace TROOPY should be replaced
        let patched_troopy = wad.get_lump_by_str_name_and_namespace("TROOPY", Namespace::Sprites);
        assert!(patched_troopy.is_some());
        let pwad_troopy = pwad
            .get_lump_by_str_name_and_namespace("TROOPY", Namespace::Sprites)
            .unwrap();
        assert_eq!(
            patched_troopy.unwrap().get_raw_data(),
            pwad_troopy.get_raw_data()
        );
    }

    #[test]
    fn multiple_doom2_maps_patch_adds_and_overrides() {
        let iwad = load_fixture_for_test("iwad_two_maps.wad");
        assert!(iwad.is_some());
        let mut wad = iwad.unwrap();
        let pwad = load_fixture_for_test("pwad_new_map.wad");
        assert!(pwad.is_some());
        let pwad = pwad.unwrap();

        // IWAD has MAP01 and MAP02
        let iwad_map01_pos = wad.get_lump_index_by_str_name("MAP01");
        assert!(iwad_map01_pos.is_some());
        let iwad_map02_pos = wad.get_lump_index_by_str_name("MAP02");
        assert!(iwad_map02_pos.is_some());

        // PWAD adds MAP02 (should not override existing MAP02 in one-map IWAD scenario,
        // but since we have two maps, should append after MAP01)
        patch_wad(&mut wad, &pwad);

        // MAP01 should still be original (PWAD doesn't override it)
        let map01_pos = wad.get_lump_index_by_str_name("MAP01");
        assert!(map01_pos.is_some());
        let map01_pos = map01_pos.unwrap();
        let map01_lump = wad.get_lump_at(map01_pos + 1);
        assert_eq!(
            map01_lump.map(Lump::get_raw_data),
            Some(vec![0x01; 4].as_slice())
        );

        // MAP02 should exist and have PWAD data
        let map02_pos = wad.get_lump_index_by_str_name("MAP02");
        assert!(map02_pos.is_some());
        // The patched MAP02 should have data from PWAD (0x02)
        let map02_pos = map02_pos.unwrap();
        let map02_lump = wad.get_lump_at(map02_pos + 1);
        assert_eq!(
            map02_lump.map(Lump::get_raw_data),
            Some(vec![0x02; 4].as_slice())
        );
    }
}
