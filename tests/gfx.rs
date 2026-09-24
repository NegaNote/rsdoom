#![allow(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::useless_conversion
)]

use proptest::prelude::*;
use rsdoom::wad::assets::gfx::{PaletteIndex, Patch, PatchError};
use rstest::rstest;

fn patch_bytes(width: u16, height: u16, columns: &[&[u8]], left: i16, top: i16) -> Vec<u8> {
    let header_size = 8 + usize::from(width) * 4;
    let mut bytes = Vec::from(
        [
            width.to_le_bytes(),
            height.to_le_bytes(),
            left.to_le_bytes(),
            top.to_le_bytes(),
        ]
        .concat(),
    );
    let mut offset = header_size;
    for column in columns {
        bytes.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += column.len();
    }
    for column in columns {
        bytes.extend_from_slice(column);
    }
    bytes
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
fn rejects_truncated_headers(#[case] length: usize) {
    assert!(matches!(
        Patch::new(&vec![0; length]),
        Err(PatchError::Header)
    ));
}

#[test]
fn decodes_header_and_zero_width_patch() {
    let patch = Patch::new(&patch_bytes(0, 12, &[], -2, 3)).unwrap();
    assert_eq!(patch.left_offset, -2);
    assert_eq!(patch.top_offset, 3);
    assert_eq!(patch.height, 12);
    assert!(patch.columns.is_empty());
}

#[test]
fn decodes_posts_padding_and_palette_boundaries() {
    let bytes = patch_bytes(1, 4, &[&[0, 2, 0xaa, 0xbb, 0, 255, 0xff]], 0, 0);
    let patch = Patch::new(&bytes).unwrap();
    assert_eq!(patch.columns[0].posts[0].top_delta, 0);
    assert_eq!(
        patch.columns[0].posts[0].pixels,
        vec![PaletteIndex(0), PaletteIndex(255)]
    );
}

#[test]
fn preserves_column_order_and_repeated_offsets() {
    let column = &[0, 1, 0, 0, 42, 0xff][..];
    let mut bytes = patch_bytes(2, 1, &[column, column], 0, 0);
    let offset = (8 + 8) as u32;
    bytes[8..12].copy_from_slice(&offset.to_le_bytes());
    bytes[12..16].copy_from_slice(&offset.to_le_bytes());
    let patch = Patch::new(&bytes).unwrap();
    assert_eq!(patch.columns.len(), 2);
    assert_eq!(
        patch.columns[0].posts[0].pixels,
        patch.columns[1].posts[0].pixels
    );
}

#[test]
fn rejects_bad_offsets_and_unterminated_posts() {
    let mut bytes = patch_bytes(1, 1, &[&[0xff]], 0, 0);
    let invalid_offset = bytes.len() as u32;
    bytes[8..12].copy_from_slice(&invalid_offset.to_le_bytes());
    assert!(matches!(
        Patch::new(&bytes),
        Err(PatchError::ColumnOffset(_))
    ));

    let bytes = patch_bytes(1, 1, &[&[0, 1, 0, 0]], 0, 0);
    assert!(matches!(
        Patch::new(&bytes),
        Err(PatchError::ColumnOffset(_))
    ));
}

#[test]
fn converts_transparency_and_clips_to_height() {
    let bytes = patch_bytes(1, 2, &[&[1, 3, 0, 0, 10, 20, 30, 0xff]], 0, 0);
    let grid = Patch::new(&bytes).unwrap().as_pixel_data();
    assert_eq!(grid.get(0, 0), Some(&None));
    assert_eq!(grid.get(0, 1), Some(&Some(PaletteIndex(10))));
}

proptest! {
    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
        let _ = Patch::new(&bytes);
    }
}
