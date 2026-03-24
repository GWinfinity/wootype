//! gRPC client example
//!
//! Demonstrates how to connect to the TypeDaemon via gRPC.
#![allow(unused_imports, unused_variables, clippy::all)]

use wootype::api::grpc::proto::{
    type_daemon_client::TypeDaemonClient, ConnectRequest, HealthRequest, QueryTypeRequest,
    SimilarTypesRequest, TypeByName, ValidateRequest,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // This example is currently disabled due to API changes
    println!("gRPC client example is currently disabled");
    Ok(())
}
