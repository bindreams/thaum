use std::ffi::OsString;

testutil::default_labels!(exec);

#[cfg(unix)]
mod posix_quoting {
    use super::*;

    fn roundtrip(args: &[&str]) {
        let argv: Vec<OsString> = args.iter().map(OsString::from).collect();
        let cmd = super::super::CommandEx::new(argv.clone());
        let cmdline = cmd.commandline();

        // Parse back using /bin/sh -c 'printf "%s\n" ...' would be ideal,
        // but for unit tests we just verify the structure.
        let cmdline_str = cmdline.to_string_lossy();

        // Each arg should be single-quoted in the output.
        for arg in args {
            // The arg should appear somewhere in the command line.
            // For args without single quotes, they appear as 'arg'.
            if !arg.contains('\'') {
                assert!(
                    cmdline_str.contains(&format!("'{arg}'")),
                    "expected '{arg}' in command line: {cmdline_str}"
                );
            }
        }
    }

    #[testutil::test]
    fn simple_args() {
        roundtrip(&["echo", "hello", "world"]);
    }

    #[testutil::test]
    fn args_with_spaces() {
        roundtrip(&["echo", "hello world", "foo bar"]);
    }

    #[testutil::test]
    fn args_with_single_quotes() {
        let argv = vec![OsString::from("echo"), OsString::from("it's")];
        let cmd = super::super::CommandEx::new(argv);
        let cmdline = cmd.commandline();
        let s = cmdline.to_string_lossy();
        // "it's" should be quoted as 'it'\''s'
        assert!(s.contains("'it'\\''s'"), "got: {s}");
    }

    #[testutil::test]
    fn empty_arg() {
        let argv = vec![OsString::from("echo"), OsString::from("")];
        let cmd = super::super::CommandEx::new(argv);
        let cmdline = cmd.commandline();
        let s = cmdline.to_string_lossy();
        // Empty arg should appear as ''
        assert!(s.contains("''"), "got: {s}");
    }

    #[testutil::test]
    fn args_with_special_chars() {
        roundtrip(&["echo", "hello\nworld", "tab\there", "$HOME"]);
    }

    #[testutil::test]
    fn spawn_echo() {
        let argv = vec![OsString::from("echo"), OsString::from("hello")];
        let mut cmd = super::super::CommandEx::new(argv);
        cmd.fds.insert(1, super::super::Fd::Pipe);
        let mut child = cmd.spawn().expect("spawn failed");
        let status = child.wait().expect("wait failed");
        assert_eq!(status, 0);
        let mut stdout_pipe = child.take_pipe(1).expect("no stdout pipe");
        let mut output = String::new();
        std::io::Read::read_to_string(&mut stdout_pipe, &mut output).unwrap();
        assert_eq!(output, "hello\n");
    }

    #[testutil::test]
    fn spawn_with_env() {
        let mut cmd = super::super::CommandEx::new(vec![
            OsString::from("sh"),
            OsString::from("-c"),
            OsString::from("echo $MY_TEST_VAR"),
        ]);
        cmd.env.insert(OsString::from("MY_TEST_VAR"), OsString::from("works"));
        cmd.fds.insert(1, super::super::Fd::Pipe);
        let mut child = cmd.spawn().expect("spawn failed");
        let status = child.wait().expect("wait failed");
        assert_eq!(status, 0);
        let mut stdout_pipe = child.take_pipe(1).expect("no stdout pipe");
        let mut output = String::new();
        std::io::Read::read_to_string(&mut stdout_pipe, &mut output).unwrap();
        assert_eq!(output, "works\n");
    }

    #[testutil::test]
    fn spawn_fd3_inheritance() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("fd3.txt");
        let file = std::fs::File::create(&file_path).unwrap();

        let mut cmd = super::super::CommandEx::new(vec![
            OsString::from("sh"),
            OsString::from("-c"),
            OsString::from("echo hello >&3"),
        ]);
        cmd.fds.insert(3, super::super::Fd::File(file));
        let mut child = cmd.spawn().expect("spawn failed");
        let status = child.wait().expect("wait failed");
        assert_eq!(status, 0);
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "hello\n");
    }
}
