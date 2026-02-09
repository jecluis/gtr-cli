use std::process::Command;

fn main() {
    // Capture git SHA at build time
    let output = Command::new("git")
        .args(["rev-parse", "--short=8", "HEAD"])
        .output();

    let git_sha = match output {
        Ok(output) if output.status.success() => {
            String::from_utf8(output.stdout).unwrap_or_else(|_| "unknown".to_string())
        }
        _ => "unknown".to_string(),
    };

    println!("cargo:rustc-env=GIT_SHA={}", git_sha.trim());
}
