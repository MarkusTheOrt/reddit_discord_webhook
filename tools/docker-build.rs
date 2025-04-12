use std::process::Command;

pub fn main() {
    let status = Command::new("docker")
        .env("DOCKER_BAKE", "1")
        .args([
            "build",
            "-f",
            "./Dockerfile",
            "-t",
            env!("CARGO_PKG_NAME"),
            ".",
        ])
        .status()
        .expect("Failed to run Docker build");

    if !status.success() {
        eprintln!("Docker build failed!");
        std::process::exit(1);
    }
}

