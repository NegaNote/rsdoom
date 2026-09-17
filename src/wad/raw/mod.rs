use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::str::FromStr;
use thiserror::Error;
use usize_conv::ToUsize;
use winnow::Result;
use winnow::binary::le_u32;
use winnow::combinator::alt;
use winnow::prelude::*;
use winnow::token::{literal, take};

#[derive(Debug, PartialEq, Eq)]
pub struct WadView {
    lumps: Vec<Lump>,
}

impl WadView {
    #[must_use]
    pub fn get_lump_by_name(&self, name: LumpName) -> Option<&Lump> {
        self.lumps.iter().find(|lump| lump.name == name)
    }

    #[must_use]
    pub fn get_lump_by_str_name(&self, name: &str) -> Option<&Lump> {
        let lump_name = LumpName::from_str(name).ok()?;
        self.get_lump_by_name(lump_name)
    }

    #[must_use]
    pub fn get_lumps_by_name(&self, name: LumpName) -> Vec<&Lump> {
        self.lumps.iter().filter(|lump| lump.name == name).collect()
    }

    #[must_use]
    pub fn get_lump_at(&self, index: usize) -> Option<&Lump> {
        self.lumps.get(index)
    }

    pub fn get_lumps_between(
        &self,
        start_marker: LumpName,
        end_marker: LumpName,
    ) -> impl Iterator<Item = &Lump> {
        self.lumps
            .iter()
            .skip_while(move |lump| lump.name != start_marker)
            .skip(1) // Skip the start marker itself
            .take_while(move |lump| start_marker != end_marker && lump.name != end_marker)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct LumpName([u8; 8]);

#[derive(Debug, PartialEq, Eq)]
pub struct Lump {
    name: LumpName,
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
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid lump name")
    }
}

impl TryFrom<[u8; 8]> for LumpName {
    type Error = LumpNameError;

    fn try_from(value: [u8; 8]) -> Result<Self, Self::Error> {
        let valid = value.iter().position(|&byte| byte == 0).map_or_else(
            || value.iter().all(u8::is_ascii),
            |null_pos| {
                null_pos != 0
                    && value.iter().take(null_pos).all(u8::is_ascii)
                    && value.iter().skip(null_pos + 1).all(|&b| b == 0)
            },
        );

        if !valid {
            return Err(LumpNameError);
        }

        Ok(Self(value))
    }
}

impl LumpName {
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.iter().position(|&b| b == 0).map_or_else(
            || {
                // SAFETY: The invariant of LumpName ensures that all bytes are ASCII graphic characters if there are no zero bytes.
                unsafe { std::str::from_utf8_unchecked(&self.0) }
            },
            |pos| {
                // SAFETY: The invariant of LumpName ensures that all bytes before the first zero byte are ASCII graphic characters.
                unsafe { std::str::from_utf8_unchecked(self.0.get_unchecked(..pos)) }
            },
        )
    }
}

impl FromStr for LumpName {
    type Err = LumpNameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() > 8 || !s.is_ascii() {
            return Err(LumpNameError);
        }

        let mut bytes = [0u8; 8];
        if s.len() == 8 {
            bytes.copy_from_slice(s.as_bytes());
        } else {
            bytes
                .get_mut(..s.len())
                .unwrap_or_default()
                .copy_from_slice(s.as_bytes());
        }
        Self::try_from(bytes)
    }
}

impl Display for LumpName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
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

    let mut lumps: Vec<Lump> = Vec::with_capacity(lump_infos.len());

    for lump_info in lump_infos {
        wad_file.seek(SeekFrom::Start(u64::from(lump_info.offset)))?;
        let mut raw_data = vec![0u8; lump_info.size.to_usize()];
        wad_file.read_exact(&mut raw_data)?;
        lumps.push(Lump {
            name: lump_info.name,
            raw_data,
        });
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
