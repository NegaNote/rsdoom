use std::fmt::Display;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use thiserror::Error;
use usize_conv::ToUsize;
use winnow::Result;
use winnow::binary::le_u32;
use winnow::combinator::alt;
use winnow::prelude::*;
use winnow::token::{literal, take};

#[derive(Debug, PartialEq, Eq)]
pub struct WadView {
    lumps: Vec<(LumpName, Lump)>,
}

impl WadView {
    #[must_use]
    pub fn get_lump_by_name(&self, name: LumpName) -> Option<&Lump> {
        self.lumps
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, lump)| lump)
    }

    #[must_use]
    pub fn get_lumps_by_name(&self, name: LumpName) -> Vec<&Lump> {
        self.lumps
            .iter()
            .filter(|(n, _)| *n == name)
            .map(|(_, lump)| lump)
            .collect()
    }

    #[must_use]
    pub fn get_lump_at(&self, index: usize) -> Option<&Lump> {
        self.lumps.get(index).map(|(_, lump)| lump)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct LumpName([u8; 8]);

#[derive(Debug, PartialEq, Eq)]
pub struct Lump {
    raw_data: Vec<u8>,
}

impl Lump {
    #[must_use]
    pub fn get_raw_data(&self) -> &[u8] {
        &self.raw_data
    }
}

#[derive(Debug, PartialEq, Eq, Error)]
pub struct LumpNameError;

impl Display for LumpNameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid lump name")
    }
}

impl TryFrom<[u8; 8]> for LumpName {
    type Error = LumpNameError;

    fn try_from(value: [u8; 8]) -> Result<Self, Self::Error> {
        if let Some(pos) = value.iter().position(|&b| b == 0) {
            // If there's a null byte, check that all bytes after it are also null and that all bytes before it are ASCII graphic characters.
            if let Some((slice1, slice2)) = value.split_at_checked(pos)
                && let Some(slice3) = slice2.get(1..)
                && (slice3.iter().any(|&b| b != 0) || slice1.iter().any(|&b| !b.is_ascii_graphic()))
            {
                return Err(LumpNameError);
            }
        } else if value.iter().any(|&b| !b.is_ascii_graphic()) {
            // Otherwise just make sure all bytes are ASCII graphic characters.
            return Err(LumpNameError);
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct HeaderInfo {
    num_lumps: u32,
    info_table_offset: u32,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct LumpInfo {
    offset: u32,
    size: u32,
    name: LumpName,
}

#[derive(Debug, Error)]
pub enum WadLoadingError {
    #[error("Error reading file: {}", .0)]
    CouldntReadFile(#[from] std::io::Error),
    #[error("Invalid header")]
    InvalidHeader,
    #[error("Invalid lump name")]
    InvalidLumpName(#[from] LumpNameError),
}

/// # Errors
/// Will error out on missing file or on errors parsing the WAD file.
pub fn load_wad(path: PathBuf) -> Result<WadView, WadLoadingError> {
    let mut wad_file: File = File::open(path).map_err(WadLoadingError::CouldntReadFile)?;
    wad_file.seek(SeekFrom::Start(0))?;
    let mut header_bytes = [0u8; 12];
    wad_file.read_exact(&mut header_bytes)?;

    let header_info = get_header_info
        .parse(&header_bytes)
        .map_err(|_| WadLoadingError::InvalidHeader)?;

    wad_file.seek(SeekFrom::Start(u64::from(header_info.info_table_offset)))?;

    let mut lump_infos: Vec<LumpInfo> = Vec::with_capacity(header_info.num_lumps.to_usize());

    for _ in 0..header_info.num_lumps {
        let mut lump_info_bytes = [0u8; 16];
        wad_file.read_exact(&mut lump_info_bytes)?;
        let lump_info = get_lump_info
            .parse(&lump_info_bytes)
            .map_err(|_| WadLoadingError::InvalidLumpName(LumpNameError))?;

        lump_infos.push(lump_info);
    }

    let mut lumps: Vec<(LumpName, Lump)> = Vec::with_capacity(lump_infos.len());

    for lump_info in lump_infos {
        wad_file.seek(SeekFrom::Start(u64::from(lump_info.offset)))?;
        let mut raw_data = vec![0u8; lump_info.size.to_usize()];
        wad_file.read_exact(&mut raw_data)?;
        lumps.push((lump_info.name, Lump { raw_data }));
    }

    Ok(WadView { lumps })
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

fn get_lump_info(input: &mut &[u8]) -> Result<LumpInfo> {
    let offset: u32 = le_u32.parse_next(input)?;
    let size: u32 = le_u32.parse_next(input)?;
    let name = take(8usize)
        .map(|name_bytes: &[u8]| {
            let mut name = [0u8; 8];
            name.copy_from_slice(name_bytes);
            name
        })
        .try_map(LumpName::try_from)
        .parse_next(input)?;

    Ok(LumpInfo { offset, size, name })
}

#[cfg(test)]
mod tests;
