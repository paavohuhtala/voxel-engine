use std::{collections::HashMap, path::Path};

use anyhow::Context;
use glam::{U8Vec4, UVec2};
use image::RgbaImage;
use serde::Deserialize;

use crate::math::rectangle::URectangle;

pub enum FontGlyph {
    Whitespace {
        width: u32,
    },
    Bitmap {
        // This is a terrible format, it'll do for now
        bitmap: Vec<bool>,
        width: u32,
        height: u32,
    },
}

pub struct Font {
    id: String,
    name: String,
    glyphs: FontGlyphs,
    letter_spacing: i32,
}

impl Font {
    pub fn get_glyph(&self, symbol: &str) -> Option<&FontGlyph> {
        self.glyphs.glyphs.get(symbol)
    }

    pub fn get_letter_spacing(&self) -> i32 {
        self.letter_spacing
    }

    pub fn get_line_height(&self) -> u32 {
        self.glyphs.line_height
    }
}

// Only FixedSizeAtlas is implemented for now
pub struct FontGlyphs {
    glyphs: HashMap<String, FontGlyph>,
    image: RgbaImage,
    line_height: u32,
}

#[derive(Debug, Deserialize)]
pub struct GlyphOverride {
    width: u32,
}

#[derive(Debug, Deserialize)]
pub enum FontGlyphsDefinition {
    FixedSizeAtlas {
        grid: (u32, u32),
        symbols: Vec<String>,
        overrides: Option<HashMap<String, GlyphOverride>>,
    },
}

#[derive(Debug, Deserialize)]
pub struct FontDefinition {
    id: String,
    name: String,
    glyphs: FontGlyphsDefinition,
}

pub fn load_font(folder: &Path, font_name: &str) -> anyhow::Result<Font> {
    let definition_path = folder.join(format!("{font_name}.ron"));
    let definition_data =
        std::fs::read_to_string(&definition_path).context("Failed to read font definition file")?;
    let definition: FontDefinition =
        ron::from_str(&definition_data).context("Failed to parse font definition file")?;

    let glyphs = match definition.glyphs {
        FontGlyphsDefinition::FixedSizeAtlas {
            grid,
            symbols,
            overrides,
        } => {
            let image_path = folder.join(format!("{}.png", font_name));
            let texture = image::open(&image_path)
                .with_context(|| {
                    format!(
                        "Failed to open font texture image at {}",
                        image_path.display()
                    )
                })?
                .to_rgba8();

            let mut glyphs = HashMap::new();
            // Every glyph has a fixed max size defined by the atlas grid
            // The actual glyph might be smaller, which we'll determine by checking image data
            // The height is fixed for each row, but the width can vary per glyph
            let grid_cell = UVec2::from(grid);

            let symbol_lines = symbols
                .iter()
                .map(|line| line.chars().collect::<Vec<_>>())
                .collect::<Vec<_>>();

            for (y, line) in symbol_lines.iter().enumerate() {
                for (x, symbol) in line.iter().enumerate() {
                    let origin = grid_cell * UVec2::new(x as u32, y as u32);
                    let rect = URectangle::new(origin, origin + grid_cell);
                    // We've determined where the glyph is contained, but now we need to find the actual x range
                    // Find the starting x by scanning from left to right
                    // And
                    let mut x_start = 0;
                    let mut x_end = grid_cell.x;

                    'outer_start: for px in 0..grid_cell.x {
                        for py in 0..grid_cell.y {
                            let pixel = texture.get_pixel(
                                (rect.origin.x + px) as u32,
                                (rect.origin.y + py) as u32,
                            );
                            if pixel.0[3] != 0 {
                                x_start = px;
                                break 'outer_start;
                            }
                        }
                    }

                    'outer_end: for px in (0..grid_cell.x).rev() {
                        for py in 0..grid_cell.y {
                            let pixel = texture.get_pixel(
                                (rect.origin.x + px) as u32,
                                (rect.origin.y + py) as u32,
                            );
                            if pixel.0[3] != 0 {
                                x_end = px + 1;
                                break 'outer_end;
                            }
                        }
                    }

                    let actual_width = x_end - x_start;

                    // Copy pixel data into bitmap
                    let mut bitmap = Vec::with_capacity((actual_width * grid_cell.y) as usize);
                    for py in 0..grid_cell.y {
                        for px in x_start..x_start + actual_width {
                            let pixel = texture.get_pixel(
                                (rect.origin.x + px) as u32,
                                (rect.origin.y + py) as u32,
                            );
                            bitmap.push(pixel.0[3] != 0);
                        }
                    }
                    glyphs.insert(
                        symbol.to_string(),
                        FontGlyph::Bitmap {
                            bitmap,
                            width: actual_width,
                            height: grid_cell.y,
                        },
                    );
                }
            }

            if let Some(overrides) = overrides {
                // overrides only exists as a workaround for whitespace glyphs for now
                for (symbol, override_data) in overrides {
                    glyphs.insert(
                        symbol,
                        FontGlyph::Whitespace {
                            width: override_data.width,
                        },
                    );
                }
            }

            FontGlyphs {
                glyphs,
                image: texture,
                line_height: grid.1,
            }
        }
    };

    Ok(Font {
        id: definition.id,
        name: definition.name,
        glyphs,
        letter_spacing: 1,
    })
}
