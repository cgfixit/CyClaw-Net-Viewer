fn main() {
    let args: Vec<String> = std::env::args().collect();
    match netboard::cli::dispatch(&args) {
        Ok(Some(0)) => {}
        Ok(Some(code)) => std::process::exit(code),
        Ok(None) => {
            if let Err(e) = netboard::app::run() {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
}
