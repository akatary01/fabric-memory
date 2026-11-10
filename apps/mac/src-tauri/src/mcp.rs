use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc, Mutex,
    },
};
struct Worker {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<String>,
}
pub struct Host {
    worker: Mutex<Option<Worker>>,
    pid: AtomicU32,
}
impl Default for Host {
    fn default() -> Self {
        Self {
            worker: Mutex::new(None),
            pid: AtomicU32::new(0),
        }
    }
}
impl Host {
    pub fn request(&self, command: &str, args: Value) -> Result<Value, String> {
        let mut holder = self.worker.lock().map_err(|_| "MCP host unavailable")?;
        if holder
            .as_mut()
            .is_some_and(|w| w.child.try_wait().ok().flatten().is_some())
        {
            *holder = None;
        }
        if holder.is_none() {
            let path = std::path::PathBuf::from(
                std::env::var_os("FABRIC_DATA_DIR").ok_or("Missing app storage")?,
            )
            .join("mcp.json");
            let python =
                std::env::var("FABRIC_PYTHON").unwrap_or_else(|_| "/usr/bin/python3".into());
            let mut child = Command::new(python)
                .args(["-u", "-c", include_str!("../../../../tools/mcp_bridge.py")])
                .arg(path)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| e.to_string())?;
            self.pid.store(child.id(), Ordering::Relaxed);
            let input = child.stdin.take().unwrap();
            let stdout = child.stdout.take().unwrap();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    match line {
                        Ok(line) => {
                            if tx.send(line).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
            *holder = Some(Worker {
                child,
                input,
                output: rx,
            });
        }
        let worker = holder.as_mut().unwrap();
        writeln!(worker.input, "{}", json!({"command":command,"args":args}))
            .map_err(|_| "MCP host disconnected")?;
        worker.input.flush().map_err(|e| e.to_string())?;
        let line = match worker
            .output
            .recv_timeout(std::time::Duration::from_secs(130))
        {
            Ok(v) => v,
            Err(_) => {
                let _ = worker.child.kill();
                *holder = None;
                self.pid.store(0, Ordering::Relaxed);
                return Err("MCP request stopped or timed out".into());
            }
        };
        let result: Value = serde_json::from_str(&line).map_err(|_| "Invalid MCP host response")?;
        if let Some(e) = result["error"].as_str() {
            return Err(e.into());
        }
        Ok(result["result"].clone())
    }
    pub fn stop(&self) {
        let pid = self.pid.swap(0, Ordering::Relaxed);
        if pid != 0 {
            let _ = Command::new("/bin/kill")
                .args(["-TERM", &pid.to_string()])
                .status();
        }
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.stop();
    }
}
