//! Structured audit logging and redaction support for TaskFence.
//!
//! This crate writes append-only JSONL audit events and sanitizes secret-like
//! fields before persistence. Audit evidence is intended to feed reports,
//! replay planning, local review, and future team state without scraping
//! terminal output.

use camino::{Utf8Path, Utf8PathBuf};
use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::sync::Mutex;
use taskfence_core::{AuditEvent, AuditLogger, Result, TaskFenceError};

const DEFAULT_MAX_STRING_BYTES: usize = 64 * 1024;
const TRUNCATION_MARKER: &str = "[taskfence truncated]";
const REDACTION_MARKER: &str = "[redacted]";

#[derive(Debug)]
pub struct LocalJsonlAuditLogger {
    path: Utf8PathBuf,
    writer: Mutex<BufWriter<File>>,
    sanitizer: AuditSanitizer,
}

impl LocalJsonlAuditLogger {
    pub fn new(path: impl Into<Utf8PathBuf>) -> Result<Self> {
        Self::with_sanitizer(path, AuditSanitizer::default())
    }

    pub fn with_sanitizer(path: impl Into<Utf8PathBuf>, sanitizer: AuditSanitizer) -> Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(audit_io_error)?;
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path.as_std_path())
            .map_err(audit_io_error)?;

        Ok(Self {
            path,
            writer: Mutex::new(BufWriter::new(file)),
            sanitizer,
        })
    }

    pub fn path(&self) -> &Utf8Path {
        &self.path
    }
}

impl AuditLogger for LocalJsonlAuditLogger {
    fn record(&self, event: AuditEvent) -> Result<()> {
        let mut value = serde_json::to_value(event).map_err(|err| {
            TaskFenceError::Audit(format!("failed to serialize audit event: {err}"))
        })?;
        self.sanitizer.sanitize_json_value(&mut value);

        let mut writer = self
            .writer
            .lock()
            .map_err(|_| TaskFenceError::Audit("audit writer lock poisoned".into()))?;
        serde_json::to_writer(&mut *writer, &value)
            .map_err(|err| TaskFenceError::Audit(format!("failed to write audit json: {err}")))?;
        writer.write_all(b"\n").map_err(audit_io_error)?;
        writer.flush().map_err(audit_io_error)
    }
}

#[derive(Clone, Debug)]
pub struct AuditSanitizer {
    max_string_bytes: usize,
}

impl AuditSanitizer {
    pub fn new(max_string_bytes: usize) -> Self {
        Self { max_string_bytes }
    }

    fn sanitize_json_value(&self, value: &mut Value) {
        match value {
            Value::Object(map) => {
                for (key, child) in map.iter_mut() {
                    if is_secret_key(key) {
                        if is_redacted_value_shape(child) {
                            *child = redacted_value_marker();
                        } else {
                            *child = Value::String(REDACTION_MARKER.into());
                        }
                    } else {
                        self.sanitize_json_value(child);
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    self.sanitize_json_value(item);
                }
            }
            Value::String(text) => {
                *text = sanitize_text(text, self.max_string_bytes);
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }
}

impl Default for AuditSanitizer {
    fn default() -> Self {
        Self {
            max_string_bytes: DEFAULT_MAX_STRING_BYTES,
        }
    }
}

fn audit_io_error(err: std::io::Error) -> TaskFenceError {
    TaskFenceError::Audit(err.to_string())
}

fn is_secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("secret")
        || key.contains("token")
        || key.contains("password")
        || key.contains("api_key")
        || key.contains("apikey")
        || key.contains("authorization")
}

fn is_redacted_value_shape(value: &Value) -> bool {
    matches!(
        value,
        Value::Object(map)
            if map.len() == 1 && (map.contains_key("Plain") || map.contains_key("Redacted"))
    )
}

fn redacted_value_marker() -> Value {
    serde_json::json!({
        "Redacted": {
            "reason": "audit sanitizer redacted secret-like field"
        }
    })
}

fn sanitize_text(input: &str, max_bytes: usize) -> String {
    let cleaned = strip_terminal_controls(input);
    let redacted = redact_secret_like_text(&cleaned);
    truncate_utf8(&redacted, max_bytes)
}

fn strip_terminal_controls(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_control() && !matches!(ch, '\n' | '\r' | '\t') {
                ' '
            } else {
                ch
            }
        })
        .collect()
}

fn redact_secret_like_text(input: &str) -> String {
    let mut output = input.to_owned();
    for prefix in ["sk-", "ghp_", "github_pat_", "xoxb-", "xoxp-"] {
        output = redact_token_prefix(&output, prefix);
    }
    for marker in [
        "token=",
        "token:",
        "password=",
        "password:",
        "secret=",
        "secret:",
        "api_key=",
        "api_key:",
        "authorization=",
        "authorization:",
        "bearer ",
    ] {
        output = redact_after_marker_case_insensitive(&output, marker);
    }
    output
}

fn redact_token_prefix(input: &str, prefix: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;

    while let Some(relative_start) = input[cursor..].find(prefix) {
        let start = cursor + relative_start;
        output.push_str(&input[cursor..start]);
        output.push_str(REDACTION_MARKER);

        let mut end = start + prefix.len();
        for (offset, ch) in input[end..].char_indices() {
            if is_token_char(ch) {
                end = start + prefix.len() + offset + ch.len_utf8();
            } else {
                break;
            }
        }
        cursor = end;
    }

    output.push_str(&input[cursor..]);
    output
}

fn redact_after_marker_case_insensitive(input: &str, marker: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;

    while let Some(relative_start) = lower[cursor..].find(marker) {
        let marker_start = cursor + relative_start;
        let value_start = marker_start + marker.len();
        output.push_str(&input[cursor..value_start]);
        output.push_str(REDACTION_MARKER);

        let mut value_end = value_start;
        for (offset, ch) in input[value_start..].char_indices() {
            if ch.is_whitespace() || matches!(ch, ',' | ';') {
                break;
            }
            value_end = value_start + offset + ch.len_utf8();
        }
        cursor = value_end;
    }

    output.push_str(&input[cursor..]);
    output
}

fn is_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.')
}

fn truncate_utf8(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_owned();
    }

    let mut end = max_bytes;
    while !input.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{}", &input[..end], TRUNCATION_MARKER)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;
    use taskfence_core::{
        AuditEvent, LogChunk, LogStream, RedactedValue, ToolAction, ToolAdapterIdentity,
        ToolRequest,
    };
    use taskfence_testkit::sample_task;
    use time::macros::datetime;

    #[test]
    fn writes_one_json_object_per_line() {
        let temp = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(temp.path().join("events.jsonl")).unwrap();
        let task = sample_task();
        let logger = LocalJsonlAuditLogger::new(path.clone()).unwrap();

        logger
            .record(AuditEvent::TaskCreated {
                task_id: task.id,
                at: datetime!(2024-01-01 00:00 UTC),
                goal: "Run tests".into(),
            })
            .unwrap();

        let contents = fs::read_to_string(path).unwrap();
        let lines = contents.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 1);
        let value: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(value["TaskCreated"]["goal"], "Run tests");
    }

    #[test]
    fn redacts_secret_like_log_text_and_terminal_controls() {
        let temp = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(temp.path().join("events.jsonl")).unwrap();
        let task = sample_task();
        let logger = LocalJsonlAuditLogger::new(path.clone()).unwrap();

        logger
            .record(AuditEvent::Log {
                task_id: task.id,
                chunk: LogChunk {
                    stream: LogStream::Stdout,
                    text: "token=FAKE_SECRET_TOKEN\x1b[31m password=FAKE_PASSWORD".into(),
                    timestamp: datetime!(2024-01-01 00:00 UTC),
                },
            })
            .unwrap();

        let contents = fs::read_to_string(path).unwrap();
        assert!(!contents.contains("FAKE_SECRET_TOKEN"));
        assert!(!contents.contains("hunter2"));
        assert!(!contents.contains('\x1b'));
        assert!(contents.contains(REDACTION_MARKER));
    }

    #[test]
    fn preserves_redacted_value_shape_for_secret_like_tool_parameters() {
        let temp = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(temp.path().join("events.jsonl")).unwrap();
        let task = sample_task();
        let logger = LocalJsonlAuditLogger::new(path.clone()).unwrap();

        logger
            .record(AuditEvent::ToolExecutionStarted {
                task_id: task.id,
                at: datetime!(2024-01-01 00:00 UTC),
                request: ToolRequest {
                    action: ToolAction {
                        protocol: "mcp".into(),
                        tool: "github".into(),
                        operation: "create_pr".into(),
                        parameters: BTreeMap::from([(
                            "authorization".into(),
                            RedactedValue::Plain(
                                "Bearer PROVIDER_SECRET_SHOULD_NOT_SURVIVE".into(),
                            ),
                        )]),
                    },
                    adapter: Some(ToolAdapterIdentity {
                        kind: "local_fixture".into(),
                        name: "github".into(),
                    }),
                },
            })
            .unwrap();

        let contents = fs::read_to_string(path).unwrap();
        assert!(!contents.contains("PROVIDER_SECRET_SHOULD_NOT_SURVIVE"));
        let event: AuditEvent = serde_json::from_str(contents.lines().next().unwrap()).unwrap();
        assert!(matches!(
            event,
            AuditEvent::ToolExecutionStarted { request, .. }
                if matches!(
                    request.action.parameters.get("authorization"),
                    Some(RedactedValue::Redacted { reason })
                        if reason.contains("audit sanitizer")
                )
        ));
    }

    #[test]
    fn truncates_large_strings_without_splitting_utf8() {
        let sanitizer = AuditSanitizer::new(5);
        let mut value = Value::String("abcde中文".into());
        sanitizer.sanitize_json_value(&mut value);

        let text = value.as_str().unwrap();
        assert!(text.starts_with("abcde"));
        assert!(text.ends_with(TRUNCATION_MARKER));
    }
}
