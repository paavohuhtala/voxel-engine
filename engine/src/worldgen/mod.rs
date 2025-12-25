use crate::voxels::{coord::WorldPos, voxel::Voxel, world::World};

pub fn generate_basic_world() -> World {
    // Generates a flat world sized 64^3 with stone between y=[-32,-1], grass on y=0 and air above (y>0)
    let world = World::new();
    let stone = Voxel::from_type(1);
    let grass = Voxel::from_type(2);

    for x in -32..32 {
        for y in -32..32 {
            for z in -32..32 {
                let pos = WorldPos::new(x, y, z);
                if y < 0 {
                    world.set_voxel(pos, stone);
                } else if y == 0 {
                    world.set_voxel(pos, grass);
                }
            }
        }
    }
    world
}
