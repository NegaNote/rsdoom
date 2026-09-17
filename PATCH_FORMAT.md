# DOOM Patch Graphics Format

Patches are the fundamental graphics format for all masked (transparent) images in DOOM: sprites, UI elements (TITLEPIC, menus), decorative graphics, etc. The format uses column-based, run-length encoded data to efficiently store images with transparency.

## Binary Structure

### Patch Header (16 bytes + variable, little-endian)

```
Offset  Size  Field
0       2     width           (int16)
2       2     height          (int16)
4       2     leftoffset      (int16) - x offset from origin
6       2     topoffset       (int16) - y offset from origin
8       4*width  columnofs[]  (int32 array) - byte offsets to each column's data
```

All multi-byte integers are **little-endian**.

### Column Format

Each column contains a sequence of **posts** (vertical runs of pixels), terminated by a sentinel byte:

```
Structure per column:
  [POST_1] [POST_2] ... [POST_N] [0xFF]

Each POST (variable length):
  Byte 0:      topdelta     - Y offset from top of patch
  Byte 1:      length       - Number of pixels in this run
  Bytes 2-3:   (padding)    - Two unused bytes (legacy format)
  Bytes 4+:    pixel_data   - 'length' palette indices (byte each)
  
  Next POST starts at: current_post_address + 4 + length
```

## Key Semantics

- **0xFF as topdelta** = end-of-column marker
- **Pixels are palette indices** (0-255), not RGB values; rendering uses the current palette to look up colors
- **Posts may be non-contiguous** within a column, creating transparency "holes"
- **leftoffset/topoffset** position the patch relative to its origin point (typically screen center for sprites)
- **Tall patches** (>255 pixels): DeePsea format allows cumulative topdelta values when a post's topdelta ≤ previous post's topdelta

## Example Parsing

```c
// Read header
patch_t *header = (patch_t*)lumpData;
int width = LittleShort(header->width);
int height = LittleShort(header->height);

// Iterate columns
for (int x = 0; x < width; x++) {
    byte *column = lumpData + LittleLong(header->columnofs[x]);
    
    // Read posts until 0xFF terminator
    while (*column != 0xFF) {
        byte topdelta = column[0];
        byte length = column[1];
        byte *pixels = column + 4;  // Skip padding bytes
        
        // Copy/render 'length' palette indices starting at row 'topdelta'
        
        column += length + 4;  // Advance to next post
    }
}
```

## Common Patch Lumps

- **Sprites**: Located between `S_START`/`S_END` markers
- **UI graphics**: `TITLEPIC`, `HELP1`, `HELP2`, `CREDIT`, `DMENUPIC`, etc.
- **Decorative/masked wall textures**: Patches that compose larger textures via `TEXTURE1`/`TEXTURE2` lumps
- **Menu patches**: UI elements and status bar graphics

The patch format is the foundation of DOOM's sprite and masked graphics system; all masked rendering ultimately decomposes patches into their column/post structure.
