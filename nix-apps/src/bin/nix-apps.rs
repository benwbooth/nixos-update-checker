fn main() {
    std::process::exit(nix_apps::cli::run(std::env::args().skip(1).collect()));
}
