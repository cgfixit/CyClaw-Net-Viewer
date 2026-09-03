fn main() {
    let args: Vec<String> = std::env::args().collect();
    match netboard::cli::dispatch(&args) {
        Ok(true) => {}
        Ok(false) => {
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
