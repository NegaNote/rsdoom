use super::*;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test_wads")
        .join(name)
}

fn load_fixture(name: &str) -> Result<WadView, WadLoadingError> {
    load_wad(fixture(name))
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
}

mod header {
    use super::*;

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
}

mod lump_info {
    use super::*;

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
    fn reads_empty_name() {
        let mut input = b"\x00\x00\x00\x00\x05\x00\x00\x00\0\0\0\0\0\0\0\0".as_ref();
        assert_eq!(
            get_lump_info(&mut input),
            Ok(LumpInfo {
                offset: 0,
                size: 5,
                name: LumpName(*b"\0\0\0\0\0\0\0\0")
            })
        );
        assert!(input.is_empty());
    }

    #[test]
    fn rejects_malformed_null_padding() {
        let mut input = b"\x00\x00\x00\x00\x05\x00\x00\x00LUMP\0\0\0X".as_ref();
        assert!(get_lump_info(&mut input).is_err());
    }

    #[test]
    fn rejects_non_ascii_graphic_name() {
        let mut input = b"\x00\x00\x00\x00\x05\x00\x00\x00LUMP\x01\0\0\0".as_ref();
        assert!(get_lump_info(&mut input).is_err());
    }
}

mod load_wad_success {
    use super::*;

    #[test]
    fn loads_empty_iwad() {
        if let Some(wad) = load_fixture_for_test("valid_empty_iwad.wad") {
            assert!(wad.lumps.is_empty());
        }
    }

    #[test]
    fn loads_empty_pwad() {
        if let Some(wad) = load_fixture_for_test("valid_empty_pwad.wad") {
            assert!(wad.lumps.is_empty());
        }
    }

    #[test]
    fn loads_single_lump() {
        if let Some(wad) = load_fixture_for_test("valid_single_lump.wad") {
            assert_eq!(wad.lumps.len(), 1);
            assert_lump_data(&wad, LumpName(*b"HELLO\0\0\0"), &[b"HELLO WAD"]);
        }
    }

    #[test]
    fn loads_multiple_lumps() {
        if let Some(wad) = load_fixture_for_test("valid_multiple_lumps.wad") {
            assert_eq!(wad.lumps.len(), 3);
            assert_lump_data(&wad, LumpName(*b"LUMPONE\0"), &[b"ONE"]);
            assert_lump_data(&wad, LumpName(*b"LUMPTWO\0"), &[b"TWO-TWO"]);
            assert_lump_data(&wad, LumpName(*b"LUMPTHR\0"), &[b"THREE"]);
        }
    }

    #[test]
    fn loads_empty_lump() {
        if let Some(wad) = load_fixture_for_test("valid_empty_lump.wad") {
            assert_eq!(wad.lumps.len(), 1);
            assert_lump_data(&wad, LumpName(*b"EMPTY\0\0\0"), &[b""]);
        }
    }

    #[test]
    fn loads_null_padded_name() {
        if let Some(wad) = load_fixture_for_test("valid_null_padded_name.wad") {
            assert_lump_data(&wad, LumpName(*b"PLAYPAL\0"), &[b"palette"]);
        }
    }

    #[test]
    fn duplicates_preserve_all_mentions() {
        if let Some(wad) = load_fixture_for_test("duplicate_names.wad") {
            assert_lump_data(&wad, LumpName(*b"DUPL\0\0\0\0"), &[b"first", b"second"]);
        }
    }

    #[test]
    fn loads_overlapping_lumps_independently() {
        if let Some(wad) = load_fixture_for_test("overlapping_lumps.wad") {
            assert_eq!(wad.lumps.len(), 2);
            assert_lump_data(&wad, LumpName(*b"FIRST\0\0\0"), &[b"ABCDE"]);
            assert_lump_data(&wad, LumpName(*b"SECOND\0\0"), &[b"DEFGHI"]);
        }
    }

    #[test]
    fn ignores_trailing_bytes_after_directory() {
        if let Some(wad) = load_fixture_for_test("trailing_bytes.wad") {
            assert_eq!(wad.lumps.len(), 1);
            assert_lump_data(&wad, LumpName(*b"DATA\0\0\0\0"), &[b"DATA"]);
        }
    }

    #[test]
    fn accepts_directory_beyond_eof_without_lumps() {
        if let Some(wad) = load_fixture_for_test("directory_beyond_eof_empty.wad") {
            assert!(wad.lumps.is_empty());
        }
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
