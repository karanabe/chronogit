use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::{self, Read};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::domain::{CommitBaseline, RepositoryRoot};
use crate::git::GitCommand;

const MAX_STDOUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 64 * 1024;
const MAX_COMMAND_DURATION: Duration = Duration::from_secs(30);
type ReaderResult = io::Result<(Vec<u8>, bool)>;
type ReaderHandle = thread::JoinHandle<ReaderResult>;

#[derive(Debug)]
pub struct CommandOutput {
    success: bool,
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdout_truncated: bool,
    stderr_truncated: bool,
}

impl CommandOutput {
    #[cfg(test)]
    pub(crate) fn for_test(
        success: bool,
        code: Option<i32>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    ) -> Self {
        Self {
            success,
            code,
            stdout,
            stderr,
            stdout_truncated: false,
            stderr_truncated: false,
        }
    }

    #[must_use]
    pub fn success(&self) -> bool {
        self.success
    }

    #[must_use]
    pub fn code(&self) -> Option<i32> {
        self.code
    }

    #[must_use]
    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }

    #[must_use]
    pub fn stderr(&self) -> &[u8] {
        &self.stderr
    }

    #[must_use]
    pub fn stdout_truncated(&self) -> bool {
        self.stdout_truncated
    }

    #[must_use]
    pub fn stderr_truncated(&self) -> bool {
        self.stderr_truncated
    }
}

#[derive(Debug)]
pub enum GitError {
    Io {
        operation: &'static str,
        source: io::Error,
    },
    CommandFailed {
        operation: &'static str,
        code: Option<i32>,
        stderr: String,
    },
    OutputLimit {
        operation: &'static str,
    },
    TimedOut {
        operation: &'static str,
    },
    Parse {
        context: &'static str,
        detail: String,
    },
    Unsupported(String),
}

impl GitError {
    #[must_use]
    pub fn parse(context: &'static str, detail: impl Into<String>) -> Self {
        Self::Parse {
            context,
            detail: detail.into(),
        }
    }
}

impl Display for GitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, .. } => write!(formatter, "could not {operation}"),
            Self::CommandFailed {
                operation,
                code,
                stderr,
            } => {
                write!(formatter, "could not {operation} (exit {code:?})")?;
                if !stderr.trim().is_empty() {
                    write!(formatter, ": {}", stderr.trim())?;
                }
                Ok(())
            }
            Self::OutputLimit { operation } => {
                write!(formatter, "{operation} exceeded the output limit")
            }
            Self::TimedOut { operation } => {
                write!(formatter, "{operation} exceeded the 30 second time limit")
            }
            Self::Parse { context, detail } => {
                write!(formatter, "could not parse {context}: {detail}")
            }
            Self::Unsupported(detail) => formatter.write_str(detail),
        }
    }
}

impl Error for GitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::CommandFailed { .. }
            | Self::OutputLimit { .. }
            | Self::TimedOut { .. }
            | Self::Parse { .. }
            | Self::Unsupported(_) => None,
        }
    }
}

pub trait GitRunner: Send + Sync + 'static {
    fn run(
        &self,
        root: Option<&RepositoryRoot>,
        command: &GitCommand,
    ) -> Result<CommandOutput, GitError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemGitRunner;

impl GitRunner for SystemGitRunner {
    fn run(
        &self,
        root: Option<&RepositoryRoot>,
        command: &GitCommand,
    ) -> Result<CommandOutput, GitError> {
        let mut process = build_process(root, command)?;
        process.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = process.spawn().map_err(|source| GitError::Io {
            operation: command.kind(),
            source,
        })?;
        let stdout = child.stdout.take().ok_or_else(|| GitError::Io {
            operation: command.kind(),
            source: io::Error::other("Git stdout pipe was unavailable"),
        })?;
        let stderr = child.stderr.take().ok_or_else(|| GitError::Io {
            operation: command.kind(),
            source: io::Error::other("Git stderr pipe was unavailable"),
        })?;

        let exceeded = Arc::new(AtomicBool::new(false));
        let stdout_exceeded = Arc::clone(&exceeded);
        let stderr_exceeded = Arc::clone(&exceeded);
        let stdout_reader = match spawn_reader(
            "chronogit-git-stdout",
            stdout,
            MAX_STDOUT_BYTES,
            stdout_exceeded,
        ) {
            Ok(reader) => reader,
            Err(source) => {
                stop_child(&mut child);
                return Err(GitError::Io {
                    operation: command.kind(),
                    source,
                });
            }
        };
        let stderr_reader = match spawn_reader(
            "chronogit-git-stderr",
            stderr,
            MAX_STDERR_BYTES,
            stderr_exceeded,
        ) {
            Ok(reader) => reader,
            Err(source) => {
                stop_child(&mut child);
                let _ignored = stdout_reader.join();
                return Err(GitError::Io {
                    operation: command.kind(),
                    source,
                });
            }
        };

        let status = wait_with_limit(&mut child, &exceeded, command.kind(), MAX_COMMAND_DURATION);
        let (stdout, stdout_truncated) = join_reader(stdout_reader, command.kind())?;
        let (stderr, stderr_truncated) = join_reader(stderr_reader, command.kind())?;
        let status = status?;

        Ok(CommandOutput {
            success: status.success(),
            code: status.code(),
            stdout,
            stderr,
            stdout_truncated,
            stderr_truncated,
        })
    }
}

fn build_process(root: Option<&RepositoryRoot>, command: &GitCommand) -> Result<Command, GitError> {
    let mut process = Command::new("git");
    process
        .arg("--no-pager")
        .arg("--literal-pathspecs")
        .arg("-c")
        .arg("color.ui=false")
        .arg("-c")
        .arg("core.quotePath=false")
        .arg("-c")
        .arg("core.fsmonitor=false")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .env("PAGER", "cat");

    if let Some(root) = root {
        process.arg("-C").arg(root.as_path());
    }

    match command {
        GitCommand::Discover { start } => {
            process
                .arg("-C")
                .arg(start)
                .args(["rev-parse", "--show-toplevel"]);
        }
        GitCommand::IsBare => {
            process.args(["rev-parse", "--is-bare-repository"]);
        }
        GitCommand::HasHead => {
            process.args(["rev-parse", "--verify", "--quiet", "HEAD"]);
        }
        GitCommand::Status => {
            process.args(["status", "--porcelain=v2", "-z", "--untracked-files=all"]);
        }
        GitCommand::WorktreeDiff { path } => {
            process.args([
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--find-renames",
                "--find-copies",
                "--",
            ]);
            process.arg(path.to_os_string());
        }
        GitCommand::UntrackedDiff { path } => {
            process.args([
                "diff",
                "--no-index",
                "--no-ext-diff",
                "--no-textconv",
                "--",
                "/dev/null",
            ]);
            process.arg(path.to_os_string());
        }
        GitCommand::Commits { skip, limit } => {
            process
                .arg("log")
                .arg("-z")
                .arg("--date=iso-strict")
                .arg("--format=%H%x00%P%x00%an%x00%aI%x00%s")
                .arg(format!("--skip={skip}"))
                .arg(format!("--max-count={limit}"));
        }
        GitCommand::CommitMessage { commit } => {
            process.args(["show", "-s", "--format=%B", commit.as_str()]);
        }
        GitCommand::ChangedFiles { commit, baseline } => match baseline {
            CommitBaseline::EmptyTree => {
                process.args([
                    "diff-tree",
                    "--root",
                    "--no-commit-id",
                    "--name-status",
                    "-r",
                    "-z",
                    "--find-renames",
                    "--find-copies",
                    commit.as_str(),
                    "--",
                ]);
            }
            CommitBaseline::FirstParent(parent) => {
                process.args([
                    "diff",
                    "--name-status",
                    "-z",
                    "--find-renames",
                    "--find-copies",
                    parent.as_str(),
                    commit.as_str(),
                    "--",
                ]);
            }
        },
        GitCommand::CommitDiff {
            commit,
            baseline,
            path,
        } => {
            match baseline {
                CommitBaseline::EmptyTree => {
                    process.args([
                        "show",
                        "--format=",
                        "--no-ext-diff",
                        "--no-textconv",
                        "--find-renames",
                        "--find-copies",
                        commit.as_str(),
                        "--",
                    ]);
                }
                CommitBaseline::FirstParent(parent) => {
                    process.args([
                        "diff",
                        "--no-ext-diff",
                        "--no-textconv",
                        "--find-renames",
                        "--find-copies",
                        parent.as_str(),
                        commit.as_str(),
                        "--",
                    ]);
                }
            }
            process.arg(path.to_os_string());
        }
        GitCommand::TreeEntries { treeish } => {
            process.args(["ls-tree", "-z", treeish.as_str()]);
        }
    }
    Ok(process)
}

fn read_limited<R: Read>(
    mut reader: R,
    limit: usize,
    exceeded: &AtomicBool,
) -> io::Result<(Vec<u8>, bool)> {
    let mut stored = Vec::with_capacity(limit.min(64 * 1024));
    let mut buffer = [0_u8; 16 * 1024];
    let mut truncated = false;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(stored.len());
        let keep = remaining.min(read);
        stored.extend_from_slice(&buffer[..keep]);
        if keep < read {
            truncated = true;
            exceeded.store(true, Ordering::Release);
        }
    }
    Ok((stored, truncated))
}

fn spawn_reader<R: Read + Send + 'static>(
    name: &'static str,
    reader: R,
    limit: usize,
    exceeded: Arc<AtomicBool>,
) -> io::Result<ReaderHandle> {
    thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || read_limited(reader, limit, &exceeded))
}

fn stop_child(child: &mut std::process::Child) {
    let _ignored = child.kill();
    let _ignored = child.wait();
}

fn wait_with_limit(
    child: &mut std::process::Child,
    exceeded: &AtomicBool,
    operation: &'static str,
    timeout: Duration,
) -> Result<ExitStatus, GitError> {
    let started = Instant::now();
    loop {
        if exceeded.load(Ordering::Acquire) {
            let _ignored = child.kill();
            return child
                .wait()
                .map_err(|source| GitError::Io { operation, source });
        }
        if started.elapsed() >= timeout {
            let _ignored = child.kill();
            child
                .wait()
                .map_err(|source| GitError::Io { operation, source })?;
            return Err(GitError::TimedOut { operation });
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|source| GitError::Io { operation, source })?
        {
            return Ok(status);
        }
        thread::sleep(Duration::from_millis(2));
    }
}

fn join_reader(handle: ReaderHandle, operation: &'static str) -> Result<(Vec<u8>, bool), GitError> {
    handle
        .join()
        .map_err(|_| GitError::Io {
            operation,
            source: io::Error::other("Git output reader panicked"),
        })?
        .map_err(|source| GitError::Io { operation, source })
}

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    use super::{GitError, wait_with_limit};

    #[cfg(unix)]
    #[test]
    fn wait_with_limit_terminates_a_slow_child() {
        let mut child = Command::new("sleep")
            .arg("10")
            .spawn()
            .unwrap_or_else(|error| panic!("failed to start sleep fixture: {error}"));
        let exceeded = AtomicBool::new(false);

        let result = wait_with_limit(
            &mut child,
            &exceeded,
            "run timeout fixture",
            Duration::from_millis(10),
        );

        assert!(matches!(result, Err(GitError::TimedOut { .. })));
    }
}
