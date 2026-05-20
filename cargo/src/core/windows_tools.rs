use crate::core::adk::AdkPaths;
use crate::core::errors::{BitOSDTError, BitOSDTResult};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

#[cfg(target_os = "windows")]
fn resolve_system_dism() -> std::path::PathBuf {
    std::path::PathBuf::from(r"C:\Windows\System32\dism.exe")
}

#[cfg(not(target_os = "windows"))]
fn resolve_system_dism() -> std::path::PathBuf {
    std::path::PathBuf::from("dism")
}

#[derive(Debug, Clone)]
pub struct TrackedBuildProcess {
    pub pid: u32,
    pub executable: String,
    pub command_line: String,
    pub role: String,
}

pub struct BuildRuntimeHooks {
    pub is_cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    pub on_started: Arc<dyn Fn(TrackedBuildProcess) + Send + Sync>,
    pub on_exited: Arc<dyn Fn(u32) + Send + Sync>,
}

static BUILD_RUNTIME_HOOKS: OnceLock<BuildRuntimeHooks> = OnceLock::new();

pub fn set_build_runtime_hooks(hooks: BuildRuntimeHooks) -> Result<(), &'static str> {
    BUILD_RUNTIME_HOOKS
        .set(hooks)
        .map_err(|_| "build runtime hooks already configured")
}

fn build_runtime_hooks() -> Option<&'static BuildRuntimeHooks> {
    BUILD_RUNTIME_HOOKS.get()
}

pub fn is_build_cancelled() -> bool {
    build_runtime_hooks()
        .map(|hooks| (hooks.is_cancelled)())
        .unwrap_or(false)
}

fn notify_process_started(process: TrackedBuildProcess) {
    if let Some(hooks) = build_runtime_hooks() {
        (hooks.on_started)(process);
    }
}

fn notify_process_exited(pid: u32) {
    if let Some(hooks) = build_runtime_hooks() {
        (hooks.on_exited)(pid);
    }
}

pub fn resolve_dism_exe(adk_paths: Option<&AdkPaths>) -> std::path::PathBuf {
    if let Some(adk) = adk_paths {
        if adk.dism_exe.exists() {
            return adk.dism_exe.clone();
        }
    }
    resolve_system_dism()
}

pub fn dism_path_arg(flag: &str, value: &Path) -> String {
    format!("{}:{}", flag, value.display())
}

pub fn format_command_for_logs(exe: &Path, args: &[String]) -> String {
    let mut rendered = exe.to_string_lossy().to_string();
    for arg in args {
        rendered.push(' ');
        if arg.contains(' ') {
            rendered.push('"');
            rendered.push_str(arg);
            rendered.push('"');
        } else {
            rendered.push_str(arg);
        }
    }
    rendered
}

fn output_text(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes).trim().to_string();
    if text.is_empty() {
        "<empty>".to_string()
    } else {
        text
    }
}

pub fn format_process_failure(exe: &Path, args: &[String], output: &Output) -> String {
    let code = output
        .status
        .code()
        .map(|c| c.to_string())
        .unwrap_or_else(|| "terminated by signal".to_string());
    format!(
        "command=\"{}\", exit_code={}, stdout={}, stderr={}",
        format_command_for_logs(exe, args),
        code,
        output_text(&output.stdout),
        output_text(&output.stderr)
    )
}

pub fn run_dism(args: &[String], adk_paths: Option<&AdkPaths>) -> BitOSDTResult<Output> {
    let dism_exe = resolve_dism_exe(adk_paths);
    Command::new(&dism_exe).args(args).output().map_err(|e| {
        BitOSDTError::WinPE(format!(
            "Failed to run DISM ({}): {}",
            format_command_for_logs(&dism_exe, args),
            e
        ))
    })
}

pub fn run_dism_with_role(
    args: &[String],
    adk_paths: Option<&AdkPaths>,
    role: &str,
) -> BitOSDTResult<Output> {
    let dism_exe = resolve_dism_exe(adk_paths);
    let mut command = Command::new(&dism_exe);
    command.args(args);
    run_tracked_command_streaming(command, &dism_exe, args, role, |_| {})
}

fn normalize_stream_line(raw: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(raw).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn drain_stream_lines(buffer: &mut Vec<u8>) -> Vec<String> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;

    while index < buffer.len() {
        if matches!(buffer[index], b'\r' | b'\n') {
            if let Some(line) = normalize_stream_line(&buffer[start..index]) {
                lines.push(line);
            }

            index += 1;
            while index < buffer.len() && matches!(buffer[index], b'\r' | b'\n') {
                index += 1;
            }
            start = index;
        } else {
            index += 1;
        }
    }

    if start > 0 {
        buffer.drain(0..start);
    }

    lines
}

fn finish_stream_line(buffer: &mut Vec<u8>) -> Option<String> {
    if buffer.is_empty() {
        return None;
    }

    let line = normalize_stream_line(buffer);
    buffer.clear();
    line
}

fn spawn_stream_reader<R>(
    mut reader: R,
    sender: mpsc::Sender<String>,
) -> thread::JoinHandle<BitOSDTResult<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut raw = Vec::new();
        let mut pending = Vec::new();
        let mut chunk = [0u8; 4096];

        loop {
            let read = reader.read(&mut chunk).map_err(|e| {
                BitOSDTError::WinPE(format!("Failed to read DISM command output: {}", e))
            })?;
            if read == 0 {
                break;
            }

            raw.extend_from_slice(&chunk[..read]);
            pending.extend_from_slice(&chunk[..read]);
            for line in drain_stream_lines(&mut pending) {
                let _ = sender.send(line);
            }
        }

        if let Some(line) = finish_stream_line(&mut pending) {
            let _ = sender.send(line);
        }

        Ok(raw)
    })
}

pub fn run_dism_streaming<F>(
    args: &[String],
    adk_paths: Option<&AdkPaths>,
    mut output_callback: F,
) -> BitOSDTResult<Output>
where
    F: FnMut(String),
{
    let dism_exe = resolve_dism_exe(adk_paths);
    let mut child = Command::new(&dism_exe)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            BitOSDTError::WinPE(format!(
                "Failed to run DISM ({}): {}",
                format_command_for_logs(&dism_exe, args),
                e
            ))
        })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| BitOSDTError::WinPE("Failed to capture DISM stdout stream".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| BitOSDTError::WinPE("Failed to capture DISM stderr stream".to_string()))?;

    let (sender, receiver) = mpsc::channel::<String>();
    let stdout_reader = spawn_stream_reader(stdout, sender.clone());
    let stderr_reader = spawn_stream_reader(stderr, sender.clone());
    drop(sender);

    let mut status = None;
    loop {
        match receiver.recv_timeout(Duration::from_millis(250)) {
            Ok(line) => output_callback(line),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if status.is_none() {
                    status = child.try_wait().map_err(|e| {
                        BitOSDTError::WinPE(format!(
                            "Failed while waiting for DISM ({}): {}",
                            format_command_for_logs(&dism_exe, args),
                            e
                        ))
                    })?;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let status = if let Some(status) = status {
        status
    } else {
        child.wait().map_err(|e| {
            BitOSDTError::WinPE(format!(
                "Failed while waiting for DISM ({}): {}",
                format_command_for_logs(&dism_exe, args),
                e
            ))
        })?
    };

    let stdout = stdout_reader
        .join()
        .map_err(|_| BitOSDTError::WinPE("DISM stdout reader thread panicked".to_string()))??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| BitOSDTError::WinPE("DISM stderr reader thread panicked".to_string()))??;

    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

pub fn run_dism_streaming_with_role<F>(
    args: &[String],
    adk_paths: Option<&AdkPaths>,
    role: &str,
    output_callback: F,
) -> BitOSDTResult<Output>
where
    F: FnMut(String),
{
    let dism_exe = resolve_dism_exe(adk_paths);
    let mut command = Command::new(&dism_exe);
    command.args(args);
    run_tracked_command_streaming(command, &dism_exe, args, role, output_callback)
}

pub fn run_tracked_command_streaming<F>(
    mut command: Command,
    executable: &Path,
    args: &[String],
    role: &str,
    mut output_callback: F,
) -> BitOSDTResult<Output>
where
    F: FnMut(String),
{
    if is_build_cancelled() {
        return Err(BitOSDTError::Cancelled);
    }

    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|e| {
        BitOSDTError::WinPE(format!(
            "Failed to run command ({}): {}",
            format_command_for_logs(executable, args),
            e
        ))
    })?;

    let child_pid = child.id();
    notify_process_started(TrackedBuildProcess {
        pid: child_pid,
        executable: executable.display().to_string(),
        command_line: format_command_for_logs(executable, args),
        role: role.to_string(),
    });

    struct ProcessRegistrationGuard {
        pid: u32,
    }

    impl Drop for ProcessRegistrationGuard {
        fn drop(&mut self) {
            notify_process_exited(self.pid);
        }
    }

    let _guard = ProcessRegistrationGuard { pid: child_pid };

    let stdout = child.stdout.take().ok_or_else(|| {
        BitOSDTError::WinPE("Failed to capture command stdout stream".to_string())
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        BitOSDTError::WinPE("Failed to capture command stderr stream".to_string())
    })?;

    let (sender, receiver) = mpsc::channel::<String>();
    let stdout_reader = spawn_stream_reader(stdout, sender.clone());
    let stderr_reader = spawn_stream_reader(stderr, sender.clone());
    drop(sender);

    let mut status = None;
    let mut cancel_requested = false;
    loop {
        match receiver.recv_timeout(Duration::from_millis(250)) {
            Ok(line) => output_callback(line),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if !cancel_requested && is_build_cancelled() {
                    cancel_requested = true;
                    let _ = child.kill();
                }
                if status.is_none() {
                    status = child.try_wait().map_err(|e| {
                        BitOSDTError::WinPE(format!(
                            "Failed while waiting for command ({}): {}",
                            format_command_for_logs(executable, args),
                            e
                        ))
                    })?;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let status = if let Some(status) = status {
        status
    } else {
        child.wait().map_err(|e| {
            BitOSDTError::WinPE(format!(
                "Failed while waiting for command ({}): {}",
                format_command_for_logs(executable, args),
                e
            ))
        })?
    };

    let stdout = stdout_reader
        .join()
        .map_err(|_| BitOSDTError::WinPE("Command stdout reader thread panicked".to_string()))??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| BitOSDTError::WinPE("Command stderr reader thread panicked".to_string()))??;

    if cancel_requested {
        return Err(BitOSDTError::Cancelled);
    }

    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::{drain_stream_lines, finish_stream_line};

    #[test]
    fn drain_stream_lines_splits_crlf_and_carriage_return_updates() {
        let mut buffer = b"10.0%\r25.0%\rCompleted step\r\nFinal line".to_vec();

        let lines = drain_stream_lines(&mut buffer);
        assert_eq!(lines, vec!["10.0%", "25.0%", "Completed step"]);
        assert_eq!(buffer, b"Final line".to_vec());
        assert_eq!(
            finish_stream_line(&mut buffer),
            Some("Final line".to_string())
        );
        assert!(buffer.is_empty());
    }

    #[test]
    fn drain_stream_lines_ignores_blank_segments() {
        let mut buffer = b"\r\nWorking...\n\nStill working\r\n".to_vec();
        let lines = drain_stream_lines(&mut buffer);

        assert_eq!(lines, vec!["Working...", "Still working"]);
        assert!(buffer.is_empty());
        assert_eq!(finish_stream_line(&mut buffer), None);
    }
}
