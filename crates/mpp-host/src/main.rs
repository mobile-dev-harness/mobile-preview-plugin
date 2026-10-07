use std::path::PathBuf;
use std::process::ExitCode;

use mpp_android::Android;
use mpp_core::{Error, Result};
use mpp_host::{Host, Response, value};

const HELP: &str = "Mobile Preview Plugin — Rust framework\n\nUsage: mpp [--adb PATH] [--emulator PATH] COMMAND\n\n  devices                 List devices and available AVDs as JSON\n  probe --device SERIAL   Check one Android transport; does not acquire a session\n  boot --avd NAME         Explicitly start an existing AVD and await Android boot\n  serve --stdio           Serve mpp/v1 JSON-lines; leases live until disconnect/EOF\n  --version               Print version\n\nLive preview requires the DSH plugin and Android device assets (Android 10–17 / API 29–37, arm64).";

struct Cli {
    adb: Option<PathBuf>,
    emulator: Option<PathBuf>,
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
        args: Vec::new(),
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--adb" | "--emulator" => {
                let path = args.next().filter(|path| !path.is_empty()).ok_or_else(|| {
                    Error::InvalidArgument {
                        message: format!("{arg} requires a path"),
                    }
                })?;
                if arg == "--adb" {
                    cli.adb = Some(path.into());
                } else {
                    cli.emulator = Some(path.into());
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
    let cli = match parse() {
        Ok(cli) => cli,
        Err(error) => return print(Err(error)),
    };
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
        && !matches!(cli.args.as_slice(), [command, flag, _] if (command == "probe" && flag == "--device") || (command == "boot" && flag == "--avd"))
    {
        return print(Err(Error::InvalidArgument {
            message: "unknown command or arguments; use mpp --help".into(),
        }));
    }
    let mut android = match cli.adb {
        Some(adb) => Android::with_tools(adb, None),
        None => match Android::discover() {
            Ok(android) => android,
            Err(error) => return print(Err(error)),
        },
    };
    if let Some(emulator) = cli.emulator {
        android = android.with_emulator(emulator);
    }
    match cli.args.as_slice() {
        [command] if command == "devices" => print(android.inventory().await.and_then(value)),
        [command, flag, serial] if command == "probe" && flag == "--device" => {
            print(android.probe(serial).await.and_then(value))
        }
        [command, flag, avd] if command == "boot" && flag == "--avd" => {
            print(android.boot(avd).await.and_then(value))
        }
        [command, flag] if command == "serve" && flag == "--stdio" => {
            let mut host = Host::new(android);
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
