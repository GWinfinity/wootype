//! WebSocket client example
//!
//! Demonstrates how to connect to the TypeDaemon via WebSocket.
#![allow(dead_code, unused_variables, clippy::single_match)]

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Debug, Serialize)]
struct WsRequest {
    method: String,
    #[serde(rename = "params")]
    params: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct WsResponse {
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<WsError>,
}

#[derive(Debug, Deserialize)]
struct WsError {
    code: i32,
    message: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to WebSocket
    let url = "ws://127.0.0.1:8080/ws";
    println!("🔌 Connecting to {}", url);

    let (ws_stream, _) = connect_async(url).await?;
    let (mut write, mut read) = ws_stream.split();

    println!("✅ Connected to Type Daemon via WebSocket\n");

    // Health check
    let health_request = WsRequest {
        method: "health".to_string(),
        params: serde_json::json!({}),
    };

    send_request(&mut write, &health_request).await?;
    receive_response(&mut read, "health").await?;

    // Connect as agent
    println!("\n🔌 Connecting as AI Agent...");
    let connect_request = WsRequest {
        method: "connect".to_string(),
        params: serde_json::json!({
            "agentName": "WebSocketExample",
            "agentType": "cursor"
        }),
    };

    send_request(&mut write, &connect_request).await?;
    let connect_result = receive_response(&mut read, "connect").await?;

    let session_id = connect_result
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    println!("  Session ID: {}", session_id);

    // Query types
    println!("\n🔍 Querying types...");
    let query_request = WsRequest {
        method: "query".to_string(),
        params: serde_json::json!({
            "sessionId": session_id,
            "type": "byName",
            "package": "main",
            "name": "int"
        }),
    };

    send_request(&mut write, &query_request).await?;
    receive_response(&mut read, "query").await?;

    // Validate expression
    println!("\n🔍 Validating expression...");
    let validate_request = WsRequest {
        method: "validate".to_string(),
        params: serde_json::json!({
            "sessionId": session_id,
            "expression": "x + 1"
        }),
    };

    send_request(&mut write, &validate_request).await?;
    receive_response(&mut read, "validate").await?;

    // Stream validation
    println!("\n🔍 Stream validation...");
    let tokens = vec!["fmt", ".", "Println", "(", "\"Hello\"", ")"];

    for token in tokens {
        let stream_request = WsRequest {
            method: "streamValidate".to_string(),
            params: serde_json::json!({
                "sessionId": session_id,
                "token": token,
                "isComplete": token == ")"
            }),
        };

        send_request(&mut write, &stream_request).await?;
        receive_response(&mut read, "streamValidate").await?;
    }

    // Import package
    println!("\n📦 Importing package...");
    let import_request = WsRequest {
        method: "import".to_string(),
        params: serde_json::json!({
            "sessionId": session_id,
            "packagePath": "fmt"
        }),
    };

    send_request(&mut write, &import_request).await?;
    receive_response(&mut read, "import").await?;

    // Close connection
    write.close().await?;
    println!("\n✅ Connection closed");

    Ok(())
}

async fn send_request(
    write: &mut futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    request: &WsRequest,
) -> Result<(), Box<dyn std::error::Error>> {
    let msg = Message::Text(serde_json::to_string(request)?);
    write.send(msg).await?;
    Ok(())
}

async fn receive_response(
    read: &mut futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    expected_method: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    if let Some(msg) = read.next().await {
        match msg? {
            Message::Text(text) => {
                let response: WsResponse = serde_json::from_str(&text)?;

                if let Some(error) = response.error {
                    println!("  Error: {}", error.message);
                    return Err(error.message.into());
                }

                if let Some(result) = response.result {
                    println!("  Result: {}", serde_json::to_string_pretty(&result)?);
                    return Ok(result);
                }
            }
            _ => {}
        }
    }

    Err("No response received".into())
}
