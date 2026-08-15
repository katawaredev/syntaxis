use syntaxis_git::{GitError, GitErrorCode, GitResult};

#[cfg(unix)]
pub(super) fn signing_wrapper(
    passphrase: &[u8],
) -> GitResult<(tempfile::TempDir, std::path::PathBuf, std::path::PathBuf)> {
    use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt};

    let directory = tempfile::Builder::new()
        .prefix("syntaxis-gpg-")
        .tempdir()
        .map_err(|_| internal_error())?;
    let path = directory.path().join("gpg-loopback");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o700)
        .open(&path)
        .map_err(|_| internal_error())?;
    file.write_all(
        b"#!/bin/sh\nprogram=${SYNTAXIS_GPG_PROGRAM:-gpg}\nif [ \"$program\" = \"$0\" ]; then program=gpg; fi\nexec 3<\"$SYNTAXIS_GPG_PASSPHRASE_FILE\"\nexec \"$program\" --batch --pinentry-mode loopback --passphrase-fd 3 \"$@\"\n",
    )
    .map_err(|_| internal_error())?;
    drop(file);

    let passphrase_path = directory.path().join("passphrase");
    let mut passphrase_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&passphrase_path)
        .map_err(|_| internal_error())?;
    passphrase_file
        .write_all(passphrase)
        .and_then(|()| passphrase_file.write_all(b"\n"))
        .map_err(|_| internal_error())?;
    drop(passphrase_file);

    Ok((directory, path, passphrase_path))
}

#[cfg(not(unix))]
pub(super) fn signing_wrapper(
    _passphrase: &[u8],
) -> GitResult<(tempfile::TempDir, std::path::PathBuf, std::path::PathBuf)> {
    Err(GitError::new(
        GitErrorCode::Unavailable,
        "In-app signing passphrase retry is not available on this server platform.",
    ))
}

fn internal_error() -> GitError {
    GitError::new(
        GitErrorCode::Internal,
        "The Git operation could not be completed.",
    )
}
