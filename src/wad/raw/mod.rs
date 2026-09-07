use std::fs::read;
use std::path::PathBuf;
use thiserror::Error;
use winnow::Result;
use winnow::binary::le_u32;
use winnow::combinator::alt;
use winnow::prelude::*;
use winnow::token::literal;

pub struct WadView {}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct HeaderInfo {
    num_lumps: u32,
    info_table_offset: u32,
}

#[derive(Debug, Error)]
pub enum WadLoadingError {
    #[error("Error reading file: {}", .0)]
    MissingFile(#[from] std::io::Error),
    #[error("Invalid header")]
    InvalidHeader,
}

/// # Errors
/// Will error out on missing file or on errors parsing the WAD file.
pub fn load_wad(path: PathBuf) -> Result<WadView, WadLoadingError> {
    let raw_bytes = read(path)?;

    get_header_info
        .parse(&raw_bytes)
        .map_err(|_| WadLoadingError::InvalidHeader)?;

    Ok(WadView {
        // Initialize fields as necessary
    })
}

/// Check if the input is a valid WAD type (IWAD or PWAD).
/// # Errors
/// Will error out if it cannot take 4 bytes from the input or if the bytes do not match "IWAD" or "PWAD".
fn is_valid_wad_type(input: &mut &[u8]) -> Result<()> {
    alt((literal(b"IWAD"), literal(b"PWAD")))
        .parse_next(input)
        .map(|_| ())
}

/// # Errors
/// Will error out if it cannot take 8 bytes from the input.
fn get_num_lumps_and_info_table_offset(input: &mut &[u8]) -> Result<HeaderInfo> {
    let num_lumps: u32 = le_u32.parse_next(input)?;
    let info_table_offset: u32 = le_u32.parse_next(input)?;

    Ok(HeaderInfo {
        num_lumps,
        info_table_offset,
    })
}

fn get_header_info(input: &mut &[u8]) -> Result<HeaderInfo> {
    is_valid_wad_type(input)?;

    get_num_lumps_and_info_table_offset(input)
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn iwad_is_valid_wad_type() {
        let mut input = b"IWAD".as_ref();
        let out = is_valid_wad_type(&mut input);
        assert_eq!(out, Ok(()));
        assert!(input.is_empty());
    }

    #[test]
    fn pwad_is_valid_wad_type() {
        let mut input = b"PWAD".as_ref();
        let out = is_valid_wad_type(&mut input);
        assert_eq!(out, Ok(()));
        assert!(input.is_empty());
    }

    #[test]
    fn invalid_wad_type() {
        let mut input = b"INVALID".as_ref();
        let out = is_valid_wad_type(&mut input);
        dbg!(&out);
        assert!(out.is_err());
    }

    #[test]
    fn num_lumps_and_info_table_offset_read_correctly() {
        let mut input = b"\x02\x00\x00\x00\x10\x00\x00\x00".as_ref();
        let out = get_num_lumps_and_info_table_offset(&mut input);
        assert_eq!(
            out,
            Ok(HeaderInfo {
                num_lumps: 2,
                info_table_offset: 16
            })
        );
        assert!(input.is_empty());
    }

    #[test]
    fn get_header_info_valid_iwad() {
        let mut input = b"IWAD\x09\x00\x00\x00\x13\x00\x00\x00".as_ref();
        let out = get_header_info(&mut input);
        assert_eq!(
            out,
            Ok(HeaderInfo {
                num_lumps: 9,
                info_table_offset: 19
            })
        );
        assert!(input.is_empty());
    }

    #[test]
    fn get_header_info_valid_pwad() {
        let mut input = b"PWAD\x05\x01\x00\x00\xE5\x02\x00\x00".as_ref();
        let out = get_header_info(&mut input);
        assert_eq!(
            out,
            Ok(HeaderInfo {
                num_lumps: 261,
                info_table_offset: 741
            })
        );
        assert!(input.is_empty());
    }

    #[test]
    fn get_header_info_invalid() {
        let mut input = b"INVALID\x00\x00\x00\x00\x00".as_ref();
        let out = get_header_info(&mut input);
        assert!(out.is_err());
    }
}
