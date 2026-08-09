//! Claude Code adapter. Rides a Claude Pro/Max subscription.
//!
//! Schema verified against Claude Code 2.1.220 by capturing a live
//! `--output-format stream-json` run.

use super::creds::Auth;
use super::persona::persona;
use super::{summarize, Adapter, BridgeEvent, TurnRequest};
use serde_json::Value;
use tokio::process::Command;

#[derive(Default)]
pub struct ClaudeAdapter {
    /// Guards against the process exiting after we already closed the turn.
    finished: bool,
}

impl Adapter for ClaudeAdapter {
    fn command(&self, program: &str, req: &TurnRequest, auth: Auth) -> Command {
        let mut cmd = Command::new(program);
        cmd.arg("-p")
            .arg(&req.prompt)
            .arg("--output-format")
            .arg("stream-json")
            // stream-json is rejected in print mode without --verbose.
            .arg("--verbose");

        // Isolation. A user's dev setup can carry dozens of MCP servers and
        // project hooks; the cat should not silently inherit any of it.
        //
        // Which flags get there depends on how the turn is being paid for, and
        // the two cases are mutually exclusive. Per `claude --help` on 2.1.220,
        // --bare makes Anthropic auth "strictly ANTHROPIC_API_KEY or
        // apiKeyHelper... OAuth and keychain are never read":
        //
        // * On a subscription that is exactly wrong — it would ignore the login
        //   the whole bridge is built on.
        // * With the user's own key it is the only way to be *sure* the key is
        //   what gets spent. Without it a leftover OAuth login can win, and the
        //   user would be billed for a subscription they thought they'd
        //   stopped using, with nothing on screen to say so.
        //
        // --bare also skips hooks, plugins and CLAUDE.md discovery, which is
        // the same isolation the flags below buy — so it replaces them rather
        // than joining them.
        match auth {
            Auth::Key => {
                cmd.arg("--bare");
            }
            Auth::Subscription | Auth::Inherit => {
                cmd.arg("--strict-mcp-config")
                    .arg("--setting-sources")
                    .arg("user");
            }
        }

        // No permission checks, ever. Verified to execute headlessly against
        // 2.1.220 — `dontAsk` silently *denies* instead of prompting, which
        // made the cat look broken rather than cautious.
        cmd.arg("--permission-mode").arg("bypassPermissions");

        // Append rather than replace: the default prompt carries the tool-use
        // conventions that make the agent competent. Only its self-image needs
        // correcting, and without this it refuses non-coding work outright.
        cmd.arg("--append-system-prompt").arg(persona(&req.name));

        if let Some(model) = &req.model {
            cmd.arg("--model").arg(model);
        }

        match &req.resume {
            Some(id) => {
                cmd.arg("--resume").arg(id);
            }
            None => {
                // Pre-seeding the id means we can resume even if the turn dies
                // before the init event lands.
                cmd.arg("--session-id")
                    .arg(uuid::Uuid::new_v4().to_string());
            }
        }

        if let Some(cwd) = &req.cwd {
            cmd.current_dir(cwd);
        }
        cmd
    }

    fn parse(&mut self, line: &str) -> Vec<BridgeEvent> {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return vec![];
        };
        let mut out = vec![];

        match v.get("type").and_then(Value::as_str) {
            // Hook chatter and other system noise: only `init` matters.
            Some("system") => {
                if v.get("subtype").and_then(Value::as_str) == Some("init") {
                    let session = v
                        .get("session_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    out.push(BridgeEvent::Started {
                        session,
                        model: v.get("model").and_then(Value::as_str).map(str::to_string),
                    });
                }
            }

            Some("assistant") => {
                let blocks = v
                    .pointer("/message/content")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                for block in blocks {
                    match block.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            let text = block
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .trim()
                                .to_string();
                            if !text.is_empty() {
                                out.push(BridgeEvent::Text { text });
                            }
                        }
                        Some("thinking") => out.push(BridgeEvent::Thinking),
                        Some("tool_use") => {
                            let tool = block
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("tool")
                                .to_string();
                            let detail =
                                summarize(&tool, block.get("input").unwrap_or(&Value::Null));
                            out.push(BridgeEvent::ToolStart { tool, detail });
                        }
                        _ => {}
                    }
                }
            }

            // Tool results come back addressed to the model as user turns.
            Some("user") => {
                let blocks = v
                    .pointer("/message/content")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                for block in blocks {
                    if block.get("type").and_then(Value::as_str) == Some("tool_result") {
                        out.push(BridgeEvent::ToolEnd {
                            tool: block
                                .get("tool_use_id")
                                .and_then(Value::as_str)
                                .unwrap_or("tool")
                                .to_string(),
                            ok: !block
                                .get("is_error")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                        });
                    }
                }
            }

            Some("result") => {
                self.finished = true;
                let is_error = v.get("is_error").and_then(Value::as_bool).unwrap_or(false);
                out.push(BridgeEvent::Finished {
                    ok: !is_error,
                    text: v
                        .get("result")
                        .and_then(Value::as_str)
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty()),
                    ms: v.get("duration_ms").and_then(Value::as_u64),
                });
            }

            _ => {}
        }
        out
    }

    fn on_exit(&mut self, code: Option<i32>, stderr: &str) -> Option<BridgeEvent> {
        if self.finished {
            return None;
        }
        // Exit 0 without a result event means the CLI stopped early but not
        // abnormally — close the turn quietly rather than showing an error.
        if code == Some(0) {
            return Some(BridgeEvent::Finished {
                ok: true,
                text: None,
                ms: None,
            });
        }
        let hint = if stderr.contains("Invalid API key") || stderr.contains("/login") {
            "Claude Code couldn't authenticate. Run `claude` once in a terminal and \
             log in, or give the cat an API key from the panel."
                .to_string()
        } else {
            stderr
                .trim()
                .chars()
                .rev()
                .take(400)
                .collect::<String>()
                .chars()
                .rev()
                .collect()
        };
        Some(BridgeEvent::Failed {
            message: if hint.is_empty() {
                format!("claude exited with status {}", code.unwrap_or(-1))
            } else {
                hint
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lines below are verbatim captures from Claude Code 2.1.220, trimmed of
    /// fields we don't read. If a future version renames any of these keys the
    /// bridge goes quiet, so pin them here.
    #[test]
    fn reads_init_and_result() {
        let mut a = ClaudeAdapter::default();

        let init = r#"{"type":"system","subtype":"init","session_id":"11111111-2222-4333-8444-555555555555","model":"claude-haiku-4-5-20251001","tools":["Bash"]}"#;
        match a.parse(init).as_slice() {
            [BridgeEvent::Started { session, model }] => {
                assert_eq!(session, "11111111-2222-4333-8444-555555555555");
                assert_eq!(model.as_deref(), Some("claude-haiku-4-5-20251001"));
            }
            other => panic!("expected Started, got {other:?}"),
        }

        // Hook chatter must not produce events.
        let hook =
            r#"{"type":"system","subtype":"hook_started","hook_name":"SessionStart:startup"}"#;
        assert!(a.parse(hook).is_empty());

        let result = r#"{"type":"result","subtype":"success","is_error":false,"result":"ok","duration_ms":1462}"#;
        match a.parse(result).as_slice() {
            [BridgeEvent::Finished { ok, text, ms }] => {
                assert!(ok);
                assert_eq!(text.as_deref(), Some("ok"));
                assert_eq!(*ms, Some(1462));
            }
            other => panic!("expected Finished, got {other:?}"),
        }

        // Terminal event already sent, so exiting must not double-report.
        assert!(a.on_exit(Some(0), "").is_none());
    }

    #[test]
    fn reads_assistant_blocks() {
        let mut a = ClaudeAdapter::default();
        let line = r#"{"type":"assistant","message":{"content":[
            {"type":"thinking","thinking":"hmm"},
            {"type":"text","text":"  on it  "},
            {"type":"tool_use","name":"Bash","input":{"command":"ls -la /tmp"}}
        ]}}"#;
        match a.parse(line).as_slice() {
            [BridgeEvent::Thinking, BridgeEvent::Text { text }, BridgeEvent::ToolStart { tool, detail }] =>
            {
                assert_eq!(text, "on it");
                assert_eq!(tool, "Bash");
                assert_eq!(detail, "ls -la /tmp");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn reads_tool_results() {
        let mut a = ClaudeAdapter::default();
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","is_error":true}]}}"#;
        match a.parse(line).as_slice() {
            [BridgeEvent::ToolEnd { ok, .. }] => assert!(!ok),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn survives_garbage_and_closes_dead_turns() {
        let mut a = ClaudeAdapter::default();
        assert!(a.parse("not json at all").is_empty());
        assert!(a.parse("").is_empty());
        // Died without a result event: the UI must not hang.
        assert!(matches!(
            a.on_exit(Some(1), "Invalid API key · Please run /login"),
            Some(BridgeEvent::Failed { .. })
        ));
    }

    fn request() -> TurnRequest {
        TurnRequest {
            backend: "claude".into(),
            prompt: "hi".into(),
            resume: None,
            cwd: None,
            model: None,
            name: "Biscuit".into(),
        }
    }

    fn args_for(req: &TurnRequest, auth: Auth) -> Vec<String> {
        ClaudeAdapter::default()
            .command("claude", req, auth)
            .as_std()
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn always_bypasses_permissions() {
        for auth in [Auth::Inherit, Auth::Subscription, Auth::Key] {
            let args = args_for(&request(), auth);

            // The cat must never be launched in a mode that can refuse work,
            // whoever is paying for the turn.
            assert!(
                args.windows(2)
                    .any(|w| w == ["--permission-mode", "bypassPermissions"]),
                "{auth:?} lost the permission bypass"
            );

            // Without the persona the agent refuses non-coding work on identity
            // grounds, which reads to the user as a broken app.
            let i = args
                .iter()
                .position(|a| a == "--append-system-prompt")
                .expect("persona must be passed");
            // And it goes in wearing this cat's name, not the app's.
            assert_eq!(args.get(i + 1), Some(&persona("Biscuit")));
        }
    }

    /// The billing invariant. `--bare` decides which credential Claude Code is
    /// even allowed to read, so getting this backwards silently charges the
    /// wrong account — the one failure the user cannot see happening.
    #[test]
    fn bare_mode_tracks_who_is_paying() {
        // On a subscription, --bare would refuse to read the OAuth login that
        // the entire bridge is built on.
        for auth in [Auth::Inherit, Auth::Subscription] {
            let args = args_for(&request(), auth);
            assert!(
                !args.iter().any(|a| a == "--bare"),
                "{auth:?} passed --bare"
            );
            assert!(args.iter().any(|a| a == "--strict-mcp-config"));
        }

        // On the user's own key it's the only way to stop a stale OAuth login
        // winning and spending the subscription instead.
        let args = args_for(&request(), Auth::Key);
        assert!(args.iter().any(|a| a == "--bare"));
        // --bare already implies this isolation; passing both is contradictory.
        assert!(!args.iter().any(|a| a == "--setting-sources"));
    }

    /// The key must never reach argv — anything there is readable by other
    /// local processes. It travels in the environment, set by the session.
    #[test]
    fn the_key_is_never_an_argument() {
        let args = args_for(&request(), Auth::Key);
        assert!(!args.iter().any(|a| a.contains("sk-")));
        assert!(!args.iter().any(|a| a.contains("ANTHROPIC_API_KEY")));
    }

    #[test]
    fn resume_replaces_session_id() {
        let mut req = TurnRequest {
            resume: Some("abc".into()),
            ..request()
        };
        let args = |req: &TurnRequest| args_for(req, Auth::Inherit);

        let resumed = args(&req);
        assert!(resumed.windows(2).any(|w| w == ["--resume", "abc"]));
        assert!(!resumed.iter().any(|a| a == "--session-id"));

        req.resume = None;
        let fresh = args(&req);
        assert!(fresh.iter().any(|a| a == "--session-id"));
        assert!(!fresh.iter().any(|a| a == "--resume"));
    }
}
