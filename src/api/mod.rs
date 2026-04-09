pub mod handlers;
pub mod server;

pub mod proto {
    tonic::include_proto!("ai");
}