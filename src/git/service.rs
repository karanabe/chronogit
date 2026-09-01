use std::ffi::OsString;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;

use bstr::ByteSlice;

use crate::domain::{
    ChangedFile, CommitBaseline, CommitMessage, CommitSummary, DiffDocument, DiffTarget,
    FileDocument, ObjectId, RepoPath, RepositoryRoot, SearchHit, TreeEntry, WorktreeChange,
};
use crate::git::parse::{
    parse_changed_files, parse_commits, parse_file_paths, parse_grep_matches, parse_patch,
    parse_status, parse_tree_entries,
};
use crate::git::{CommandOutput, GitCommand, GitError, GitRunner};

#[derive(Debug)]
pub struct GitService<R> {
    runner: R,
    root: RepositoryRoot,
}

impl<R: GitRunner> GitService<R> {
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
        let service = Self { runner, root };
        let bare = service
            .runner
            .run(Some(&service.root), &GitCommand::IsBare)?;
        ensure_complete(&bare, "check bare repository")?;
        ensure_success(&bare, "check bare repository")?;
        if trim_newline(bare.stdout()) == b"true" {
            return Err(GitError::Unsupported(
                "bare repositories are not supported by ChronoGit v0.1.0".to_owned(),
            ));
        }
        Ok(service)
    }

    #[must_use]
    pub fn root(&self) -> &RepositoryRoot {
        &self.root
    }

    pub fn changes(&self) -> Result<Vec<WorktreeChange>, GitError> {
        let output = self.runner.run(Some(&self.root), &GitCommand::Status)?;
        ensure_complete(&output, "read worktree status")?;
        ensure_success(&output, "read worktree status")?;
        parse_status(output.stdout())
    }

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

    pub fn file_content(&self, path: &RepoPath) -> Result<FileDocument, GitError> {
        const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;
        let absolute = self.root.as_path().join(path.to_os_string());
        let metadata = match fs::symlink_metadata(&absolute) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(FileDocument::Unavailable {
                    summary: format!(
                        "File does not exist in the current working tree: {}",
                        path.display()
                    ),
                });
            }
            Err(source) => {
                return Err(GitError::Io {
                    operation: "read working tree file metadata",
                    source,
                });
            }
        };
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(&absolute).map_err(|source| GitError::Io {
                operation: "read working tree symlink",
                source,
            })?;
            return Ok(FileDocument::Symlink {
                target: format!("symlink → {}", target.display()),
            });
        }
        if !metadata.is_file() {
            return Ok(FileDocument::Unavailable {
                summary: format!("Not a regular working-tree file: {}", path.display()),
            });
        }
        let mut file = File::open(&absolute).map_err(|source| GitError::Io {
            operation: "open working tree file",
            source,
        })?;
        let mut bytes = Vec::with_capacity(
            usize::try_from(metadata.len())
                .unwrap_or(MAX_FILE_BYTES)
                .min(MAX_FILE_BYTES),
        );
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
