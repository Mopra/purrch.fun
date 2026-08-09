//! Codex CLI adapter. Rides a ChatGPT Plus/Pro subscription.
//!
//! Unlike the Claude adapter, this schema is *not* verified against a live
//! run — Codex wasn't installed on the development machine. The parser is
//! therefore written defensively: it accepts both the `{id, msg:{type,..}}`
//! envelope and the newer flat `{type: "item.completed", item:{..}}` thread
//! events, and silently ignores anything it doesn't recognise. Worst case a
//! turn shows less detail; it should never hard-fail on an unexpected line.

use super::creds::Auth;
use super::persona::persona;
use super::{Adapter, BridgeEvent, TurnRequest};
use serde_json::Value;
use tokio::process::Command;

#[derive(Default)]
pub struct CodexAdapter {
    session: Option<String>,
    finished: bool,
    /// Last assistant message, used as the turn result if the terminal event
    /// doesn't carry one.
    last_message: Option<String>,
}

impl Adapter for CodexAdapter {
    fn command(&self, program: &str, req: &TurnRequest, auth: Auth) -> Command {
        let mut cmd = Command::new(program);
        cmd.arg("exec");

        if let Some(id) = &req.resume {
            cmd.arg("resume").arg(id);
        }

        cmd.arg("--json")
            // The cat's working dir is usually not a git repo.
            .arg("--skip-git-repo-check");

        // Matches the Claude adapter: no sandbox, no approvals.
        cmd.arg("--dangerously-bypass-approvals-and-sandbox");

        // Codex has no `--bare`, so the equivalent lever is a config override.
        // Unverified, like the rest of this file — but it can only affect a
        // user who has explicitly chosen to spend a key, and the alternative is
        // worse: with a ChatGPT login still on disk, `OPENAI_API_KEY` alone may
        // simply lose, and their subscription gets spent without a word.
        if auth == Auth::Key {
            cmd.arg("--config").arg("preferred_auth_method=\"apikey\"");
        }

        if let Some(model) = &req.model {
            cmd.arg("--model").arg(model);
        }

        // Codex has no `--append-system-prompt`, so the persona rides in on
        // the prompt itself — and only on the first turn, since a resumed
        // conversation already carries it. A cat renamed mid-conversation
        // therefore answers to its old name here until it forgets and starts
        // over; Claude, which is re-told every turn, picks it up immediately.
        let prompt = match req.resume {
            Some(_) => req.prompt.clone(),
            None => format!("{}\n\n---\n\n{}", persona(&req.name), req.prompt),
        };

        // Prompt goes last so it is never parsed as a flag value.
        cmd.arg("--").arg(prompt);

        if let Some(cwd) = &req.cwd {
            cmd.current_dir(cwd);
        }
        cmd
    }

    fn parse(&mut self, line: &str) -> Vec<BridgeEvent> {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return vec![];
        };

        // Both envelopes carry the payload under a different key.
        let msg = v.get("msg").or_else(|| v.get("item")).unwrap_or(&v);
        let Some(ty) = msg
            .get("type")
            .and_then(Value::as_str)
            .or_else(|| v.get("type").and_then(Value::as_str))
        else {
            return vec![];
        };

        let text_of = |keys: &[&str]| -> Option<String> {
            keys.iter()
                .find_map(|k| msg.get(*k).and_then(Value::as_str))
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };

        match ty {
            "session_configured" | "task_started" | "thread.started" => {
                let session = msg
                    .get("session_id")
                    .or_else(|| msg.get("thread_id"))
                    .or_else(|| v.get("session_id"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if session.is_empty() && self.session.is_some() {
                    return vec![];
                }
                self.session = Some(session.clone());
                vec![BridgeEvent::Started {
                    session,
                    model: msg.get("model").and_then(Value::as_str).map(str::to_string),
                }]
            }

            "agent_reasoning" | "agent_reasoning_delta" | "reasoning" => {
                vec![BridgeEvent::Thinking]
            }

            "agent_message" | "assistant_message" => match text_of(&["message", "text"]) {
                Some(text) => {
                    self.last_message = Some(text.clone());
                    vec![BridgeEvent::Text { text }]
                }
                None => vec![],
            },

            "exec_command_begin" | "command_execution.started" => {
                // `command` is either a string or an argv array.
                let detail = msg
                    .get("command")
                    .map(|c| match c.as_array() {
                        Some(parts) => parts
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(" "),
                        None => c.as_str().unwrap_or_default().to_string(),
                    })
                    .unwrap_or_default();
                vec![BridgeEvent::ToolStart {
                    tool: "shell".into(),
                    detail,
                }]
            }

            "exec_command_end" | "command_execution.completed" => {
                vec![BridgeEvent::ToolEnd {
                    tool: "shell".into(),
                    ok: msg.get("exit_code").and_then(Value::as_i64).unwrap_or(0) == 0,
                }]
            }

            "patch_apply_begin" | "file_change.started" => vec![BridgeEvent::ToolStart {
                tool: "edit".into(),
                detail: text_of(&["path", "file"]).unwrap_or_else(|| "editing files".into()),
            }],

            "patch_apply_end" | "file_change.completed" => vec![BridgeEvent::ToolEnd {
                tool: "edit".into(),
                ok: msg.get("success").and_then(Value::as_bool).unwrap_or(true),
            }],

            "task_complete" | "thread.completed" => {
                self.finished = true;
                vec![BridgeEvent::Finished {
                    ok: true,
                    text: text_of(&["last_agent_message"]).or_else(|| self.last_message.clone()),
                    ms: None,
                }]
            }

            "error" | "stream_error" | "thread.failed" => {
                self.finished = true;
                vec![BridgeEvent::Failed {
                    message: text_of(&["message", "error"])
                        .unwrap_or_else(|| "codex reported an error".into()),
                }]
            }

            _ => vec![],
        }
    }

    fn on_exit(&mut self, code: Option<i32>, stderr: &str) -> Option<BridgeEvent> {
        if self.finished {
            return None;
        }
        if code == Some(0) {
            return Some(BridgeEvent::Finished {
                ok: true,
                text: self.last_message.clone(),
                ms: None,
            });
        }
        let msg = if stderr.contains("not logged in") || stderr.contains("login") {
            "Codex isn't signed in. Run `codex login` once in a terminal.".to_string()
        } else {
            let tail: String = stderr.trim().chars().rev().take(400).collect();
            let tail: String = tail.chars().rev().collect();
            if tail.is_empty() {
                format!("codex exited with status {}", code.unwrap_or(-1))
            } else {
                tail
            }
        };
        Some(BridgeEvent::Failed { message: msg })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_for(auth: Auth) -> Vec<String> {
        CodexAdapter::default()
            .command(
                "codex",
                &TurnRequest {
                    backend: "codex".into(),
                    prompt: "hi".into(),
                    resume: None,
                    cwd: None,
                    model: None,
                    name: "Biscuit".into(),
                },
                auth,
            )
            .as_std()
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect()
    }

    /// Same billing invariant as the Claude adapter: nothing may nudge Codex
    /// towards the API key unless the user actually asked to spend one.
    #[test]
    fn the_auth_override_only_appears_for_a_key() {
        for auth in [Auth::Inherit, Auth::Subscription] {
            assert!(
                !args_for(auth).iter().any(|a| a.contains("preferred_auth_method")),
                "{auth:?} overrode the auth method"
            );
        }
        let args = args_for(Auth::Key);
        let i = args.iter().position(|a| a == "--config").expect("no override");
        assert_eq!(
            args.get(i + 1).map(String::as_str),
            Some("preferred_auth_method=\"apikey\"")
        );
    }

    #[test]
    fn the_key_is_never_an_argument() {
        assert!(!args_for(Auth::Key).iter().any(|a| a.contains("sk-")));
    }
}
