use serde_json::json;
use serialporttool_lib::mcp::{
    tool_definitions, ActionResult, ConfigureConnectionRequest, ConnectionKind, DataEncoding,
    PermissionMode, ReadReceivedRequest, SelectProtocolRequest, SendDataRequest, ToolError,
    ToolErrorCode, ToolResult, WaitForDataRequest, MAX_BAUD_RATE, MAX_READ_BYTES, MAX_SEND_BYTES,
    MAX_WAIT_BYTES, MAX_WAIT_TIMEOUT_MS,
};

const EXPECTED_TOOLS: [&str; 17] = [
    "list_ports",
    "get_state",
    "read_received",
    "wait_for_data",
    "get_connection_profiles",
    "configure_connection",
    "connect",
    "disconnect",
    "send_data",
    "clear_received",
    "select_protocol",
    "get_protocol_templates",
    "get_protocol_state",
    "get_frame_statistics",
    "get_graph_state",
    "get_graph_data",
    "clear_graph",
];

#[test]
fn mcp_schema_tool_definitions_have_stable_names_and_required_schema_fields() {
    let tools = tool_definitions();
    let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_str()).collect();
    assert_eq!(names, EXPECTED_TOOLS);

    for tool in tools {
        assert!(!tool.name.is_empty());
        assert!(!tool.title.is_empty());
        assert!(!tool.description.is_empty());
        assert_eq!(tool.input_schema["type"], "object");
        assert!(tool.input_schema.get("properties").is_some());
        assert!(tool.input_schema.get("required").is_some());
        assert!(
            tool.output_schema.is_some(),
            "{} has no outputSchema",
            tool.name
        );
        let serialized = serde_json::to_value(&tool).unwrap();
        assert!(serialized.get("inputSchema").is_some());
        assert!(serialized.get("outputSchema").is_some());
        assert!(serialized.get("annotations").is_some());
        assert!(serialized["annotations"].get("readOnlyHint").is_some());
        assert!(serialized["annotations"].get("destructiveHint").is_some());
        assert!(serialized["annotations"].get("idempotentHint").is_some());
    }
}

#[test]
fn mcp_schema_exposes_bounds_and_encoding_options() {
    let tools = tool_definitions();
    let send = tools.iter().find(|tool| tool.name == "send_data").unwrap();
    assert_eq!(
        send.input_schema["properties"]["encoding"]["enum"],
        json!(["text", "hex", "escape", "base64"])
    );
    assert_eq!(
        send.input_schema["properties"]["data"]["maxLength"],
        json!(MAX_SEND_BYTES * 4)
    );

    let read = tools
        .iter()
        .find(|tool| tool.name == "read_received")
        .unwrap();
    assert_eq!(
        read.input_schema["properties"]["limit"]["maximum"],
        json!(MAX_READ_BYTES)
    );
    assert_eq!(read.input_schema["required"], json!(["cursor", "limit"]));

    let wait = tools
        .iter()
        .find(|tool| tool.name == "wait_for_data")
        .unwrap();
    assert_eq!(
        wait.input_schema["properties"]["timeout_ms"]["maximum"],
        json!(MAX_WAIT_TIMEOUT_MS)
    );
    assert_eq!(
        wait.input_schema["properties"]["max_bytes"]["maximum"],
        json!(MAX_WAIT_BYTES)
    );
}

#[test]
fn mcp_schema_send_data_decodes_supported_encodings_and_enforces_limit() {
    assert_eq!(
        SendDataRequest {
            encoding: DataEncoding::Text,
            data: "hello".into(),
        }
        .validate_and_decode()
        .unwrap(),
        b"hello"
    );
    assert_eq!(
        SendDataRequest {
            encoding: DataEncoding::Hex,
            data: "48 65 6c6c6f".into(),
        }
        .validate_and_decode()
        .unwrap(),
        b"Hello"
    );
    assert_eq!(
        SendDataRequest {
            encoding: DataEncoding::Escape,
            data: r"A\x42\n\\".into(),
        }
        .validate_and_decode()
        .unwrap(),
        b"AB\n\\"
    );
    assert_eq!(
        SendDataRequest {
            encoding: DataEncoding::Base64,
            data: "SGVsbG8=".into(),
        }
        .validate_and_decode()
        .unwrap(),
        b"Hello"
    );

    let too_large = SendDataRequest {
        encoding: DataEncoding::Text,
        data: "x".repeat(MAX_SEND_BYTES + 1),
    };
    assert_eq!(
        too_large.validate_and_decode().unwrap_err().code,
        ToolErrorCode::InvalidParams
    );
    assert_eq!(
        SendDataRequest {
            encoding: DataEncoding::Hex,
            data: "0g".into(),
        }
        .validate_and_decode()
        .unwrap_err()
        .code,
        ToolErrorCode::InvalidParams
    );
}

#[test]
fn mcp_schema_bounded_read_and_wait_requests_reject_zero_and_over_limit() {
    assert!(ReadReceivedRequest {
        cursor: 0,
        limit: 1,
    }
    .validate()
    .is_ok());
    assert!(ReadReceivedRequest {
        cursor: 0,
        limit: 0,
    }
    .validate()
    .is_err());
    assert!(ReadReceivedRequest {
        cursor: 0,
        limit: MAX_READ_BYTES + 1,
    }
    .validate()
    .is_err());

    assert!(WaitForDataRequest {
        timeout_ms: 1,
        max_bytes: 1,
    }
    .validate()
    .is_ok());
    assert!(WaitForDataRequest {
        timeout_ms: MAX_WAIT_TIMEOUT_MS + 1,
        max_bytes: 1,
    }
    .validate()
    .is_err());
    assert!(WaitForDataRequest {
        timeout_ms: 1,
        max_bytes: MAX_WAIT_BYTES + 1,
    }
    .validate()
    .is_err());
}

#[test]
fn mcp_schema_connection_and_protocol_validation_rejects_obviously_invalid_values() {
    let serial = |target: &str, baud_rate: u32| ConfigureConnectionRequest {
        kind: ConnectionKind::Serial,
        target: target.into(),
        port: None,
        baud_rate: Some(baud_rate),
    };
    assert!(serial("/dev/cu.usbserial", 115200).validate().is_ok());
    assert!(serial("", 115200).validate().is_err());
    assert!(serial("/dev/cu.usbserial", 0).validate().is_err());
    assert!(serial("/dev/cu.usbserial", MAX_BAUD_RATE + 1)
        .validate()
        .is_err());

    let tcp = ConfigureConnectionRequest {
        kind: ConnectionKind::Tcp,
        target: "127.0.0.1".into(),
        port: Some(0),
        baud_rate: None,
    };
    assert!(tcp.validate().is_err());

    assert!(SelectProtocolRequest {
        protocol_id: "template-1".into(),
    }
    .validate()
    .is_ok());
    assert!(SelectProtocolRequest {
        protocol_id: "  ".into(),
    }
    .validate()
    .is_err());
}

#[test]
fn mcp_schema_permission_modes_serialize_with_ask_as_default() {
    assert_eq!(PermissionMode::default(), PermissionMode::Ask);
    assert_eq!(
        serde_json::to_string(&PermissionMode::Observe).unwrap(),
        "\"observe\""
    );
    assert_eq!(
        serde_json::to_string(&PermissionMode::Ask).unwrap(),
        "\"ask\""
    );
    assert_eq!(
        serde_json::to_string(&PermissionMode::Full).unwrap(),
        "\"full\""
    );
}

#[test]
fn mcp_schema_annotations_distinguish_read_only_and_destructive_tools() {
    let tools = tool_definitions();
    let list = tools.iter().find(|tool| tool.name == "list_ports").unwrap();
    assert!(list.annotations.read_only_hint);
    assert!(!list.annotations.destructive_hint);
    assert!(list.annotations.idempotent_hint);

    let clear = tools
        .iter()
        .find(|tool| tool.name == "clear_received")
        .unwrap();
    assert!(!clear.annotations.read_only_hint);
    assert!(clear.annotations.destructive_hint);
}

#[test]
fn mcp_schema_tool_errors_are_result_errors_and_structured_results_keep_text_compatibility() {
    let error = ToolError::new(ToolErrorCode::ApprovalRequired, "confirmation is required")
        .with_action_id("act-1")
        .with_details(json!({"mode": "ask"}));
    let error_result = ToolResult::error(&error);
    assert!(error_result.is_error);
    assert_eq!(
        error_result.structured_content["error"]["code"],
        "approval_required"
    );
    assert_eq!(
        error_result.structured_content["error"]["action_id"],
        "act-1"
    );
    assert_eq!(error_result.content[0].content_type, "text");
    assert!(error_result.content[0].text.contains("approval_required"));

    let action = ActionResult {
        action_id: "act-2".into(),
        summary: "sent 3 bytes".into(),
        result: json!({"bytes_sent": 3}),
    };
    let result = ToolResult::success(&action, &action.summary);
    assert!(!result.is_error);
    assert_eq!(result.structured_content["action_id"], "act-2");
    assert_eq!(result.structured_content["result"]["bytes_sent"], 3);
    assert!(result.content[0].text.starts_with("sent 3 bytes\n"));

    for code in [
        ToolErrorCode::InvalidParams,
        ToolErrorCode::NotConnected,
        ToolErrorCode::Busy,
        ToolErrorCode::ApprovalRequired,
        ToolErrorCode::PermissionDenied,
        ToolErrorCode::Timeout,
        ToolErrorCode::TransportError,
    ] {
        assert!(!code.to_string().is_empty());
        assert!(ToolResult::error(&ToolError::new(code, "failure")).is_error);
    }
}
