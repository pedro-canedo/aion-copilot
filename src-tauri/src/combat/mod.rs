pub mod data_storage;
pub mod dps_calculator;
pub mod fight_buffs;
pub mod ping_tracker;
// aion-copilot: what is around you, for the map.
pub mod world_tracker;

// Owns the capture threads and the tokio channel they feed.
#[cfg(feature = "desktop")]
pub mod capture_dispatcher;
