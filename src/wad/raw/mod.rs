use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::str::FromStr;
use thiserror::Error;
use usize_conv::ToUsize;
use winnow::Result;
use winnow::binary::le_u32;
use winnow::combinator::alt;
use winnow::prelude::*;
use winnow::token::{literal, take};

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct WadView {
    lumps: Vec<Lump>,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum WadType {
    Iwad,
    Pwad,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Namespace {
    Global,
    Sprites,
    Flats,
    Colormaps,
    PrBoom,
    Demos,
    HiRes,
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
    pub fn get_lump_by_str_name_and_namespace(
        &self,
        name: &str,
        namespace: Namespace,
    ) -> Option<&Lump> {
        let lump_name = LumpName::from_str(name).ok()?;
        self.lumps
            .iter()
            .find(|lump| lump.name == lump_name && lump.namespace == namespace)
    }

    #[must_use]
    pub fn get_lump_at(&self, index: usize) -> Option<&Lump> {
        self.lumps.get(index)
    }

    #[must_use]
    pub fn get_lump_index_by_str_name(&self, name: &str) -> Option<usize> {
        let lump_name = LumpName::from_str(name).ok()?;
        self.lumps.iter().position(|lump| lump.name == lump_name)
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

    pub fn get_lumps_between_mut(
        &mut self,
        start_marker: LumpName,
        end_marker: LumpName,
    ) -> impl Iterator<Item = &mut Lump> {
        self.lumps
            .iter_mut()
            .skip_while(move |lump| lump.name != start_marker)
            .skip(1) // Skip the start marker itself
            .take_while(move |lump| start_marker != end_marker && lump.name != end_marker)
    }

    pub fn get_lumps_by_namespace(&self, namespace: Namespace) -> impl Iterator<Item = &Lump> {
        self.lumps
            .iter()
            .filter(move |lump| lump.namespace == namespace)
    }
}

/// Lump names are 8-byte ASCII strings padded by null bytes.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct LumpName([u8; 8]);

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Lump {
    name: LumpName,
    raw_data: Vec<u8>,
    source_type: WadType,
    namespace: Namespace,
}

impl Lump {
    #[must_use]
    pub fn get_raw_data(&self) -> &[u8] {
        &self.raw_data
    }

    #[must_use]
    pub const fn get_namespace(&self) -> Namespace {
        self.namespace
    }

    #[must_use]
    pub const fn get_name(&self) -> LumpName {
        self.name
    }
}

#[derive(Debug, PartialEq, Eq, Error)]
#[error("Invalid lump name")]
pub struct LumpNameError;

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

impl TryFrom<&[u8]> for LumpName {
    type Error = LumpNameError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != 8 {
            return Err(LumpNameError);
        }
        let mut array = [0u8; 8];
        array.copy_from_slice(value);
        Self::try_from(array)
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
                // SAFETY: The invariant of LumpName ensures that all bytes are ASCII characters if there are no zero bytes.
                unsafe { std::str::from_utf8_unchecked(&self.0) }
            },
            |pos| {
                // SAFETY: The invariant of LumpName ensures that all bytes before the first zero byte are ASCII characters.
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
pub fn load_wad(path: &PathBuf, wad_type: WadType) -> Result<WadView, WadLoadingError> {
    let mut wad_file = File::open(path).map_err(WadLoadingError::CouldntReadFile)?;
    let mut bytes = Vec::new();
    wad_file.read_to_end(&mut bytes)?;

    drop(wad_file); // Explicitly drop the file handle to ensure it's closed before proceeding.

    let header_info = parse_wad_header(&bytes)?;
    let lump_infos = parse_wad_directory(&bytes, header_info)?;
    let lumps = lump_infos
        .into_iter()
        .map(|lump_info| read_lump(&bytes, wad_type, lump_info))
        .collect::<Result<Vec<_>, WadLoadingError>>()?;

    let mut wad_view = WadView { lumps };

    apply_namespaces_between_markers(
        &mut wad_view,
        LumpName(*b"S_START\0"),
        LumpName(*b"S_END\0\0\0"),
        Namespace::Sprites,
    );
    apply_namespaces_between_markers(
        &mut wad_view,
        LumpName(*b"SS_START"),
        LumpName(*b"SS_END\0\0"),
        Namespace::Sprites,
    );
    apply_namespaces_between_markers(
        &mut wad_view,
        LumpName(*b"F_START\0"),
        LumpName(*b"F_END\0\0\0"),
        Namespace::Flats,
    );
    apply_namespaces_between_markers(
        &mut wad_view,
        LumpName(*b"FF_START"),
        LumpName(*b"FF_END\0\0"),
        Namespace::Flats,
    );
    apply_namespaces_between_markers(
        &mut wad_view,
        LumpName(*b"C_START\0"),
        LumpName(*b"C_END\0\0\0"),
        Namespace::Colormaps,
    );
    apply_namespaces_between_markers(
        &mut wad_view,
        LumpName(*b"B_START\0"),
        LumpName(*b"B_END\0\0\0"),
        Namespace::PrBoom,
    );
    apply_namespaces_between_markers(
        &mut wad_view,
        LumpName(*b"HI_START"),
        LumpName(*b"HI_END\0\0"),
        Namespace::HiRes,
    );

    Ok(wad_view)
}

fn parse_wad_header(bytes: &[u8]) -> Result<HeaderInfo, WadLoadingError> {
    if bytes.len() < 12 {
        return Err(WadLoadingError::CouldntReadFile(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "truncated header",
        )));
    }

    let mut input = bytes;
    get_header_info
        .parse_next(&mut input)
        .map_err(|_| WadLoadingError::InvalidHeader)
}

fn parse_wad_directory(
    bytes: &[u8],
    header_info: HeaderInfo,
) -> Result<Vec<LumpInfo>, WadLoadingError> {
    if header_info.num_lumps == 0 {
        return Ok(Vec::new());
    }

    let directory_start = header_info.info_table_offset.to_usize();
    let directory = bytes.get(directory_start..).ok_or_else(|| {
        WadLoadingError::CouldntReadFile(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "directory beyond eof",
        ))
    })?;

    let num_lumps = header_info.num_lumps.to_usize();
    let mut remaining = directory;
    let lump_infos = (0..num_lumps)
        .map(|_| {
            let Some(entry) = remaining.get(..16) else {
                return Err(WadLoadingError::CouldntReadFile(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "truncated directory",
                )));
            };

            let mut entry_bytes = entry;
            let lump_info = get_lump_info
                .parse_next(&mut entry_bytes)
                .map_err(|_| WadLoadingError::InvalidLumpName(LumpNameError))?;

            remaining = remaining.get(16..).unwrap_or_default();
            Ok(lump_info)
        })
        .collect::<Result<Vec<_>, WadLoadingError>>()?;

    Ok(lump_infos)
}

fn read_lump(
    bytes: &[u8],
    wad_type: WadType,
    lump_info: LumpInfo,
) -> Result<Lump, WadLoadingError> {
    if lump_info.size == 0 {
        return Ok(Lump {
            name: lump_info.name,
            raw_data: Vec::new(),
            source_type: wad_type,
            namespace: Namespace::Global,
        });
    }

    let start = lump_info.offset.to_usize();
    let end = start
        .checked_add(lump_info.size.to_usize())
        .ok_or_else(|| {
            WadLoadingError::CouldntReadFile(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "lump size overflow",
            ))
        })?;

    let raw_data = bytes.get(start..end).ok_or_else(|| {
        WadLoadingError::CouldntReadFile(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "truncated lump data",
        ))
    })?;

    Ok(Lump {
        name: lump_info.name,
        raw_data: raw_data.to_vec(),
        source_type: wad_type,
        namespace: Namespace::Global,
    })
}

fn apply_namespaces_between_markers(
    wad: &mut WadView,
    start_marker: LumpName,
    end_marker: LumpName,
    namespace: Namespace,
) {
    for lump in wad.get_lumps_between_mut(start_marker, end_marker) {
        lump.namespace = namespace;
    }
}

pub fn patch_wad(wad: &mut WadView, patch_wad: &WadView) {
    patch_wad
        .lumps
        .iter()
        .filter(|patch_lump| !(is_map_lump(patch_lump.name) || is_map_marker(patch_lump.name)))
        .for_each(|patch_lump| {
            if let Some(existing) = wad
                .lumps
                .iter_mut()
                .find(|l| l.name == patch_lump.name && l.namespace == patch_lump.namespace)
            {
                existing.raw_data.clone_from(&patch_lump.raw_data);
                existing.source_type = patch_lump.source_type;
            } else {
                wad.lumps.push(patch_lump.clone());
            }
        });

    patch_wad
        .lumps
        .iter()
        .enumerate()
        .filter(|(_, patch_lump)| is_map_marker(patch_lump.name))
        .for_each(|(patch_map_idx, patch_lump)| {
            if let Some(wad_map_idx) = wad.lumps.iter().position(|l| l.name == patch_lump.name) {
                (1..=11).for_each(|offset| {
                    if let Some(patch_lump_to_add) = patch_wad.lumps.get(patch_map_idx + offset) {
                        match wad.lumps.get_mut(wad_map_idx + offset) {
                            Some(existing) => *existing = patch_lump_to_add.clone(),
                            None => wad.lumps.push(patch_lump_to_add.clone()),
                        }
                    }
                });
            } else {
                (0..=11).for_each(|offset| {
                    if let Some(patch_lump_to_add) = patch_wad.lumps.get(patch_map_idx + offset) {
                        wad.lumps.push(patch_lump_to_add.clone());
                    }
                });
            }
        });
}

#[must_use]
fn is_map_marker(name: LumpName) -> bool {
    let s = name.as_str();
    parse_map_marker(s).is_ok()
}

fn parse_map_marker(input: &str) -> Result<()> {
    use winnow::ascii::digit1;
    use winnow::combinator::eof;

    alt((
        // ExMx format: E[digits]M[digits]
        (
            literal("E"),
            digit1.void(),
            literal("M"),
            digit1.void(),
            eof,
        )
            .void(),
        // MAPxx format: MAP[digits]
        (literal("MAP"), digit1.void(), eof).void(),
    ))
    .parse_next(&mut &*input)
}

#[must_use]
fn is_map_lump(name: LumpName) -> bool {
    let s = name.as_str();
    matches!(
        s,
        "THINGS"
            | "LINEDEFS"
            | "SIDEDEFS"
            | "VERTEXES"
            | "SEGS"
            | "SSECTORS"
            | "NODES"
            | "SECTORS"
            | "REJECT"
            | "BLOCKMAP"
            | "BEHAVIOR"
            | "TEXTMAP"
    )
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
