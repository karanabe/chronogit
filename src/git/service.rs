//! Domain-level repository reads built on the typed Git runner boundary.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::Read;
use std::os::fd::OwnedFd;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};

use bstr::ByteSlice;
use rustix::fs::{FileType, Mode, OFlags};
use rustix::io::Errno;

use crate::domain::{
    ChangedFile, CommitBaseline, CommitMessage, CommitSummary, DiffDocument, DiffTarget,
    FileDocument, ObjectId, RepoPath, RepositoryRoot, SearchHit, TreeEntry, WorktreeChange,
};
use crate::git::parse::{
    parse_changed_files, parse_commits, parse_file_paths, parse_grep_matches, parse_patch,
    parse_status, parse_tree_entries,
};
use crate::git::{CommandOutput, GitCommand, GitError, GitRunner};

/// A discovered, non-bare repository accessed through a [`GitRunner`].
///
/// The service owns the absolute worktree root and is the only component that
/// combines typed commands, bounded output, parsers, and domain comparison
/// policy. It exposes no operation that mutates the repository.
#[derive(Debug)]
pub struct GitService<R> {
    runner: R,
    root: RepositoryRoot,
    worktree: OwnedFd,
}

impl<R: GitRunner> GitService<R> {
    /// Resolves `start` to a non-bare worktree and constructs its service.
    ///
    /// `start` may be the worktree root or any directory beneath it.
    ///
    /// # Errors
    ///
    /// Returns an error when the path cannot be canonicalized, is not a
    /// directory, is outside a Git worktree, is bare, or when Git returns
    /// malformed, truncated, or unsuccessful discovery output.
    pub fn discover(runner: R, start: &Path) -> Result<Self, GitError> {
        let start = start.canonicalize().map_err(|source| GitError::Io {
            operation: "resolve repository path",
            source,
        })?;
        if !start.is_dir() {
            return Err(GitError::Unsupported(format!(
                "repository path is not a directory: {}",
                start.display()
            )));
        }
        let output = runner.run(None, &GitCommand::Discover { start })?;
        ensure_complete(&output, "discover repository")?;
        ensure_success(&output, "discover repository")?;
        let raw = trim_newline(output.stdout());
        if raw.is_empty() {
            return Err(GitError::parse(
                "repository root",
                "Git returned an empty path",
            ));
        }
        #[cfg(unix)]
        let path = std::path::PathBuf::from(OsString::from_vec(raw.to_vec()));
        let root = RepositoryRoot::new(path)
            .map_err(|detail| GitError::parse("repository root", detail))?;
        let bare = runner.run(Some(&root), &GitCommand::IsBare)?;
        ensure_complete(&bare, "check bare repository")?;
        ensure_success(&bare, "check bare repository")?;
        if trim_newline(bare.stdout()) == b"true" {
            return Err(GitError::Unsupported(format!(
                "bare repositories are not supported by ChronoGit v{}",
                env!("CARGO_PKG_VERSION")
            )));
        }
        let worktree = rustix::fs::open(
            root.as_path(),
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|source| filesystem_error("open repository root", source))?;
        Ok(Self {
            runner,
            root,
            worktree,
        })
    }

    /// Returns the absolute root of the discovered working tree.
    #[must_use]
    pub fn root(&self) -> &RepositoryRoot {
        &self.root
    }

    /// Reads unstaged tracked and untracked worktree changes.
    ///
    /// Inclusion follows the worktree side of porcelain-v2 XY status, so a
    /// staged-only path is intentionally omitted.
    ///
    /// # Errors
    ///
    /// Returns an error when Git cannot read status, bounded output is
    /// incomplete, or a porcelain record is malformed.
    pub fn changes(&self) -> Result<Vec<WorktreeChange>, GitError> {
        let output = self.runner.run(Some(&self.root), &GitCommand::Status)?;
        ensure_complete(&output, "read worktree status")?;
        ensure_success(&output, "read worktree status")?;
        parse_status(output.stdout())
    }

    /// Reads at most `limit` commit summaries after skipping `skip` commits.
    ///
    /// An unborn `HEAD` is a valid empty history. Parent order and complete
    /// object IDs are retained for graph rendering and baseline selection.
    ///
    /// # Errors
    ///
    /// Returns an error when Git execution fails, bounded output is incomplete,
    /// or a commit record is malformed.
    pub fn commits(&self, skip: usize, limit: usize) -> Result<Vec<CommitSummary>, GitError> {
        let head = self.runner.run(Some(&self.root), &GitCommand::HasHead)?;
        ensure_complete(&head, "check HEAD")?;
        if !head.success() && head.code() == Some(1) {
            return Ok(Vec::new());
        }
        ensure_success(&head, "check HEAD")?;
        let output = self
            .runner
            .run(Some(&self.root), &GitCommand::Commits { skip, limit })?;
        ensure_complete(&output, "read commit history")?;
        ensure_success(&output, "read commit history")?;
        parse_commits(output.stdout())
    }

    /// Searches tracked and untracked path names using smart-case substring matching.
    ///
    /// Queries containing an uppercase character are case-sensitive; all other
    /// queries use Unicode lowercase matching. An empty query returns every path.
    ///
    /// # Errors
    ///
    /// Returns an error when Git cannot enumerate files, output crosses its
    /// limit, or the NUL-delimited path list is malformed.
    pub fn search_files(&self, query: &str) -> Result<Vec<SearchHit>, GitError> {
        let output = self
            .runner
            .run(Some(&self.root), &GitCommand::RepositoryFiles)?;
        ensure_complete(&output, "list repository files")?;
        ensure_success(&output, "list repository files")?;
        let mut files = parse_file_paths(output.stdout())?;
        let case_sensitive = query.chars().any(char::is_uppercase);
        let folded_query = (!case_sensitive).then(|| query.to_lowercase());
        files.retain(|hit| {
            let path = hit.path().display();
            if case_sensitive {
                path.contains(query)
            } else {
                path.to_lowercase()
                    .contains(folded_query.as_deref().unwrap_or_default())
            }
        });
        Ok(files)
    }

    /// Searches non-binary working-tree contents for literal `query` text.
    ///
    /// An empty query or Git's no-match status returns an empty result rather
    /// than an error. Matches include repository path, one-based line, and a
    /// bounded preview supplied by Git.
    ///
    /// # Errors
    ///
    /// Returns an error for Git execution failures other than no-match,
    /// truncated output, or malformed match records.
    pub fn search_content(&self, query: &str) -> Result<Vec<SearchHit>, GitError> {
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let output = self.runner.run(
            Some(&self.root),
            &GitCommand::Grep {
                query: query.to_owned(),
            },
        )?;
        ensure_complete(&output, "search repository content")?;
        if !output.success() && output.code() == Some(1) && output.stderr().is_empty() {
            return Ok(Vec::new());
        }
        ensure_success(&output, "search repository content")?;
        parse_grep_matches(output.stdout())
    }

    /// Reads at most `limit` commits that touched `path`.
    ///
    /// # Errors
    ///
    /// Returns an error when Git execution fails, output is incomplete, or a
    /// commit record is malformed.
    pub fn file_history(
        &self,
        path: &RepoPath,
        limit: usize,
    ) -> Result<Vec<CommitSummary>, GitError> {
        let output = self.runner.run(
            Some(&self.root),
            &GitCommand::FileHistory {
                path: path.clone(),
                limit,
            },
        )?;
        ensure_complete(&output, "read file history")?;
        ensure_success(&output, "read file history")?;
        parse_commits(output.stdout())
    }

    /// Reads current working-tree content without following symbolic links in
    /// any path component.
    ///
    /// Regular-file reads are capped at 8 MiB. A NUL byte classifies the result
    /// as binary, missing or special files become an unavailable document, and
    /// a symbolic link returns its target instead of opening the target.
    ///
    /// # Errors
    ///
    /// Returns an error when metadata, link-target, open, or read operations fail
    /// for reasons other than a missing path or a rejected intermediate link.
    pub fn file_content(&self, path: &RepoPath) -> Result<FileDocument, GitError> {
        const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;
        let (mut file, size) = match open_current_file(&self.worktree, path)? {
            CurrentFile::Regular { file, size } => (file, size),
            CurrentFile::Symlink { target } => return Ok(FileDocument::Symlink { target }),
            CurrentFile::Unavailable { summary } => {
                return Ok(FileDocument::Unavailable { summary });
            }
        };
        let mut bytes = Vec::with_capacity(size.min(MAX_FILE_BYTES));
        file.by_ref()
            .take((MAX_FILE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|source| GitError::Io {
                operation: "read working tree file",
                source,
            })?;
        let truncated = bytes.len() > MAX_FILE_BYTES;
        bytes.truncate(MAX_FILE_BYTES);
        if bytes.contains(&0) {
            return Ok(FileDocument::Binary {
                summary: format!("Binary file: {}", path.display()),
            });
        }
        let text = bytes.to_str_lossy();
        Ok(FileDocument::Text {
            lines: text.lines().map(ToOwned::to_owned).collect(),
            truncated,
        })
    }

    /// Reads the complete subject and body of `commit`.
    ///
    /// Invalid UTF-8 is replaced only at this presentation-oriented text
    /// boundary; object identifiers remain validated full hexadecimal values.
    ///
    /// # Errors
    ///
    /// Returns an error when Git execution fails or either output stream is
    /// incomplete.
    pub fn commit_message(&self, commit: &ObjectId) -> Result<CommitMessage, GitError> {
        let output = self.runner.run(
            Some(&self.root),
            &GitCommand::CommitMessage {
                commit: commit.clone(),
            },
        )?;
        ensure_complete(&output, "read commit message")?;
        ensure_success(&output, "read commit message")?;
        Ok(CommitMessage::new(
            output.stdout().to_str_lossy().into_owned(),
        ))
    }

    /// Lists paths changed by `commit` relative to `baseline`.
    ///
    /// # Errors
    ///
    /// Returns an error when Git execution fails, output is incomplete, or a
    /// NUL-delimited name-status record is malformed.
    pub fn changed_files(
        &self,
        commit: &ObjectId,
        baseline: &CommitBaseline,
    ) -> Result<Vec<ChangedFile>, GitError> {
        let output = self.runner.run(
            Some(&self.root),
            &GitCommand::ChangedFiles {
                commit: commit.clone(),
                baseline: baseline.clone(),
            },
        )?;
        ensure_complete(&output, "read changed files")?;
        ensure_success(&output, "read changed files")?;
        parse_changed_files(output.stdout())
    }

    /// Reads and parses a bounded patch for a worktree or commit target.
    ///
    /// Text standard output that crosses 8 MiB becomes
    /// [`DiffDocument::Truncated`] so a useful prefix remains visible. Truncated
    /// standard error and unsuccessful commands remain failures.
    ///
    /// # Errors
    ///
    /// Returns an error when the runner or Git fails, or when diagnostic output
    /// is incomplete. Exit status 1 is accepted only for the expected
    /// `--no-index` difference produced for an untracked path.
    pub fn diff(&self, target: &DiffTarget) -> Result<DiffDocument, GitError> {
        let command = match target {
            DiffTarget::Worktree { path, untracked } if *untracked => {
                GitCommand::UntrackedDiff { path: path.clone() }
            }
            DiffTarget::Worktree { path, .. } => GitCommand::WorktreeDiff { path: path.clone() },
            DiffTarget::Commit {
                commit,
                baseline,
                path,
            } => GitCommand::CommitDiff {
                commit: commit.clone(),
                baseline: baseline.clone(),
                path: path.clone(),
            },
        };
        let output = self.runner.run(Some(&self.root), &command)?;
        if output.stderr_truncated() {
            return Err(GitError::OutputLimit {
                operation: command.kind(),
            });
        }
        let allowed_no_index_difference = matches!(command, GitCommand::UntrackedDiff { .. })
            && output.code() == Some(1)
            && output.stderr().is_empty();
        if !output.success() && !allowed_no_index_difference && !output.stdout_truncated() {
            return Err(command_failed(&output, command.kind()));
        }
        Ok(parse_patch(output.stdout(), output.stdout_truncated()))
    }

    /// Reads the direct children of one commit or tree object.
    ///
    /// Recursion is deliberately left to the application so directories can be
    /// expanded lazily.
    ///
    /// # Errors
    ///
    /// Returns an error when Git execution fails, output is incomplete, or an
    /// `ls-tree` record is malformed.
    pub fn tree_entries(&self, treeish: &ObjectId) -> Result<Vec<TreeEntry>, GitError> {
        let output = self.runner.run(
            Some(&self.root),
            &GitCommand::TreeEntries {
                treeish: treeish.clone(),
            },
        )?;
        ensure_complete(&output, "read tree entries")?;
        ensure_success(&output, "read tree entries")?;
        parse_tree_entries(output.stdout())
    }
}

enum CurrentFile {
    Regular { file: File, size: usize },
    Symlink { target: String },
    Unavailable { summary: String },
}

fn open_current_file(worktree: &OwnedFd, path: &RepoPath) -> Result<CurrentFile, GitError> {
    let directory_flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut directory = rustix::io::dup(worktree)
        .map_err(|source| filesystem_error("duplicate repository root", source))?;

    let mut components = path.as_bytes().split(|byte| *byte == b'/');
    let final_component = components
        .next_back()
        .ok_or_else(|| GitError::parse("working tree path", "path is empty"))?;
    for component in components {
        let name = OsStr::from_bytes(component);
        directory = match rustix::fs::openat(&directory, name, directory_flags, Mode::empty()) {
            Ok(descriptor) => descriptor,
            Err(Errno::NOENT) => {
                return Ok(CurrentFile::Unavailable {
                    summary: format!(
                        "File does not exist in the current working tree: {}",
                        path.display()
                    ),
                });
            }
            Err(Errno::LOOP | Errno::NOTDIR) => {
                return Ok(CurrentFile::Unavailable {
                    summary: format!(
                        "Path contains a symbolic link or non-directory component: {}",
                        path.display()
                    ),
                });
            }
            Err(source) => {
                return Err(filesystem_error(
                    "open working tree directory component",
                    source,
                ));
            }
        };
    }

    let final_name = OsStr::from_bytes(final_component);
    let descriptor = match rustix::fs::openat(
        &directory,
        final_name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(descriptor) => descriptor,
        Err(Errno::LOOP) => {
            let target = rustix::fs::readlinkat(&directory, final_name, Vec::new())
                .map_err(|source| filesystem_error("read working tree symlink", source))?;
            let target = OsString::from_vec(target.as_bytes().to_vec());
            return Ok(CurrentFile::Symlink {
                target: format!("symlink → {}", target.to_string_lossy()),
            });
        }
        Err(Errno::NOENT) => {
            return Ok(CurrentFile::Unavailable {
                summary: format!(
                    "File does not exist in the current working tree: {}",
                    path.display()
                ),
            });
        }
        Err(Errno::NOTDIR) => {
            return Ok(CurrentFile::Unavailable {
                summary: format!("Not a regular working-tree file: {}", path.display()),
            });
        }
        Err(source) => return Err(filesystem_error("open working tree file", source)),
    };

    let metadata = rustix::fs::fstat(&descriptor)
        .map_err(|source| filesystem_error("read working tree file metadata", source))?;
    if !FileType::from_raw_mode(metadata.st_mode).is_file() {
        return Ok(CurrentFile::Unavailable {
            summary: format!("Not a regular working-tree file: {}", path.display()),
        });
    }
    Ok(CurrentFile::Regular {
        file: File::from(descriptor),
        size: usize::try_from(metadata.st_size).unwrap_or(usize::MAX),
    })
}

fn filesystem_error(operation: &'static str, source: Errno) -> GitError {
    GitError::Io {
        operation,
        source: source.into(),
    }
}

fn ensure_success(output: &CommandOutput, operation: &'static str) -> Result<(), GitError> {
    if output.success() {
        Ok(())
    } else {
        Err(command_failed(output, operation))
    }
}

fn ensure_complete(output: &CommandOutput, operation: &'static str) -> Result<(), GitError> {
    if output.stdout_truncated() || output.stderr_truncated() {
        Err(GitError::OutputLimit { operation })
    } else {
        Ok(())
    }
}

fn command_failed(output: &CommandOutput, operation: &'static str) -> GitError {
    GitError::CommandFailed {
        operation,
        code: output.code(),
        stderr: output.stderr().to_str_lossy().into_owned(),
    }
}

fn trim_newline(mut value: &[u8]) -> &[u8] {
    while value
        .last()
        .is_some_and(|byte| matches!(byte, b'\n' | b'\r'))
    {
        value = &value[..value.len() - 1];
    }
    value
}
