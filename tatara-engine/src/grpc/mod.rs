#[cfg(feature = "grpc")]
pub mod external;
#[cfg(feature = "grpc")]
pub mod internal;

#[cfg(feature = "grpc")]
pub mod proto {
    tonic::include_proto!("tatara.v1");
}
