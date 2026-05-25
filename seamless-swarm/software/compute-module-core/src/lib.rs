pub mod registry;
pub mod scheduler;
pub mod auth;

pub mod proto {
    prost::include_proto!("seamless_swarm");
}
