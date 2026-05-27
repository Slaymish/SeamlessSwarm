pub mod election;
pub mod scout;
pub mod transport;
pub mod secure_element;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/seamless_swarm.rs"));
}
