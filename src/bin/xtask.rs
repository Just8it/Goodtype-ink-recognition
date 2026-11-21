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
    for (program, args) in [
        (
            "python",
            vec![
                "-m",
                "unittest",
                "discover",
                "-s",
                "scripts",
                "-p",
                "test_release.py",
            ],
        ),
        ("node", vec!["--test", "scripts/release.test.mjs"]),
    ] {
        if !std::process::Command::new(program)
            .args(args)
            .status()
            .expect("run release script tests")
            .success()
        {
            std::process::exit(1);
        }
    }
}
