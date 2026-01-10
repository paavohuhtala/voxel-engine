// View distance from current chunk, per side
pub const VIEW_DISTANCE: i32 = 64;
pub const LOAD_DISTANCE: i32 = VIEW_DISTANCE + 4;
pub const UNLOAD_DISTANCE: i32 = LOAD_DISTANCE + 2;

// TODO: Make this configurable at runtime
// TODO: Select chunks to render more intelligently based on occlusion and view frustum
