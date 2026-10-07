use std::{io, path::Path, process::Stdio, time::Duration};

use mpp_core::{Error, Result};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    time::timeout,
};

const MAX_OUTPUT: usize = 1024 * 1024;
pub(crate) const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) async fn run(tool: &Path, args: &[&str]) -> Result<String> {
    run_with_timeout(tool, args, COMMAND_TIMEOUT).await
}

async fn run_with_timeout(tool: &Path, args: &[&str], duration: Duration) -> Result<String> {
    let name = tool.display().to_string();
    let mut child = Command::new(tool)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| spawn_error(&name, error))?;
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");
    let operation =
        async { tokio::try_join!(read_bounded(stdout), read_bounded(stderr), child.wait()) };
    let result = timeout(duration, operation).await;
    match result {
        Ok(Ok((stdout, stderr, status))) => {
            if !status.success() {
                return Err(Error::CommandFailed {
                    tool: name,
                    message: format!("exit {status}: {}", String::from_utf8_lossy(&stderr).trim()),
                });
            }
            String::from_utf8(stdout).map_err(|_| Error::CommandFailed {
                tool: name,
                message: "output was not UTF-8".into(),
            })
        }
        Ok(Err(error)) => {
            let _ = child.kill().await;
            Err(Error::CommandFailed {
                tool: name,
                message: error.to_string(),
            })
        }
        Err(_) => {
            let _ = child.kill().await;
            Err(Error::Timeout {
                operation: format!("{name} {}", args.first().unwrap_or(&"")),
            })
        }
    }
}

pub(crate) fn spawn_error(tool: &str, error: io::Error) -> Error {
    match error.kind() {
        io::ErrorKind::NotFound => Error::ToolNotFound { tool: tool.into() },
        io::ErrorKind::PermissionDenied => Error::PermissionDenied {
            message: format!("Cannot execute {tool}; check its file permissions"),
        },
        _ => Error::Io(error),
    }
}

async fn read_bounded(mut reader: impl AsyncRead + Unpin) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut chunk = [0; 8192];
    loop {
        let length = reader.read(&mut chunk).await?;
        if length == 0 {
            return Ok(bytes);
        }
        if bytes.len() + length > MAX_OUTPUT {
            return Err(io::Error::other("tool output exceeded 1 MiB"));
        }
        bytes.extend_from_slice(&chunk[..length]);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bounded_runner_times_out_and_caps_output() {
        let error = run_with_timeout(
            Path::new("/bin/sh"),
            &["-c", "exec sleep 2"],
            Duration::from_millis(20),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::Timeout { .. }));
        let error = run(
            Path::new("/bin/sh"),
            &["-c", "exec head -c 1048577 /dev/zero"],
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::CommandFailed { .. }));
    }
}
