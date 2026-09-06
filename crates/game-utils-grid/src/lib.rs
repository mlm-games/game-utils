//! Integer grid coordinates, storage, pathfinding, and procgen helpers.
//!
//! No engine types; works for tilemaps, board games, and inventories.

pub mod bit;
pub mod dense;
pub mod path;
pub mod pos;
pub mod procgen;
pub mod sparse;

pub use bit::BitGrid;
pub use dense::DenseGrid;
pub use path::{
    AstarConfig, DiagonalMode, Heuristic, astar, astar_adjacent, bfs_fill, distance_field,
};
pub use pos::{DIRS4, DIRS8, GridPos, GridPos3};
pub use procgen::{Rect, RoomSpec, cellular_step, place_rooms, tunnel_walk};
pub use sparse::SparseGrid;
