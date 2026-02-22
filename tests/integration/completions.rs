#[test]
fn completions_bash_generates_output() {
    use clap::CommandFactory;
    use clap_complete::aot::generate;
    use std::io::BufWriter;

    let mut buf = BufWriter::new(Vec::new());
    generate(
        clap_complete::aot::Shell::Bash,
        &mut devrig::cli::Cli::command(),
        "devrig",
        &mut buf,
    );

    let output = String::from_utf8(buf.into_inner().unwrap()).unwrap();
    assert!(!output.is_empty(), "bash completions should not be empty");
    assert!(
        output.contains("devrig"),
        "bash completions should reference 'devrig'"
    );
}

#[test]
fn completions_zsh_generates_output() {
    use clap::CommandFactory;
    use clap_complete::aot::generate;
    use std::io::BufWriter;

    let mut buf = BufWriter::new(Vec::new());
    generate(
        clap_complete::aot::Shell::Zsh,
        &mut devrig::cli::Cli::command(),
        "devrig",
        &mut buf,
    );

    let output = String::from_utf8(buf.into_inner().unwrap()).unwrap();
    assert!(!output.is_empty(), "zsh completions should not be empty");
    assert!(
        output.contains("devrig"),
        "zsh completions should reference 'devrig'"
    );
}

#[test]
fn completions_fish_generates_output() {
    use clap::CommandFactory;
    use clap_complete::aot::generate;
    use std::io::BufWriter;

    let mut buf = BufWriter::new(Vec::new());
    generate(
        clap_complete::aot::Shell::Fish,
        &mut devrig::cli::Cli::command(),
        "devrig",
        &mut buf,
    );

    let output = String::from_utf8(buf.into_inner().unwrap()).unwrap();
    assert!(!output.is_empty(), "fish completions should not be empty");
    assert!(
        output.contains("devrig"),
        "fish completions should reference 'devrig'"
    );
}
