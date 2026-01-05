// View distance from current chunk, per side
// Actual number of loaded chunks is (2 * VIEW_DISTANCE + 1)² in XZ plane
pub const VIEW_DISTANCE: i32 = 16;
// We use a smaller view distance in Y direction to save memory
pub const VIEW_DISTANCE_Y: i32 = 6;

// TODO: Make this configurable at runtime
// TODO: Select chunks to render more intelligently based on occlusion and view frustum
