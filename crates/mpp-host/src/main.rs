use std::path::PathBuf;
use std::process::ExitCode;

use mpp_android::Android;
use mpp_core::{Error, Result};
use mpp_host::{Host, Response, value};
use mpp_ios::{CaptureConfig, Ios};

const HELP: &str = "Mobile Preview Plugin — Rust framework\n\nUsage: mpp [--adb PATH] [--emulator PATH] [--xcrun PATH] COMMAND\n\n  devices                 List Android devices and iOS Simulators as JSON\n  probe --device SERIAL   Check one Android transport; does not acquire a session\n  probe --simulator UDID  Check one booted iOS Simulator\n  boot --avd NAME         Explicitly start an existing Android AVD\n  boot --simulator UDID   Explicitly start an existing iOS Simulator\n  serve --stdio           Serve mpp/v1 JSON-lines; leases live until disconnect/EOF\n  --version               Print version\n\nAndroid preview requires device assets (Android 10–17 / API 29–37, arm64). iOS Simulator preview requires macOS with Xcode; input depends on Simulator support.";

struct Cli {
    adb: Option<PathBuf>,
    emulator: Option<PathBuf>,
    xcrun: Option<PathBuf>,
    args: Vec<String>,
}

fn parse() -> Result<Cli> {
    let args = std::env::args_os()
        .skip(1)
        .map(|arg| {
            arg.into_string().map_err(|_| Error::InvalidArgument {
                message: "arguments must be valid UTF-8".into(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut args = args.into_iter();
    let mut cli = Cli {
        adb: None,
        emulator: None,
        xcrun: None,
        args: Vec::new(),
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--adb" | "--emulator" | "--xcrun" => {
                let path = args.next().filter(|path| !path.is_empty()).ok_or_else(|| {
                    Error::InvalidArgument {
                        message: format!("{arg} requires a path"),
                    }
                })?;
                match arg.as_str() {
                    "--adb" => cli.adb = Some(path.into()),
                    "--emulator" => cli.emulator = Some(path.into()),
                    _ => cli.xcrun = Some(path.into()),
                }
            }
            _ => cli.args.push(arg),
        }
    }
    Ok(cli)
}

fn print(result: Result<serde_json::Value>) -> ExitCode {
    let response = Response::new(None, result);
    let success = response.ok;
    match serde_json::to_string(&response) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("Could not encode response: {error}");
            return ExitCode::FAILURE;
        }
    }
    if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn main() -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("Cannot initialize the local runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    let status = runtime.block_on(run());
    // Host teardown has awaited owned resources. Tokio's blocking stdin read may still await
    // a writer after SIGTERM; it must not keep this process alive after graceful cleanup.
    runtime.shutdown_timeout(std::time::Duration::from_millis(100));
    status
}

async fn run() -> ExitCode {
    let internal = std::env::args_os()
        .skip(1)
        .any(|arg| arg == "--ios-capture" || arg == "--ios-probe");
    let cli = match parse() {
        Ok(cli) => cli,
        Err(error) if internal => return diagnostic(error),
        Err(error) => return print(Err(error)),
    };
    if internal {
        return run_internal(&cli.args).await;
    }
    match cli.args.as_slice() {
        [] => {
            println!("{HELP}");
            return ExitCode::SUCCESS;
        }
        [arg] if arg == "--help" || arg == "-h" => {
            println!("{HELP}");
            return ExitCode::SUCCESS;
        }
        [arg] if arg == "--version" => {
            println!("mpp {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        _ => {}
    }
    if !matches!(cli.args.as_slice(), [command] if command == "devices")
        && !matches!(cli.args.as_slice(), [command, flag] if command == "serve" && flag == "--stdio")
        && !matches!(cli.args.as_slice(), [command, flag, _] if (command == "probe" && matches!(flag.as_str(), "--device" | "--simulator")) || (command == "boot" && matches!(flag.as_str(), "--avd" | "--simulator")))
    {
        return print(Err(Error::InvalidArgument {
            message: "unknown command or arguments; use mpp --help".into(),
        }));
    }
    let android = match cli.adb {
        Some(adb) => Some(Android::with_tools(adb, None)),
        None => Android::discover().ok(),
    };
    let android = android.map(|android| match cli.emulator {
        Some(emulator) => android.with_emulator(emulator),
        None => android,
    });
    let ios = match cli.xcrun {
        Some(xcrun) => match std::env::current_exe() {
            Ok(executable) => Some(Ios::with_tools(xcrun, executable)),
            Err(error) => return print(Err(error.into())),
        },
        None => Ios::discover().ok(),
    };
    match cli.args.as_slice() {
        [command] if command == "devices" => print(
            Host::with_backends(android, ios)
                .inventory(None)
                .await
                .and_then(value),
        ),
        [command, flag, serial] if command == "probe" && flag == "--device" => {
            print(match &android {
                Some(android) => android.probe(serial).await.and_then(value),
                None => Err(missing_backend("Android SDK")),
            })
        }
        [command, flag, avd] if command == "boot" && flag == "--avd" => print(match &android {
            Some(android) => android.boot(avd).await.and_then(value),
            None => Err(missing_backend("Android SDK")),
        }),
        [command, flag, udid] if flag == "--simulator" => print(match &ios {
            Some(ios) if command == "probe" => ios.probe(udid).await.and_then(value),
            Some(ios) => ios.boot(udid).await.and_then(value),
            None => Err(missing_backend("Xcode Simulator")),
        }),
        [command, flag] if command == "serve" && flag == "--stdio" => {
            let mut host = Host::with_backends(android, ios);
            match mpp_host::serve(
                &mut host,
                tokio::io::BufReader::new(tokio::io::stdin()),
                tokio::io::stdout(),
            )
            .await
            {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("{error}; {}", error.hint());
                    ExitCode::FAILURE
                }
            }
        }
        _ => print(Err(Error::InvalidArgument {
            message: "unknown command or arguments; use mpp --help".into(),
        })),
    }
}

fn missing_backend(tool: &str) -> Error {
    Error::ToolNotFound { tool: tool.into() }
}

fn diagnostic(error: Error) -> ExitCode {
    eprintln!("{error}; {}", error.hint());
    ExitCode::FAILURE
}

async fn run_internal(args: &[String]) -> ExitCode {
    if let [mode, udid] = args
        && mode == "--ios-probe"
    {
        return match mpp_ios::capture::probe(udid).and_then(value) {
            Ok(probe) => {
                println!("{probe}");
                ExitCode::SUCCESS
            }
            Err(error) => diagnostic(error),
        };
    }
    let config = match capture_config(args) {
        Ok(config) => config,
        Err(error) => return diagnostic(error),
    };
    match mpp_ios::capture::run(config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => diagnostic(error),
    }
}

fn capture_config(args: &[String]) -> Result<CaptureConfig> {
    let invalid = || Error::InvalidArgument {
        message: "invalid internal iOS capture arguments".into(),
    };
    let [
        mode,
        udid,
        boot_flag,
        boot_id,
        generation_flag,
        generation,
        epoch_flag,
        epoch,
        size_flag,
        max_size,
        rate_flag,
        bit_rate,
        fps_flag,
        max_fps,
        control_args @ ..,
    ] = args
    else {
        return Err(invalid());
    };
    if mode != "--ios-capture"
        || boot_flag != "--boot-id"
        || generation_flag != "--generation"
        || epoch_flag != "--epoch"
        || size_flag != "--max-size"
        || rate_flag != "--bit-rate"
        || fps_flag != "--max-fps"
    {
        return Err(invalid());
    }
    let (control_fd, input_enabled) = match control_args {
        [] => (None, false),
        [flag, fd] if flag == "--control-fd" && fd == "0" => (Some(0), false),
        [flag, fd, input] if flag == "--control-fd" && fd == "0" && input == "--enable-input" => {
            (Some(0), true)
        }
        _ => return Err(invalid()),
    };
    Ok(CaptureConfig {
        udid: udid.clone(),
        boot_id: boot_id.clone(),
        generation: generation.parse().map_err(|_| invalid())?,
        epoch: epoch.parse().map_err(|_| invalid())?,
        max_size: max_size.parse().map_err(|_| invalid())?,
        bit_rate: bit_rate.parse().map_err(|_| invalid())?,
        max_fps: max_fps.parse().map_err(|_| invalid())?,
        control_fd,
        input_enabled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture_args() -> Vec<String> {
        [
            "--ios-capture",
            "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE",
            "--boot-id",
            "boot-1",
            "--generation",
            "1",
            "--epoch",
            "2",
            "--max-size",
            "1280",
            "--bit-rate",
            "4000000",
            "--max-fps",
            "30",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    #[test]
    fn internal_capture_control_uses_only_explicit_fd_zero() {
        assert_eq!(capture_config(&capture_args()).unwrap().control_fd, None);
        assert!(!capture_config(&capture_args()).unwrap().input_enabled);
        let mut args = capture_args();
        args.extend(["--control-fd".into(), "0".into()]);
        assert_eq!(capture_config(&args).unwrap().control_fd, Some(0));
        assert!(!capture_config(&args).unwrap().input_enabled);
        args.push("--enable-input".into());
        assert!(capture_config(&args).unwrap().input_enabled);
        for fd in ["1", "2", "-1", "00", "not-a-fd"] {
            let mut args = capture_args();
            args.extend(["--control-fd".into(), fd.into()]);
            assert!(capture_config(&args).is_err(), "accepted descriptor {fd}");
        }
        args.push("--control-fd".into());
        assert!(capture_config(&args).is_err());
        let mut args = capture_args();
        args.push("--control-fd".into());
        assert!(capture_config(&args).is_err());
        let mut args = capture_args();
        args.push("--enable-input".into());
        assert!(capture_config(&args).is_err());
    }
}
