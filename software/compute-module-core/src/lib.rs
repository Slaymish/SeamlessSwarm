pub mod registry;
pub mod scheduler;
pub mod auth;
pub mod server;
pub mod transport;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/seamless_swarm.rs"));
}
