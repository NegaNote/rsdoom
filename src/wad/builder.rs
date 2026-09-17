use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::str::FromStr;

pub use crate::wad::raw::LumpNameError;
use crate::wad::raw::{LumpName, WadType};

/// A builder for constructing WAD files programmatically.
///
/// This allows end users to create WAD files in-memory and serialize them to disk
/// using the same binary format that the WAD loader expects.
#[derive(Debug, Clone)]
pub struct WadBuilder {
    wad_type: WadType,
    lumps: Vec<LumpEntry>,
}

#[derive(Debug, Clone)]
struct LumpEntry {
    name: LumpName,
    data: Vec<u8>,
}

impl WadBuilder {
    /// Create a new WAD builder for the given WAD type (IWAD or PWAD).
    #[must_use]
    pub const fn new(wad_type: WadType) -> Self {
        Self {
            wad_type,
            lumps: Vec::new(),
        }
    }

    /// Add a lump with raw data to the WAD.
    ///
    /// # Errors
    /// Returns an error if the lump name is invalid (not ASCII or too long).
    pub fn add_lump(&mut self, name: &str, data: Vec<u8>) -> Result<(), LumpNameError> {
        let lump_name = LumpName::from_str(name)?;
        self.lumps.push(LumpEntry {
            name: lump_name,
            data,
        });
        Ok(())
    }

    /// Add a lump with empty data (useful for markers like `S_START`, `S_END`).
    ///
    /// # Errors
    /// Returns an error if the lump name is invalid (not ASCII or too long).
    pub fn add_empty_lump(&mut self, name: &str) -> Result<(), LumpNameError> {
        self.add_lump(name, Vec::new())
    }

    /// Serialize the WAD to a file.
    ///
    /// # Errors
    /// Returns an error if the file cannot be written, if the WAD structure exceeds u32 bounds,
    /// or if the data layout is invalid.
    pub fn write_to_file<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let mut file = File::create(path)?;
        self.write_to(&mut file)
    }

    /// Serialize the WAD to any writer.
    ///
    /// # Errors
    /// Returns an error if writing fails or the WAD structure exceeds u32 bounds.
    pub fn write_to<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let header = self.build_header()?;
        let (lump_data, lump_infos) = self.build_lumps_and_infos()?;

        // Write header
        writer.write_all(&header)?;

        // Write lump data
        writer.write_all(&lump_data)?;

        // Write lump info table
        for info in lump_infos {
            writer.write_all(&info)?;
        }

        Ok(())
    }

    /// Get the number of lumps in this WAD.
    #[must_use]
    pub const fn lump_count(&self) -> usize {
        self.lumps.len()
    }

    /// Get a reference to a lump by name (first match).
    #[must_use]
    pub fn get_lump(&self, name: &str) -> Option<&[u8]> {
        self.lumps
            .iter()
            .find(|l| l.name.as_str() == name)
            .map(|l| l.data.as_slice())
    }

    fn build_header(&self) -> std::io::Result<Vec<u8>> {
        let mut header = Vec::with_capacity(12);

        // WAD type (4 bytes)
        let type_str = match self.wad_type {
            WadType::Iwad => "IWAD",
            WadType::Pwad => "PWAD",
        };
        header.extend_from_slice(type_str.as_bytes());

        // Number of lumps (4 bytes, little-endian u32)
        let num_lumps = u32::try_from(self.lumps.len())
            .map_err(|_| std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "WAD file has too many lumps",
            ))?;
        header.extend_from_slice(&num_lumps.to_le_bytes());

        // Offset to info table (4 bytes, little-endian u32)
        // Info table comes after all lump data
        let info_table_offset = u32::try_from(12 + self.get_total_lump_data_size())
            .map_err(|_| std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "WAD file is too large",
            ))?;
        header.extend_from_slice(&info_table_offset.to_le_bytes());

        Ok(header)
    }

    fn build_lumps_and_infos(&self) -> std::io::Result<(Vec<u8>, Vec<Vec<u8>>)> {
        let mut lump_data = Vec::new();
        let mut lump_infos = Vec::new();
        let mut current_offset = 12; // Start after header

        for entry in &self.lumps {
            // Record where this lump's data starts
            let offset = u32::try_from(current_offset)
                .map_err(|_| std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Lump offset exceeds u32 bounds",
                ))?;
            let size = u32::try_from(entry.data.len())
                .map_err(|_| std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Lump size exceeds u32 bounds",
                ))?;

            // Add the lump data
            lump_data.extend_from_slice(&entry.data);
            current_offset += entry.data.len();

            // Build the lump info entry (16 bytes)
            let mut info = Vec::with_capacity(16);
            info.extend_from_slice(&offset.to_le_bytes()); // 4 bytes
            info.extend_from_slice(&size.to_le_bytes()); // 4 bytes
            info.extend_from_slice(entry.name.as_bytes()); // 8 bytes
            lump_infos.push(info);
        }

        Ok((lump_data, lump_infos))
    }

    fn get_total_lump_data_size(&self) -> usize {
        self.lumps.iter().map(|l| l.data.len()).sum()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_build_simple_iwad() {
        let mut builder = WadBuilder::new(WadType::Iwad);
        builder.add_lump("TEST", vec![1, 2, 3]).unwrap();
        builder.add_empty_lump("END").unwrap();

        assert_eq!(builder.lump_count(), 2);
        assert_eq!(builder.get_lump("TEST"), Some(&[1, 2, 3][..]));
    }

    #[test]
    fn test_build_pwad() {
        let mut builder = WadBuilder::new(WadType::Pwad);
        builder.add_lump("DOOM", vec![0x42]).unwrap();
        builder.add_lump("HELLO", vec![]).unwrap();

        assert_eq!(builder.lump_count(), 2);
        assert_eq!(builder.get_lump("DOOM"), Some(&[0x42][..]));
    }

    #[test]
    fn test_invalid_lump_name() {
        let mut builder = WadBuilder::new(WadType::Iwad);
        assert!(builder.add_lump("TOOLONGNAME", vec![]).is_err());
        assert!(builder.add_lump("TES\u{FF}T", vec![]).is_err());
    }
}
