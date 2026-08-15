use portable_pty::CommandBuilder;
use std::{
    env, fs,
    path::{Path, PathBuf},
};
use syntaxis_terminal::{TerminalError, TerminalErrorCode};

pub(super) struct ShellRc(PathBuf);

impl ShellRc {
    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ShellRc {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub(super) fn controlled_shell_command(
    root: &Path,
) -> Result<(CommandBuilder, ShellRc), TerminalError> {
    let shell = env::var_os("SHELL")
        .map(PathBuf::from)
        .filter(|path| {
            path.is_absolute()
                && path.is_file()
                && path.file_name().is_some_and(|name| name == "bash")
        })
        .or_else(|| {
            Path::new("/bin/bash")
                .is_file()
                .then(|| PathBuf::from("/bin/bash"))
        })
        .ok_or_else(|| unavailable("Bash is unavailable"))?;
    let shell_rc = create_shell_rc()?;
    let mut command = CommandBuilder::new(shell);
    command.env_clear();
    for key in [
        "HOME",
        "USER",
        "LOGNAME",
        "PATH",
        "LANG",
        "LC_ALL",
        "SSH_AUTH_SOCK",
    ] {
        if let Some(value) = env::var_os(key) {
            command.env(key, value);
        }
    }
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("TERM_PROGRAM", "Syntaxis");
    command.env("PWD", root);
    command.arg("--noprofile");
    command.arg("--rcfile");
    command.arg(shell_rc.path());
    command.arg("-i");
    Ok((command, shell_rc))
}

fn create_shell_rc() -> Result<ShellRc, TerminalError> {
    let path = env::temp_dir().join(format!("syntaxis-bash-{}.rc", uuid::Uuid::new_v4()));
    fs::write(
        &path,
        r#"if [[ -r "$HOME/.bashrc" ]]; then
    source "$HOME/.bashrc"
fi

if [[ "$(declare -p PROMPT_COMMAND 2>/dev/null)" == "declare -a"* ]]; then
    __syntaxis_original_prompt_commands=("${PROMPT_COMMAND[@]}")
elif [[ -n "${PROMPT_COMMAND-}" ]]; then
    __syntaxis_original_prompt_commands=("$PROMPT_COMMAND")
else
    __syntaxis_original_prompt_commands=()
fi
PS0=$'\e]777;syntaxis;command-start\a'"${PS0-}"

__syntaxis_prompt_command() {
    local __syntaxis_status=$?
    printf '\e]777;syntaxis;command-end;%d\a' "$__syntaxis_status"
    local __syntaxis_prompt_status="$__syntaxis_status"
    local __syntaxis_prompt_entry
    for __syntaxis_prompt_entry in "${__syntaxis_original_prompt_commands[@]}"; do
        (exit "$__syntaxis_prompt_status")
        eval -- "$__syntaxis_prompt_entry"
        __syntaxis_prompt_status=$?
    done
}

PROMPT_COMMAND=(__syntaxis_prompt_command)
"#,
    )
    .map_err(|_| unavailable("Failed to prepare Bash integration"))?;
    Ok(ShellRc(path))
}

fn unavailable(message: &'static str) -> TerminalError {
    TerminalError::new(TerminalErrorCode::Unavailable, message)
}
