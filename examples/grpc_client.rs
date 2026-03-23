//! gRPC client example
//!
//! Demonstrates how to connect to the TypeDaemon via gRPC.

use tonic::transport::Channel;
use wootype::api::grpc::proto::{
    type_daemon_client::TypeDaemonClient,
    HealthRequest, ConnectRequest, QueryTypeRequest, TypeByName,
    ValidateRequest, SimilarTypesRequest,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to the daemon
    let mut client = TypeDaemonClient::connect("http://127.0.0.1:50051").await?;

    println!("✅ Connected to Type Daemon\n");

    // Health check
    let health = client.health(HealthRequest {}).await?;
    println!("📊 Health Check:");
    println!("  Version: {}", health.get_ref().version);
    println!("  Healthy: {}", health.get_ref().healthy);
    
    if let Some(stats) = &health.get_ref().stats {
        println!("  Uptime: {}s", stats.uptime_seconds);
        println!("  Requests: {}", stats.requests_processed);
        println!("  Sessions: {}", stats.active_sessions);
        println!("  Types: {}", stats.type_count);
    }

    // Connect as an AI agent
    println!("\n🔌 Connecting as AI Agent...");
    let connect_resp = client.connect(ConnectRequest {
        agent_name: "ExampleClient".to_string(),
        agent_type: "cursor".to_string(),
        isolation_level: "full".to_string(),
        metadata: Default::default(),
    }).await?;

    let session_id = connect_resp.get_ref().session_id.clone();
    println!("  Session ID: {}", session_id);
    println!("  Success: {}", connect_resp.get_ref().success);

    // Query types
    println!("\n🔍 Querying types...");
    let query_resp = client.query_type(QueryTypeRequest {
        session_id: session_id.clone(),
        query: Some(TypeByName {
            package: "main".to_string(),
            name: "int".to_string(),
        }.into()),
    }).await?;

    println!("  Found {} types", query_resp.get_ref().total_count);
    println!("  Latency: {}μs", query_resp.get_ref().latency_micros);

    for typ in &query_resp.get_ref().types {
        println!("    - {} ({})", typ.name, typ.kind);
    }

    // Find similar types
    println!("\n🔍 Finding similar types...");
    let similar_resp = client.find_similar_types(SimilarTypesRequest {
        session_id: session_id.clone(),
        type_id: 1,
        threshold: 0.5,
        max_results: 10,
    }).await?;

    println!("  Found {} similar types", similar_resp.get_ref().total_count);

    // Validate expression
    println!("\n🔍 Validating expression...");
    let validate_resp = client.validate_expression(ValidateRequest {
        session_id: session_id.clone(),
        expression: "x + 1".to_string(),
        file: "main.go".to_string(),
        line: 10,
        column: 5,
        expected_type_id: 0,
    }).await?;

    println!("  Valid: {}", validate_resp.get_ref().valid);
    println!("  Latency: {}μs", validate_resp.get_ref().latency_micros);

    if !validate_resp.get_ref().errors.is_empty() {
        println!("  Errors:");
        for error in &validate_resp.get_ref().errors {
            println!("    - {}", error.message);
        }
    }

    println!("\n✅ Done!");
    Ok(())
}
