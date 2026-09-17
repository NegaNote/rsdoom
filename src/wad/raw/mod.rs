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
    pub fn get_lump_by_str_name_and_namespace(&self, name: &str, namespace: Namespace) -> Option<&Lump> {
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
pub fn load_wad(path: PathBuf, wad_type: WadType) -> Result<WadView, WadLoadingError> {
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
            source_type: wad_type,
            namespace: Namespace::Global, // Default namespace, updated later based on markers
        });
    }

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
    // Phase 1: Handle non-map lumps by name+namespace
    for patch_lump in &patch_wad.lumps {
        // Skip map-related lumps; handle them in phase 2
        if is_map_lump(patch_lump.name) || is_map_marker(patch_lump.name) {
            continue;
        }

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
    }

    // Phase 2: Handle map lumps (position-aware)
    // For each map marker in patch_wad, find the corresponding map in wad
    // and replace its sequential lumps (the 11 lumps following the map marker)
    for (patch_map_idx, patch_lump) in patch_wad.lumps.iter().enumerate() {
        if !is_map_marker(patch_lump.name) {
            continue;
        }

        // Find the same map in wad
        if let Some(wad_map_idx) = wad.lumps.iter().position(|l| l.name == patch_lump.name) {
            // Replace the next 11 lumps (THINGS through BEHAVIOR)
            for offset in 1..=11 {
                if let Some(patch_lump_to_add) = patch_wad.lumps.get(patch_map_idx + offset) {
                    if let Some(existing) = wad.lumps.get_mut(wad_map_idx + offset) {
                        // Replace existing lump
                        *existing = patch_lump_to_add.clone();
                    } else {
                        // Append if wad doesn't have that slot
                        wad.lumps.push(patch_lump_to_add.clone());
                    }
                }
            }
        } else {
            // Map doesn't exist in wad; add the entire map with its 11 lumps
            for offset in 0..=11 {
                if let Some(patch_lump_to_add) = patch_wad.lumps.get(patch_map_idx + offset) {
                    wad.lumps.push(patch_lump_to_add.clone());
                }
            }
        }
    }
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
