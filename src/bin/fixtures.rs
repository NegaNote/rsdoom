//! Fixture generator for WAD loading and patching tests.
//!
//! This program generates test WAD files covering both valid cases and error conditions.
//! Includes basic valid WADs, maps, namespaces, and various malformed/edge-case WADs.

use rsdoom::wad::builder::WadBuilder;
use rsdoom::wad::raw::WadType;
use std::fs;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

fn to_io_error(_e: rsdoom::wad::builder::LumpNameError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "Invalid lump name")
}

fn main() -> io::Result<()> {
    let fixture_dir = Path::new("tests/test_wads");
    fs::create_dir_all(fixture_dir)?;

    // Valid WADs
    create_valid_empty_iwad(fixture_dir)?;
    create_valid_empty_pwad(fixture_dir)?;
    create_valid_single_lump(fixture_dir)?;
    create_valid_multiple_lumps(fixture_dir)?;
    create_valid_empty_lump(fixture_dir)?;
    create_valid_null_padded_name(fixture_dir)?;

    // Error condition WADs
    create_invalid_magic(fixture_dir)?;
    create_truncated_header(fixture_dir)?;
    create_truncated_directory(fixture_dir)?;
    create_truncated_lump_data(fixture_dir)?;
    create_invalid_lump_name(fixture_dir)?;
    create_malformed_null_padded_name(fixture_dir)?;

    // Lump layout edge cases
    create_lump_overlaps_header(fixture_dir)?;
    create_lump_overlaps_directory(fixture_dir)?;
    create_gapped_layout(fixture_dir)?;
    create_overlapping_lumps(fixture_dir)?;
    create_shared_lump_data(fixture_dir)?;
    create_trailing_bytes(fixture_dir)?;
    create_zero_sized_lump_beyond_eof(fixture_dir)?;

    // Directory edge cases
    create_directory_beyond_eof(fixture_dir)?;
    create_directory_beyond_eof_empty(fixture_dir)?;
    create_duplicate_names(fixture_dir)?;

    // Map-based test fixtures (using builder)
    create_map_override(fixture_dir)?;
    create_new_map(fixture_dir)?;
    create_doom1_map(fixture_dir)?;

    // Namespace/marker test fixtures (using builder)
    create_sprite_override(fixture_dir)?;
    create_multi_namespace(fixture_dir)?;
    create_marked_lumps(fixture_dir)?;

    println!("✓ All {} test WAD fixtures generated", 34);
    Ok(())
}

// ============================================================================
// VALID WAD FILES
// ============================================================================

fn create_valid_empty_iwad(dir: &Path) -> io::Result<()> {
    let builder = WadBuilder::new(WadType::Iwad);
    builder.write_to_file(dir.join("valid_empty_iwad.wad"))
}

fn create_valid_empty_pwad(dir: &Path) -> io::Result<()> {
    let builder = WadBuilder::new(WadType::Pwad);
    builder.write_to_file(dir.join("valid_empty_pwad.wad"))
}

fn create_valid_single_lump(dir: &Path) -> io::Result<()> {
    let mut builder = WadBuilder::new(WadType::Pwad);
    builder
        .add_lump("HELLO", b"HELLO WAD".to_vec())
        .map_err(to_io_error)?;
    builder.write_to_file(dir.join("valid_single_lump.wad"))
}

fn create_valid_multiple_lumps(dir: &Path) -> io::Result<()> {
    let mut builder = WadBuilder::new(WadType::Iwad);
    builder
        .add_lump("LUMPONE", b"ONE".to_vec())
        .map_err(to_io_error)?;
    builder
        .add_lump("LUMPTWO", b"TWO-TWO".to_vec())
        .map_err(to_io_error)?;
    builder
        .add_lump("LUMPTHR", b"THREE".to_vec())
        .map_err(to_io_error)?;
    builder.write_to_file(dir.join("valid_multiple_lumps.wad"))
}

fn create_valid_empty_lump(dir: &Path) -> io::Result<()> {
    let mut builder = WadBuilder::new(WadType::Pwad);
    builder.add_empty_lump("EMPTY").map_err(to_io_error)?;
    builder.write_to_file(dir.join("valid_empty_lump.wad"))
}

fn create_valid_null_padded_name(dir: &Path) -> io::Result<()> {
    let mut builder = WadBuilder::new(WadType::Pwad);
    builder
        .add_lump("PLAYPAL", b"palette".to_vec())
        .map_err(to_io_error)?;
    builder.write_to_file(dir.join("valid_null_padded_name.wad"))
}

// ============================================================================
// ERROR CONDITION WAD FILES
// ============================================================================

fn create_invalid_magic(dir: &Path) -> io::Result<()> {
    // Invalid magic "XXXX" instead of IWAD/PWAD
    let mut file = File::create(dir.join("invalid_magic.wad"))?;
    file.write_all(b"XXXX")?;
    file.write_all(&[0, 0, 0, 0])?; // num_lumps
    file.write_all(&[12, 0, 0, 0])?; // dir_offset
    Ok(())
}

fn create_truncated_header(dir: &Path) -> io::Result<()> {
    // Header missing bytes
    let mut file = File::create(dir.join("truncated_header.wad"))?;
    file.write_all(b"PWAD\x01\x00\x00")?; // Only 7 bytes instead of 12
    Ok(())
}

fn create_truncated_directory(dir: &Path) -> io::Result<()> {
    // Directory extends beyond file end
    let mut file = File::create(dir.join("truncated_directory.wad"))?;
    file.write_all(b"PWAD")?; // "PWAD"
    file.write_all(&1u32.to_le_bytes())?; // 1 lump
    file.write_all(&12u32.to_le_bytes())?; // dir_offset at 12
    // File ends here, directory incomplete
    Ok(())
}

fn create_truncated_lump_data(dir: &Path) -> io::Result<()> {
    // Lump data extends beyond file
    let mut file = File::create(dir.join("truncated_lump_data.wad"))?;
    file.write_all(b"PWAD")?;
    file.write_all(&1u32.to_le_bytes())?; // 1 lump
    file.write_all(&12u32.to_le_bytes())?; // dir_offset
    // Directory at offset 12
    file.write_all(&28u32.to_le_bytes())?; // lump offset (beyond file)
    file.write_all(&6u32.to_le_bytes())?; // lump size
    file.write_all(b"SHORT\0\0\0")?; // lump name
    Ok(())
}

fn create_invalid_lump_name(dir: &Path) -> io::Result<()> {
    // Lump name with non-ASCII byte
    let mut file = File::create(dir.join("invalid_lump_name.wad"))?;
    file.write_all(b"PWAD")?;
    file.write_all(&1u32.to_le_bytes())?; // 1 lump
    file.write_all(&12u32.to_le_bytes())?; // dir_offset
    // Directory
    file.write_all(&12u32.to_le_bytes())?; // lump offset
    file.write_all(&0u32.to_le_bytes())?; // lump size
    file.write_all(b"BAD")?;
    file.write_all(&[0xFF])?; // Non-ASCII byte
    file.write_all(&[0, 0, 0, 0])?; // padding
    Ok(())
}

fn create_malformed_null_padded_name(dir: &Path) -> io::Result<()> {
    // Name with null in middle but non-null after (invalid padding)
    let mut file = File::create(dir.join("malformed_null_padded_name.wad"))?;
    file.write_all(b"PWAD")?;
    file.write_all(&1u32.to_le_bytes())?;
    file.write_all(&12u32.to_le_bytes())?;
    // Directory
    file.write_all(&12u32.to_le_bytes())?;
    file.write_all(&0u32.to_le_bytes())?;
    file.write_all(b"LUMP")?;
    file.write_all(&[0u8])?; // null byte
    file.write_all(&[0u8])?; // null byte
    file.write_all(&[0u8])?; // null byte
    file.write_all(b"X")?; // non-null after nulls (invalid!)
    Ok(())
}

// ============================================================================
// LUMP LAYOUT EDGE CASES
// ============================================================================

fn create_lump_overlaps_header(dir: &Path) -> io::Result<()> {
    // Lump data overlaps with WAD header
    let mut file = File::create(dir.join("lump_overlaps_header.wad"))?;
    file.write_all(b"PWAD")?;
    file.write_all(&1u32.to_le_bytes())?;
    file.write_all(&12u32.to_le_bytes())?;
    // Directory at 12
    file.write_all(&0u32.to_le_bytes())?; // lump offset 0 (overlaps header!)
    file.write_all(&12u32.to_le_bytes())?; // lump size
    file.write_all(b"HEADER\0\0")?; // name
    Ok(())
}

fn create_lump_overlaps_directory(dir: &Path) -> io::Result<()> {
    // Lump data exactly overlaps with directory
    let mut file = File::create(dir.join("lump_overlaps_directory.wad"))?;
    file.write_all(b"PWAD")?;
    file.write_all(&1u32.to_le_bytes())?;
    file.write_all(&12u32.to_le_bytes())?; // dir at 12
    // Directory
    file.write_all(&12u32.to_le_bytes())?; // lump offset = directory offset
    file.write_all(&16u32.to_le_bytes())?; // 16 bytes (size of one directory entry)
    file.write_all(b"DIRDATA\0")?;
    Ok(())
}

fn create_gapped_layout(dir: &Path) -> io::Result<()> {
    // Gap between header and lump data, then gap between lump data and directory
    let mut file = File::create(dir.join("gapped_layout.wad"))?;
    file.write_all(b"PWAD")?;
    file.write_all(&1u32.to_le_bytes())?;
    file.write_all(&28u32.to_le_bytes())?; // dir_offset = 28
    // 8 bytes of gap (offset 12-19)
    file.write_all(&[0u8; 8])?;
    // Lump data at offset 20
    file.write_all(b"GAP")?; // 3 bytes at offset 20-22
    // 5 bytes of gap (offset 23-27) to reach directory offset 28
    file.write_all(&[0u8; 5])?;
    // Directory at offset 28
    file.write_all(&20u32.to_le_bytes())?; // lump offset
    file.write_all(&3u32.to_le_bytes())?; // lump size
    file.write_all(b"GAPPED\0\0")?;
    Ok(())
}

fn create_overlapping_lumps(dir: &Path) -> io::Result<()> {
    // Two lumps with overlapping data
    // FIRST should be "ABCDE" (5 bytes), SECOND should be "DEFGHI" (6 bytes)
    // They overlap at "DE"
    let mut file = File::create(dir.join("overlapping_lumps.wad"))?;
    file.write_all(b"PWAD")?;
    file.write_all(&2u32.to_le_bytes())?; // 2 lumps
    file.write_all(&21u32.to_le_bytes())?; // dir_offset
    // Lump data: "ABCDEFGHI" (9 bytes)
    // FIRST (offset 12, size 5) = bytes 0-4 = "ABCDE"
    // SECOND (offset 15, size 6) = bytes 3-8 = "DEFGHI"
    file.write_all(b"ABCDEFGHI")?;
    // Directory at offset 21
    file.write_all(&12u32.to_le_bytes())?; // FIRST offset
    file.write_all(&5u32.to_le_bytes())?; // FIRST size
    file.write_all(b"FIRST\0\0\0")?;
    file.write_all(&15u32.to_le_bytes())?; // SECOND offset (overlaps!)
    file.write_all(&6u32.to_le_bytes())?; // SECOND size
    file.write_all(b"SECOND\0\0")?;
    Ok(())
}

fn create_shared_lump_data(dir: &Path) -> io::Result<()> {
    // Two lumps pointing to the same data
    let mut file = File::create(dir.join("shared_lump_data.wad"))?;
    file.write_all(b"PWAD")?;
    file.write_all(&2u32.to_le_bytes())?;
    file.write_all(&18u32.to_le_bytes())?;
    // Shared data "SHARED"
    file.write_all(b"SHARED")?;
    // Directory
    file.write_all(&12u32.to_le_bytes())?; // SAMEONE offset
    file.write_all(&6u32.to_le_bytes())?; // SAMEONE size
    file.write_all(b"SAMEONE\0")?;
    file.write_all(&12u32.to_le_bytes())?; // SAMETWO offset (same!)
    file.write_all(&6u32.to_le_bytes())?; // SAMETWO size
    file.write_all(b"SAMETWO\0")?;
    Ok(())
}

fn create_trailing_bytes(dir: &Path) -> io::Result<()> {
    // Valid WAD with extra bytes after directory
    let mut file = File::create(dir.join("trailing_bytes.wad"))?;
    file.write_all(b"PWAD")?;
    file.write_all(&1u32.to_le_bytes())?;
    file.write_all(&16u32.to_le_bytes())?;
    // Lump data
    file.write_all(b"DATA")?;
    // Directory
    file.write_all(&12u32.to_le_bytes())?;
    file.write_all(&4u32.to_le_bytes())?;
    file.write_all(b"DATA\0\0\0\0")?;
    // Trailing bytes
    file.write_all(b"EXTRA")?;
    Ok(())
}

fn create_zero_sized_lump_beyond_eof(dir: &Path) -> io::Result<()> {
    // Zero-sized lump with offset beyond EOF
    let mut file = File::create(dir.join("zero_sized_lump_beyond_eof.wad"))?;
    file.write_all(b"PWAD")?;
    file.write_all(&1u32.to_le_bytes())?;
    file.write_all(&12u32.to_le_bytes())?;
    // Directory at 12
    file.write_all(&256u32.to_le_bytes())?; // offset way beyond EOF
    file.write_all(&0u32.to_le_bytes())?; // size 0
    file.write_all(b"ZERO\0\0\0\0")?;
    Ok(())
}

// ============================================================================
// DIRECTORY EDGE CASES
// ============================================================================

fn create_directory_beyond_eof(dir: &Path) -> io::Result<()> {
    // Directory offset beyond file with lumps
    let mut file = File::create(dir.join("directory_beyond_eof.wad"))?;
    file.write_all(b"PWAD")?;
    file.write_all(&1u32.to_le_bytes())?;
    file.write_all(&1000u32.to_le_bytes())?; // dir way beyond EOF
    // File ends here
    Ok(())
}

fn create_directory_beyond_eof_empty(dir: &Path) -> io::Result<()> {
    // Directory offset beyond file with no lumps
    let mut file = File::create(dir.join("directory_beyond_eof_empty.wad"))?;
    file.write_all(b"PWAD")?;
    file.write_all(&0u32.to_le_bytes())?; // 0 lumps
    file.write_all(&1000u32.to_le_bytes())?; // dir way beyond EOF
    Ok(())
}

fn create_duplicate_names(dir: &Path) -> io::Result<()> {
    // Multiple lumps with identical names, with specific data content
    let mut file = File::create(dir.join("duplicate_names.wad"))?;
    file.write_all(b"PWAD")?;
    file.write_all(&2u32.to_le_bytes())?;
    file.write_all(&23u32.to_le_bytes())?; // dir_offset
    // Lump data: "firstsecond" (11 bytes)
    file.write_all(b"firstsecond")?;
    // Directory at 23
    file.write_all(&12u32.to_le_bytes())?; // first DUPL at 12
    file.write_all(&5u32.to_le_bytes())?; // size 5 = "first"
    file.write_all(b"DUPL\0\0\0\0")?;
    file.write_all(&17u32.to_le_bytes())?; // second DUPL at 17
    file.write_all(&6u32.to_le_bytes())?; // size 6 = "second"
    file.write_all(b"DUPL\0\0\0\0")?;
    Ok(())
}

// ============================================================================
// MAP-BASED TEST FIXTURES (using WadBuilder)
// ============================================================================

fn create_map_override(dir: &Path) -> io::Result<()> {
    // IWAD with two Doom 2 maps
    let mut iwad = WadBuilder::new(WadType::Iwad);
    create_doom2_map(&mut iwad, "MAP01", 0x01)?;
    create_doom2_map(&mut iwad, "MAP02", 0x01)?;
    iwad.write_to_file(dir.join("iwad_two_maps.wad"))?;

    // PWAD that overrides MAP01
    let mut pwad = WadBuilder::new(WadType::Pwad);
    create_doom2_map(&mut pwad, "MAP01", 0x02)?;
    pwad.write_to_file(dir.join("pwad_map_override.wad"))?;

    Ok(())
}

fn create_new_map(dir: &Path) -> io::Result<()> {
    // IWAD with one map
    let mut iwad = WadBuilder::new(WadType::Iwad);
    create_doom2_map(&mut iwad, "MAP01", 0x01)?;
    iwad.write_to_file(dir.join("iwad_one_map.wad"))?;

    // PWAD that adds MAP02
    let mut pwad = WadBuilder::new(WadType::Pwad);
    create_doom2_map(&mut pwad, "MAP02", 0x02)?;
    pwad.write_to_file(dir.join("pwad_new_map.wad"))?;

    Ok(())
}

fn create_doom1_map(dir: &Path) -> io::Result<()> {
    // IWAD with Doom 1 format maps
    let mut iwad = WadBuilder::new(WadType::Iwad);
    create_doom1_map_lump(&mut iwad, "E1M1", 0x01)?;
    create_doom1_map_lump(&mut iwad, "E1M2", 0x01)?;
    iwad.write_to_file(dir.join("iwad_doom1.wad"))?;

    // PWAD that overrides E1M1
    let mut pwad = WadBuilder::new(WadType::Pwad);
    create_doom1_map_lump(&mut pwad, "E1M1", 0x02)?;
    pwad.write_to_file(dir.join("pwad_doom1_override.wad"))?;

    Ok(())
}

// ============================================================================
// NAMESPACE/MARKER TEST FIXTURES (using WadBuilder)
// ============================================================================

fn create_sprite_override(dir: &Path) -> io::Result<()> {
    // IWAD with one sprite
    let mut iwad = WadBuilder::new(WadType::Iwad);
    iwad.add_empty_lump("S_START").map_err(to_io_error)?;
    iwad.add_lump("TROOPY", vec![0x01; 16])
        .map_err(to_io_error)?;
    iwad.add_empty_lump("S_END").map_err(to_io_error)?;
    iwad.write_to_file(dir.join("iwad_sprites.wad"))?;

    // PWAD that overrides the sprite
    let mut pwad = WadBuilder::new(WadType::Pwad);
    pwad.add_empty_lump("S_START").map_err(to_io_error)?;
    pwad.add_lump("TROOPY", vec![0x02; 16])
        .map_err(to_io_error)?;
    pwad.add_empty_lump("S_END").map_err(to_io_error)?;
    pwad.write_to_file(dir.join("pwad_sprite_override.wad"))?;

    Ok(())
}

fn create_multi_namespace(dir: &Path) -> io::Result<()> {
    // IWAD with same-named lumps in different namespaces
    let mut iwad = WadBuilder::new(WadType::Iwad);
    iwad.add_empty_lump("S_START").map_err(to_io_error)?;
    iwad.add_lump("DOOM", vec![0x01; 8]).map_err(to_io_error)?;
    iwad.add_empty_lump("S_END").map_err(to_io_error)?;
    iwad.add_empty_lump("F_START").map_err(to_io_error)?;
    iwad.add_lump("DOOM", vec![0x01; 8]).map_err(to_io_error)?;
    iwad.add_empty_lump("F_END").map_err(to_io_error)?;
    iwad.write_to_file(dir.join("iwad_multi_namespace.wad"))?;

    // PWAD that overrides only the sprite version
    let mut pwad = WadBuilder::new(WadType::Pwad);
    pwad.add_empty_lump("S_START").map_err(to_io_error)?;
    pwad.add_lump("DOOM", vec![0x02; 8]).map_err(to_io_error)?;
    pwad.add_empty_lump("S_END").map_err(to_io_error)?;
    pwad.write_to_file(dir.join("pwad_sprite_same_name.wad"))?;

    Ok(())
}

fn create_marked_lumps(dir: &Path) -> io::Result<()> {
    // IWAD with various marker lumps
    let mut iwad = WadBuilder::new(WadType::Iwad);
    iwad.add_empty_lump("S_START").map_err(to_io_error)?;
    iwad.add_empty_lump("S_END").map_err(to_io_error)?;
    iwad.add_empty_lump("F_START").map_err(to_io_error)?;
    iwad.add_empty_lump("F_END").map_err(to_io_error)?;
    iwad.add_empty_lump("C_START").map_err(to_io_error)?;
    iwad.add_empty_lump("C_END").map_err(to_io_error)?;
    iwad.write_to_file(dir.join("iwad_marked.wad"))?;

    // PWAD with the same markers
    let mut pwad = WadBuilder::new(WadType::Pwad);
    pwad.add_empty_lump("S_START").map_err(to_io_error)?;
    pwad.add_empty_lump("S_END").map_err(to_io_error)?;
    pwad.add_empty_lump("F_START").map_err(to_io_error)?;
    pwad.add_empty_lump("F_END").map_err(to_io_error)?;
    pwad.add_empty_lump("C_START").map_err(to_io_error)?;
    pwad.add_empty_lump("C_END").map_err(to_io_error)?;
    pwad.write_to_file(dir.join("pwad_marked.wad"))?;

    Ok(())
}

// ============================================================================
// HELPER FUNCTIONS FOR MAP CREATION
// ============================================================================

/// Create a Doom 2 format map with marker + 11 lumps
fn create_doom2_map(builder: &mut WadBuilder, map_name: &str, byte_val: u8) -> io::Result<()> {
    builder
        .add_lump(map_name, Vec::new())
        .map_err(to_io_error)?;
    builder
        .add_lump("THINGS", vec![byte_val; 4])
        .map_err(to_io_error)?;
    builder
        .add_lump("LINEDEFS", vec![byte_val; 4])
        .map_err(to_io_error)?;
    builder
        .add_lump("SIDEDEFS", vec![byte_val; 4])
        .map_err(to_io_error)?;
    builder
        .add_lump("VERTEXES", vec![byte_val; 4])
        .map_err(to_io_error)?;
    builder
        .add_lump("SEGS", vec![byte_val; 4])
        .map_err(to_io_error)?;
    builder
        .add_lump("SSECTORS", vec![byte_val; 4])
        .map_err(to_io_error)?;
    builder
        .add_lump("NODES", vec![byte_val; 4])
        .map_err(to_io_error)?;
    builder
        .add_lump("SECTORS", vec![byte_val; 4])
        .map_err(to_io_error)?;
    builder
        .add_lump("REJECT", vec![byte_val; 4])
        .map_err(to_io_error)?;
    builder
        .add_lump("BLOCKMAP", vec![byte_val; 4])
        .map_err(to_io_error)?;
    builder
        .add_lump("BEHAVIOR", vec![byte_val; 4])
        .map_err(to_io_error)?;
    Ok(())
}

/// Create a Doom 1 format map with marker + 11 lumps
fn create_doom1_map_lump(builder: &mut WadBuilder, map_name: &str, byte_val: u8) -> io::Result<()> {
    builder
        .add_lump(map_name, Vec::new())
        .map_err(to_io_error)?;
    builder
        .add_lump("THINGS", vec![byte_val; 4])
        .map_err(to_io_error)?;
    builder
        .add_lump("LINEDEFS", vec![byte_val; 4])
        .map_err(to_io_error)?;
    builder
        .add_lump("SIDEDEFS", vec![byte_val; 4])
        .map_err(to_io_error)?;
    builder
        .add_lump("VERTEXES", vec![byte_val; 4])
        .map_err(to_io_error)?;
    builder
        .add_lump("SEGS", vec![byte_val; 4])
        .map_err(to_io_error)?;
    builder
        .add_lump("SSECTORS", vec![byte_val; 4])
        .map_err(to_io_error)?;
    builder
        .add_lump("NODES", vec![byte_val; 4])
        .map_err(to_io_error)?;
    builder
        .add_lump("SECTORS", vec![byte_val; 4])
        .map_err(to_io_error)?;
    builder
        .add_lump("REJECT", vec![byte_val; 4])
        .map_err(to_io_error)?;
    builder
        .add_lump("BLOCKMAP", vec![byte_val; 4])
        .map_err(to_io_error)?;
    builder
        .add_lump("BEHAVIOR", vec![byte_val; 4])
        .map_err(to_io_error)?;
    Ok(())
}
