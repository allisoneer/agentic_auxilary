use claudecode::types::Event;
use claudecode::types::Result as ClaudeResult;
use claudecode::types::ResultEvent;
use claudecode::types::SystemEvent;
use std::env;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::signal::unix::SignalKind;

#[derive(serde::Serialize)]
struct PidInfo {
    parent_pid: u32,
    child_pid: u32,
}

struct HelperChild {
    child: Option<Child>,
}

impl HelperChild {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn id(&self) -> u32 {
        match self.child.as_ref() {
            Some(child) => child.id(),
            None => panic!("helper child must exist while pid is queried"),
        }
    }

    fn cleanup(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for HelperChild {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let output_format = arg_value(&args, "--output-format").unwrap_or_else(|| "text".into());
    let query = args
        .iter()
        .position(|arg| arg == "--")
        .and_then(|idx| args.get(idx + 1))
        .cloned()
        .unwrap_or_default();
    let should_hang_on_term = query.contains("[hang]");

    let child = Command::new("sh")
        .arg("-c")
        .arg("trap '' TERM INT; while :; do sleep 1; done")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let child = HelperChild::new(child);

    if let Some(pid_file) = env::var_os("FAKE_CLAUDE_PID_FILE") {
        let info = PidInfo {
            parent_pid: std::process::id(),
            child_pid: child.id(),
        };
        std::fs::write(PathBuf::from(pid_file), serde_json::to_vec(&info)?)?;
    }

    maybe_force_error_after_spawn()?;

    emit_output(&output_format).await?;

    let mut sigterm = tokio::signal::unix::signal(SignalKind::terminate())?;
    let mut sigint = tokio::signal::unix::signal(SignalKind::interrupt())?;
    loop {
        tokio::select! {
            _ = sigterm.recv() => {
                if !should_hang_on_term {
                    break;
                }
            }
            _ = sigint.recv() => {
                if !should_hang_on_term {
                    break;
                }
            }
            () = tokio::time::sleep(std::time::Duration::from_secs(60)) => {}
        }
    }

    Ok(())
}

fn maybe_force_error_after_spawn() -> Result<(), Box<dyn std::error::Error>> {
    if env::var_os("FAKE_CLAUDE_FORCE_ERROR_AFTER_SPAWN").is_some() {
        return Err(std::io::Error::other("forced fake_claude error after helper spawn").into());
    }

    Ok(())
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].clone())
}

async fn emit_output(output_format: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = tokio::io::stdout();

    match output_format {
        "json" => {
            let result = ClaudeResult {
                result_type: Some("result".into()),
                content: Some("fake json output".into()),
                result: Some("fake json output".into()),
                is_error: false,
                session_id: Some("fake-session".into()),
                ..ClaudeResult::default()
            };
            stdout
                .write_all(serde_json::to_string(&result)?.as_bytes())
                .await?;
        }
        "stream-json" => {
            let system = Event::System(SystemEvent {
                session_id: "fake-session".into(),
                subtype: Some("init".into()),
                cwd: None,
                model: Some("fake-model".into()),
                permission_mode: None,
                api_key_source: None,
                tools: None,
                mcp_servers: None,
            });
            let result = Event::Result(ResultEvent {
                session_id: "fake-session".into(),
                result: Some("fake stream output".into()),
                is_error: false,
                error: None,
                total_cost_usd: None,
                duration_ms: Some(1),
                duration_api_ms: Some(1),
                num_turns: Some(1),
                usage: None,
            });
            stdout
                .write_all(
                    format!(
                        "{}\n{}\n",
                        serde_json::to_string(&system)?,
                        serde_json::to_string(&result)?
                    )
                    .as_bytes(),
                )
                .await?;
        }
        _ => {
            stdout.write_all(b"fake text output").await?;
        }
    }

    stdout.flush().await?;
    Ok(())
}
