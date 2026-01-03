use crate::{
    assets::fonts::{Font, FontGlyph},
    voxels::{coord::WorldPos, voxel::Voxel},
    world::World,
};

const EMPTY_VOXEL: Voxel = Voxel::AIR;
const FILLED_VOXEL: Voxel = Voxel::GOLD;

pub fn draw_text(world: &World, origin: WorldPos, font: &Font, text: &str) {
    let mut cursor = origin;

    for ch in text.chars() {
        if let Some(glyph) = font.get_glyph(&ch.to_string()) {
            match glyph {
                FontGlyph::Whitespace { width } => {
                    cursor.0.x += font.get_letter_spacing() + *width as i32;
                }
                FontGlyph::Bitmap {
                    bitmap,
                    width,
                    height,
                } => {
                    let width = *width;
                    let height = *height;

                    for y in 0..height {
                        for x in 0..width {
                            let bitmap_index = (y * width + x) as usize;
                            let pixel_filled = bitmap[bitmap_index];
                            let voxel = if pixel_filled {
                                FILLED_VOXEL
                            } else {
                                EMPTY_VOXEL
                            };

                            // The glyphs are stored top to bottom, so we need to invert the y coordinate
                            let target_y = height as i32 - 1 - y as i32;

                            let voxel_pos =
                                WorldPos(cursor.0 + glam::IVec3::new(x as i32, target_y, 0));
                            world.set_voxel(voxel_pos, voxel);
                        }
                    }

                    cursor.0.x += font.get_letter_spacing() + width as i32;
                }
            }
        } else {
            log::warn!("Glyph not found for character: {} ({})", ch, ch as u32);
        }
    }
}
