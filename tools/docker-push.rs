use std::process::Command;

pub fn main() {
    let status = Command::new("docker")
        .args([
            "tag",
            env!("CARGO_PKG_NAME"),
            &format!(
                "codeberg.org/mto/{}:{}",
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION")
            ),
        ])
        .status()
        .expect("Failed to run Docker tag");

    if !status.success() {
        eprintln!("Docker rename failed!");
        std::process::exit(1);
    }

    let status = Command::new("docker")
        .args([
            "tag",
            env!("CARGO_PKG_NAME"),
            &format!(
                "codeberg.org/mto/{}:latest",
                env!("CARGO_PKG_NAME")
            ),
        ])
        .status()
        .expect("Failed to run Docker tag");

    if !status.success() {
        eprintln!("Docker rename failed!");
        std::process::exit(1);
    }

    let status = Command::new("docker")
        .args([
            "push",
            &format!(
                "codeberg.org/mto/{}:{}",
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION")
            ),
        ])
        .status()
        .expect("Failed to run Docker push");

    if !status.success() {
        eprintln!("Docker push failed!");
        std::process::exit(1);
    }

    let status = Command::new("docker")
        .args([
            "push",
            &format!(
                "codeberg.org/mto/{}:latest",
                env!("CARGO_PKG_NAME")
            ),
        ])
        .status()
        .expect("Failed to run Docker push");

    if !status.success() {
        eprintln!("Docker push failed!");
        std::process::exit(1);
    }
}

