use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

use romm_sync_romm::{
    ClientConfig, DeviceAuthorizationRequest, DeviceCredential, LocalSaveObservation, MemoryTrace,
    OperationAction, RomMClient, RomMError, SaveRecord, TokenPoll, UploadOutcome, exact_scope_set,
    md5_hex,
};

struct Step {
    method: &'static str,
    path_contains: &'static str,
    body_contains: Option<&'static str>,
    response: Option<ResponseSpec>,
}

struct ResponseSpec {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

struct ScriptServer {
    origin: String,
    requests: Arc<Mutex<Vec<String>>>,
    handle: JoinHandle<()>,
}

impl ScriptServer {
    fn start(steps: Vec<Step>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind disposable server");
        let address = listener.local_addr().expect("address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&requests);
        let handle = thread::spawn(move || {
            let mut steps = VecDeque::from(steps);
            while let Some(step) = steps.pop_front() {
                let (mut stream, _) = listener.accept().expect("accept request");
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .expect("timeout");
                let request = read_request(&mut stream);
                let request_line = request.lines().next().unwrap_or_default();
                assert!(
                    request_line.starts_with(step.method),
                    "expected {} but received {request_line}",
                    step.method
                );
                assert!(
                    request_line.contains(step.path_contains),
                    "expected path containing {} but received {request_line}",
                    step.path_contains
                );
                if let Some(expected) = step.body_contains {
                    assert!(
                        request.contains(expected),
                        "request body missing {expected:?}"
                    );
                }
                observed.lock().expect("request log").push(request);
                if let Some(response) = step.response {
                    write_response(&mut stream, &response);
                }
            }
        });
        Self {
            origin: format!("http://{address}"),
            requests,
            handle,
        }
    }

    fn finish(self) -> Vec<String> {
        self.handle.join().expect("script server");
        Arc::try_unwrap(self.requests)
            .expect("request observers released")
            .into_inner()
            .expect("request log")
    }
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut expected = None;
    loop {
        let count = stream.read(&mut buffer).expect("read request");
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
        if expected.is_none()
            && let Some(header_end) = find_bytes(&bytes, b"\r\n\r\n")
        {
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("content-length: ")
                        .or_else(|| line.strip_prefix("Content-Length: "))
                })
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            expected = Some(header_end + 4 + content_length);
        }
        if expected.is_some_and(|length| bytes.len() >= length) {
            break;
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn write_response(stream: &mut TcpStream, response: &ResponseSpec) {
    let reason = match response.status {
        200 => "OK",
        201 => "Created",
        409 => "Conflict",
        _ => "Test",
    };
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        reason,
        response.content_type,
        response.body.len()
    )
    .expect("response headers");
    stream.write_all(&response.body).expect("response body");
}

#[allow(clippy::unnecessary_wraps)]
fn json(status: u16, body: &str) -> Option<ResponseSpec> {
    Some(ResponseSpec {
        status,
        content_type: "application/json",
        body: body.as_bytes().to_vec(),
    })
}

fn empty(status: u16) -> ResponseSpec {
    ResponseSpec {
        status,
        content_type: "application/json",
        body: Vec::new(),
    }
}

fn heartbeat(version: &str) -> Step {
    Step {
        method: "GET",
        path_contains: "/api/heartbeat",
        body_contains: None,
        response: json(200, &format!(r#"{{"SYSTEM":{{"VERSION":"{version}"}}}}"#)),
    }
}

fn client(origin: &str) -> RomMClient {
    let config = ClientConfig::new(origin)
        .expect("config")
        .with_insecure_http(true);
    let credential = DeviceCredential::new(
        origin,
        "beelink-test-device",
        exact_scope_set(),
        Some("2099-01-01T00:00:00Z".to_owned()),
        "not-a-real-disposable-test-token",
    )
    .expect("credential");
    RomMClient::authenticated(config, credential).expect("client")
}

#[test]
fn negotiation_uses_md5_autosave_and_sanitized_traces() {
    let server = ScriptServer::start(vec![
        heartbeat("5.4.0"),
        Step {
            method: "POST",
            path_contains: "/api/sync/negotiate",
            body_contains: Some("\"content_hash\":\"900150983cd24fb0d6963f7d28e17f72\""),
            response: json(
                200,
                r#"{"session_id":42,"operations":[{"action":"conflict","rom_id":1234,"save_id":99,"file_name":"mario.sav","slot":"autosave","emulator":"melondsds","reason":"both changed","server_updated_at":"2026-10-14T10:00:00Z","server_content_hash":"e4d909c290d0fb1ca068ffaddf22cbd0"}],"total_upload":0,"total_download":0,"total_conflict":1,"total_no_op":0,"total_delete":0}"#,
            ),
        },
    ]);
    let trace = Arc::new(MemoryTrace::default());
    let api = client(&server.origin).with_trace_sink(trace.clone());
    let response = api
        .negotiate(
            vec![LocalSaveObservation {
                rom_id: 1234,
                file_name: "mario.sav".to_owned(),
                slot: "autosave".to_owned(),
                emulator: Some("melondsds".to_owned()),
                content_hash: md5_hex(b"abc"),
                updated_at: "2026-10-14T09:00:00Z".to_owned(),
                file_size_bytes: 3,
            }],
            Some(vec![1234]),
            Some(vec!["melondsds".to_owned()]),
        )
        .expect("negotiate");
    assert_eq!(response.operations[0].action, OperationAction::Conflict);
    let requests = server.finish();
    assert!(requests[1].contains("authorization: Bearer not-a-real-disposable-test-token"));
    let traces = trace.events();
    assert_eq!(traces.len(), 2);
    let encoded = format!("{traces:?}");
    assert!(!encoded.contains("TOKEN"));
    assert!(!encoded.contains("1234"));
}

#[test]
fn lost_upload_response_reconciles_by_hash_without_duplicate_post() {
    let bytes = b"disposable-save";
    let digest = md5_hex(bytes);
    let save_json = format!(
        r#"[{{"id":77,"rom_id":1234,"file_name":"mario.sav","slot":"autosave","emulator":"melondsds","content_hash":"{digest}","file_size_bytes":{},"updated_at":"2026-10-14T10:00:00Z","device_id":"beelink-test-device"}}]"#,
        bytes.len()
    );
    let server = ScriptServer::start(vec![
        heartbeat("5.4.0"),
        Step {
            method: "GET",
            path_contains: "/api/saves?rom_id=1234",
            body_contains: None,
            response: json(200, "[]"),
        },
        Step {
            method: "POST",
            path_contains: "/api/saves?",
            body_contains: Some("disposable-save"),
            response: None,
        },
        Step {
            method: "GET",
            path_contains: "/api/saves?rom_id=1234",
            body_contains: None,
            response: json(200, &save_json),
        },
    ]);
    let result = client(&server.origin)
        .upload_autosave(
            1234,
            42,
            "autosave",
            "mario.sav",
            Some("melondsds"),
            bytes.to_vec(),
        )
        .expect("lost response should reconcile");
    assert!(matches!(result, UploadOutcome::AlreadyPresent(save) if save.id == 77));
    let requests = server.finish();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.starts_with("POST /api/saves?"))
            .count(),
        1
    );
    assert!(requests[2].contains("overwrite=false"));
    assert!(requests[2].contains("emulator=melondsds"));
    assert!(!requests.iter().any(|request| request.starts_with("PUT ")));
}

#[test]
fn qualification_response_loss_reconciles_the_live_success_without_duplicate_post() {
    let bytes = b"disposable-save";
    let digest = md5_hex(bytes);
    let save = format!(
        r#"{{"id":77,"rom_id":1234,"file_name":"mario.sav","slot":"autosave","emulator":"melondsds","content_hash":"{digest}","file_size_bytes":{},"updated_at":"2026-10-14T10:00:00Z","device_id":"beelink-test-device"}}"#,
        bytes.len()
    );
    let saves = format!("[{save}]");
    let server = ScriptServer::start(vec![
        heartbeat("5.4.0"),
        Step {
            method: "GET",
            path_contains: "/api/saves?rom_id=1234",
            body_contains: None,
            response: json(200, "[]"),
        },
        Step {
            method: "POST",
            path_contains: "/api/saves?",
            body_contains: Some("disposable-save"),
            response: json(200, &save),
        },
        Step {
            method: "GET",
            path_contains: "/api/saves?rom_id=1234",
            body_contains: None,
            response: json(200, &saves),
        },
    ]);

    let outcome = client(&server.origin)
        .upload_autosave_with_response_loss(
            1234,
            42,
            "autosave",
            "mario.sav",
            Some("melondsds"),
            bytes.to_vec(),
        )
        .expect("discarded success should reconcile by visible head hash");

    assert!(matches!(outcome, UploadOutcome::AlreadyPresent(save) if save.id == 77));
    let requests = server.finish();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.starts_with("POST /api/saves?"))
            .count(),
        1
    );
}

#[test]
fn deleting_the_bound_device_revokes_its_credential() {
    let server = ScriptServer::start(vec![
        heartbeat("5.4.0"),
        Step {
            method: "DELETE",
            path_contains: "/api/devices/beelink-test-device",
            body_contains: None,
            response: Some(empty(204)),
        },
        Step {
            method: "GET",
            path_contains: "/api/saves?rom_id=1234",
            body_contains: None,
            response: json(401, r#"{"detail":"Unauthorized"}"#),
        },
    ]);
    let client = client(&server.origin);

    client
        .revoke_bound_device()
        .expect("bound device deletion should succeed");
    let error = client
        .list_saves(1234)
        .expect_err("the deleted device token must be rejected");

    assert!(matches!(error, RomMError::Api { status: 401, .. }));
    server.finish();
}

#[test]
fn guarded_conflict_never_falls_back_to_overwrite() {
    let changed = r#"[{"id":88,"rom_id":1234,"file_name":"mario.sav","slot":"autosave","content_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","file_size_bytes":8,"updated_at":"2026-10-14T11:00:00Z"}]"#;
    let server = ScriptServer::start(vec![
        heartbeat("5.4.0"),
        Step {
            method: "GET",
            path_contains: "/api/saves?rom_id=1234",
            body_contains: None,
            response: json(200, changed),
        },
        Step {
            method: "POST",
            path_contains: "/api/saves?",
            body_contains: None,
            response: json(409, r#"{"error":"slot_changed"}"#),
        },
        Step {
            method: "GET",
            path_contains: "/api/saves?rom_id=1234",
            body_contains: None,
            response: json(200, changed),
        },
    ]);
    let error = client(&server.origin)
        .upload_autosave(
            1234,
            42,
            "autosave",
            "mario.sav",
            Some("melondsds"),
            b"local".to_vec(),
        )
        .expect_err("different remote head must remain a conflict");
    assert!(matches!(error, RomMError::Conflict));
    let requests = server.finish();
    assert!(!requests.iter().any(|request| request.starts_with("PUT ")));
}

#[test]
fn completed_upload_retry_is_a_read_only_no_op() {
    let bytes = b"already-uploaded";
    let digest = md5_hex(bytes);
    let saves = format!(
        r#"[{{"id":91,"rom_id":1234,"file_name":"mario.sav","slot":"autosave","content_hash":"{digest}","file_size_bytes":{},"updated_at":"2026-10-14T11:00:00Z"}}]"#,
        bytes.len()
    );
    let server = ScriptServer::start(vec![
        heartbeat("5.4.0"),
        Step {
            method: "GET",
            path_contains: "/api/saves?rom_id=1234",
            body_contains: None,
            response: json(200, &saves),
        },
    ]);
    let outcome = client(&server.origin)
        .upload_autosave(1234, 42, "autosave", "mario.sav", None, bytes.to_vec())
        .expect("identical head is already complete");
    assert!(matches!(outcome, UploadOutcome::AlreadyPresent(save) if save.id == 91));
    let requests = server.finish();
    assert_eq!(requests.len(), 2);
    assert!(
        !requests
            .iter()
            .any(|request| request.starts_with("POST /api/saves?"))
    );
}

#[test]
fn save_listing_rejects_rows_for_other_roms() {
    let server = ScriptServer::start(vec![Step {
        method: "GET",
        path_contains: "/api/saves?rom_id=1234",
        body_contains: None,
        response: json(
            200,
            r#"[{"id":91,"rom_id":1234,"file_name":"mario.sav","slot":"autosave","content_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","updated_at":"2026-10-14T11:00:00Z"},{"id":92,"rom_id":9999,"file_name":"other.sav","slot":"autosave","content_hash":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","updated_at":"2026-10-14T12:00:00Z"}]"#,
        ),
    }]);

    let saves = client(&server.origin).list_saves(1234).expect("save list");

    assert_eq!(saves.len(), 1);
    assert_eq!(saves[0].rom_id, 1234);
    server.finish();
}

#[test]
fn download_is_unconfirmed_until_explicit_post() {
    let bytes = b"remote-save".to_vec();
    let digest = md5_hex(&bytes);
    let server = ScriptServer::start(vec![
        heartbeat("5.4.0"),
        Step {
            method: "GET",
            path_contains: "/api/saves/99/content?device_id=beelink-test-device&session_id=42&optimistic=false",
            body_contains: None,
            response: Some(ResponseSpec {
                status: 200,
                content_type: "application/octet-stream",
                body: bytes.clone(),
            }),
        },
        heartbeat("5.4.0"),
        Step {
            method: "POST",
            path_contains: "/api/saves/99/downloaded",
            body_contains: Some("\"device_id\":\"beelink-test-device\""),
            response: json(200, r"{}"),
        },
    ]);
    let api = client(&server.origin);
    let verified = api
        .download_unconfirmed(
            SaveRecord {
                id: 99,
                rom_id: 1234,
                file_name: "mario.sav".to_owned(),
                slot: Some("autosave".to_owned()),
                emulator: Some("melondsds".to_owned()),
                content_hash: Some(digest.clone()),
                file_size: Some(bytes.len() as u64),
                updated_at: Some("2026-10-14T10:00:00Z".to_owned()),
                device_id: Some("source-device".to_owned()),
                origin_device: None,
            },
            42,
        )
        .expect("verified download");
    assert_eq!(verified.bytes, bytes);
    api.confirm_download(99, &digest)
        .expect("explicit confirmation");
    let requests = server.finish();
    assert!(requests[1].contains("optimistic=false"));
    assert!(requests[3].contains(&format!("\"content_hash\":\"{digest}\"")));
}

#[test]
fn version_drift_blocks_before_negotiation() {
    let server = ScriptServer::start(vec![heartbeat("5.5.0")]);
    let error = client(&server.origin)
        .negotiate(Vec::new(), None, None)
        .expect_err("unsupported version must fail closed");
    assert!(matches!(error, RomMError::UnsupportedVersion { .. }));
    let requests = server.finish();
    assert_eq!(requests.len(), 1);
}

#[test]
fn device_authorization_returns_a_bound_exact_scope_credential() {
    let granted = exact_scope_set()
        .into_iter()
        .map(|scope| format!("\"{scope}\""))
        .collect::<Vec<_>>()
        .join(",");
    let server = ScriptServer::start(vec![
        heartbeat("5.4.0"),
        Step {
            method: "POST",
            path_contains: "/api/auth/device/init",
            body_contains: Some("\"devices.write\""),
            response: json(
                201,
                r#"{"device_code":"long-code","user_code":"1234-5678","verification_path":"/pair/device","verification_path_complete":"/pair/device?user_code=1234-5678","expires_in":600,"interval":5}"#,
            ),
        },
        Step {
            method: "POST",
            path_contains: "/api/auth/device/token",
            body_contains: Some("\"device_code\":\"long-code\""),
            response: json(
                200,
                &format!(
                    r#"{{"access_token":"not-a-real-bound-token","device_id":"beelink-device","scopes":[{granted}],"expires_at":"2099-01-01T00:00:00Z"}}"#
                ),
            ),
        },
    ]);
    let config = ClientConfig::new(&server.origin)
        .expect("config")
        .with_insecure_http(true);
    let api = RomMClient::for_pairing(config).expect("pairing client");
    let authorization = api
        .begin_device_authorization(DeviceAuthorizationRequest {
            client_device_identifier: "beelink-device".to_owned(),
            name: "Beelink".to_owned(),
            client: "romm-autosync".to_owned(),
            client_version: "0.1.0".to_owned(),
            platform: "bazzite".to_owned(),
            requested_scopes: vec!["must-be-replaced".to_owned()],
        })
        .expect("authorization");
    assert_eq!(authorization.interval, 5);
    let TokenPoll::Authorized(credential) = api
        .poll_device_token(&authorization.device_code)
        .expect("token poll")
    else {
        panic!("authorization should be complete");
    };
    assert_eq!(credential.device_id, "beelink-device");
    assert_eq!(credential.granted_scopes, exact_scope_set());
    let requests = server.finish();
    let body = requests[1]
        .split_once("\r\n\r\n")
        .expect("request contains header separator")
        .1;
    let payload: serde_json::Value = serde_json::from_str(body).expect("valid JSON request");
    assert_eq!(
        payload["client_device_identifier"],
        serde_json::json!("beelink-device")
    );
    assert_eq!(payload["name"], serde_json::json!("Beelink"));
    assert_eq!(payload["client"], serde_json::json!("romm-autosync"));
    assert_eq!(payload["client_version"], serde_json::json!("0.1.0"));
    assert_eq!(payload["platform"], serde_json::json!("bazzite"));
    assert_eq!(
        payload["requested_scopes"],
        serde_json::json!([
            "assets.read",
            "assets.write",
            "devices.read",
            "devices.write",
            "platforms.read",
            "roms.read"
        ])
    );
    assert_eq!(payload.as_object().expect("JSON object").len(), 6);
    assert!(payload.get("device_id").is_none());
    assert!(payload.get("device_name").is_none());
    assert!(payload.get("client_name").is_none());
    assert!(payload.get("scopes").is_none());
    assert!(!body.contains("must-be-replaced"));
}

#[test]
fn session_completion_reports_explicit_counts() {
    let server = ScriptServer::start(vec![
        heartbeat("5.4.0"),
        Step {
            method: "POST",
            path_contains: "/api/sync/sessions/42/complete",
            body_contains: Some("\"operations_completed\":3"),
            response: json(200, r#"{"session":{},"play_session_ingest":null}"#),
        },
    ]);
    client(&server.origin)
        .complete_session(42, 3, 1)
        .expect("complete session");
    let requests = server.finish();
    assert!(requests[1].contains("\"operations_failed\":1"));
}
