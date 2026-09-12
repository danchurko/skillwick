fn main() {
    if let Err(error) = skillwick::cli::run() {
        eprintln!("error: {error}");
        std::process::exit(error.code());
    }
}
