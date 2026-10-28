fn main() {
    assert_eq!(std::env::args().nth(1).as_deref(), Some("verify"));
    for args in [
        vec!["fmt", "--all", "--", "--check"],
        vec!["test", "--locked"],
        vec![
            "clippy",
            "--locked",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    ] {
        let status = std::process::Command::new("cargo")
            .args(args)
            .status()
            .expect("run cargo");
        if !status.success() {
            std::process::exit(1);
        }
    }
}
