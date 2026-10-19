// Parser core — compiled for wasm32 as well as the desktop app.
pub mod abnormal;
pub mod boss_map;
pub mod field_bosses;
pub mod captured_payload;
pub mod counters;
pub mod evidence_slice;
pub mod frame_tap;
pub mod framing;
pub mod packet_accumulator;
// aion-copilot: your pet collection and wild pet creatures, for the pet radar.
pub mod pets;
pub mod stream_assembler;
pub mod stream_processor;
// aion-copilot: positions, for the map.
pub mod world_position;

// Live capture. pcap needs libloading, the port detector reads the wall clock,
// and the file replay drives them both — none of which exist on wasm32.
#[cfg(feature = "desktop")]
pub mod combat_port_detector;
#[cfg(feature = "desktop")]
pub mod file_replay;
#[cfg(feature = "desktop")]
pub mod pcap_capturer;
