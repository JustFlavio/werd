fn main() {
    if let Err(error) = werd_core::run_daemon() {
        eprintln!("Werd daemon: {error:#}");
        std::process::exit(1);
    }
}
