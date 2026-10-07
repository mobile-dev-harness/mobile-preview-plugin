fn main() {
    match mpp_android_device::probe_encoder() {
        Ok(report) => {
            if let Err(error) = serde_json::to_writer(std::io::stdout().lock(), &report) {
                eprintln!("could not write probe result: {error}");
                std::process::exit(1);
            }
            println!();
        }
        Err(error) => {
            eprintln!("{}: {error}; {}", error.code(), error.hint());
            std::process::exit(1);
        }
    }
}
