use grid::{Grid, Order};
use thiserror::Error;
use truncate_integer::TruncateUnchecked;
use usize_conv::ToUsize;
use winnow::Parser;
use winnow::Result;
use winnow::binary::{le_i16, le_u8, le_u16, le_u32};
use winnow::combinator::repeat_till;
use winnow::token::{literal, take};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct PaletteIndex(pub u8);

impl From<u8> for PaletteIndex {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl PaletteColor {
    #[must_use]
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    #[must_use]
    pub const fn from_bytes(bytes: [u8; 3]) -> Self {
        Self {
            red: bytes[0],
            green: bytes[1],
            blue: bytes[2],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("Invalid palette length: {length}")]
pub struct PaletteError {
    pub length: usize,
}

pub struct Palette {
    colors: [PaletteColor; 256],
}

impl Palette {
    #[must_use]
    pub fn get_color(&self, index: PaletteIndex) -> PaletteColor {
        // SAFETY: PaletteIndex uses u8 so it is guaranteed to be max 255
        // and the colors array has a length of 256.
        unsafe { *self.colors.get_unchecked(index.0.to_usize()) }
    }

    /// # Errors
    /// `PaletteError` if the input slice does not have a length of 256 * 3 bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PaletteError> {
        if bytes.len() != 256 * 3 {
            return Err(PaletteError {
                length: bytes.len(),
            });
        }
        let mut palette = Self {
            colors: [PaletteColor::new(0, 0, 0); 256],
        };
        for (i, chunk) in bytes.as_chunks::<3>().0.iter().enumerate() {
            palette.set_color(
                PaletteIndex(i.truncate_unchecked()),
                PaletteColor::from_bytes(*chunk),
            );
        }
        Ok(palette)
    }

    pub fn set_color(&mut self, index: PaletteIndex, color: PaletteColor) {
        // SAFETY: PaletteIndex uses u8 so it is guaranteed to be max 255
        // and the colors array has a length of 256.
        unsafe {
            *self.colors.get_unchecked_mut(index.0.to_usize()) = color;
        }
    }
}

struct PatchHeader {
    left_offset: i16,
    top_offset: i16,
    height: u16,
    column_offsets: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PatchError {
    #[error("Invalid patch header")]
    Header,
    #[error("Invalid column offset: {0}")]
    ColumnOffset(u32),
}

impl PatchHeader {
    fn parser(bytes: &mut &[u8]) -> Result<Self> {
        let width = le_u16.parse_next(bytes)?;
        let height = le_u16.parse_next(bytes)?;
        let left_offset = le_i16.parse_next(bytes)?;
        let top_offset = le_i16.parse_next(bytes)?;

        let mut column_offsets = Vec::with_capacity(width.to_usize());
        for _ in 0..width {
            let offset = le_u32.parse_next(bytes)?;
            column_offsets.push(offset);
        }

        Ok(Self {
            left_offset,
            top_offset,
            height,
            column_offsets,
        })
    }
}

pub struct Post {
    pub top_delta: u8,
    pub pixel_count: u8,
    pub pixels: Vec<PaletteIndex>,
}

impl Post {
    fn parser(bytes: &mut &[u8]) -> Result<Self> {
        let top_delta = le_u8.parse_next(bytes)?;
        let pixel_count = le_u8.parse_next(bytes)?;

        // Ignore padding bytes
        take(2usize).parse_next(bytes)?;

        let mut pixels = Vec::with_capacity(pixel_count.to_usize());
        for _ in 0..pixel_count {
            let pixel = le_u8.parse_next(bytes)?;
            pixels.push(PaletteIndex(pixel));
        }

        Ok(Self {
            top_delta,
            pixel_count,
            pixels,
        })
    }
}

pub struct Column {
    pub posts: Vec<Post>,
}

impl Column {
    fn parser(bytes: &mut &[u8]) -> Result<Self> {
        repeat_till(0.., Post::parser, literal(0xFF))
            .parse_next(bytes)
            .map(|(posts, _)| Self { posts })
    }
}

pub struct Patch {
    pub left_offset: i16,
    pub top_offset: i16,
    pub height: u16,
    // Width is the number of columns
    pub columns: Vec<Column>,
}

impl Patch {
    /// # Errors
    /// Returns `PatchError` if the input bytes are invalid.
    pub fn new(bytes: &[u8]) -> Result<Self, PatchError> {
        let mut input = bytes;

        let header = PatchHeader::parser
            .parse_next(&mut input)
            .map_err(|_| PatchError::Header)?;

        let columns = header
            .column_offsets
            .into_iter()
            .map(|offset| {
                let mut column_bytes = bytes
                    .get(offset.to_usize()..)
                    .ok_or(PatchError::ColumnOffset(offset))?;

                Column::parser
                    .parse_next(&mut column_bytes)
                    .map_err(|_| PatchError::ColumnOffset(offset))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            left_offset: header.left_offset,
            top_offset: header.top_offset,
            height: header.height,
            columns,
        })
    }

    #[must_use]
    pub fn as_pixel_data(&self) -> Grid<Option<PaletteIndex>> {
        let width = self.columns.len();
        let height = self.height.to_usize();
        let mut grid: Grid<Option<PaletteIndex>> =
            Grid::init_with_order(width, height, Order::ColumnMajor, None);

        self.columns.iter().enumerate().for_each(|(x, column)| {
            let mut prev_y_start: Option<usize> = None;
            column.posts.iter().for_each(|post| {
                let top_delta = post.top_delta.to_usize();
                let y_start = match prev_y_start {
                    Some(previous) if top_delta <= previous => previous + top_delta,
                    _ => top_delta,
                };
                prev_y_start = Some(y_start);
                post.pixels.iter().enumerate().for_each(|(i, pixel)| {
                    let y = y_start + i;
                    grid.get_mut(x, y)
                        .into_iter()
                        .for_each(|color| *color = Some(*pixel));
                });
            });
        });

        grid
    }
}
