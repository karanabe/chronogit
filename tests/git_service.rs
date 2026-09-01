use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;

use chronogit::domain::{
    ChangeKind, CommitBaseline, DiffDocument, DiffTarget, FileDocument, TreeKind,
};
use chronogit::git::{GitService, SystemGitRunner};
use tempfile::TempDir;

struct TestRepository {
    directory: TempDir,
}

impl TestRepository {
    fn new() -> Self {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("could not create temporary directory: {error}"));
        let repository = Self { directory };
        repository.git(&["init", "-b", "main"]);
        repository.git(&["config", "user.name", "ChronoGit Tests"]);
        repository.git(&["config", "user.email", "chronogit@example.invalid"]);
        repository
    }

    fn path(&self) -> &Path {
        self.directory.path()
    }

    fn write(&self, relative: &str, content: &[u8]) {
        let path = self.path().join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|error| panic!("could not create {}: {error}", parent.display()));
        }
        fs::write(&path, content)
            .unwrap_or_else(|error| panic!("could not write {}: {error}", path.display()));
    }

    fn git(&self, arguments: &[&str]) -> Output {
        let output = self.git_output(arguments);
        assert!(
            output.status.success(),
            "git {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn git_output(&self, arguments: &[&str]) -> Output {
        Command::new("git")
            .arg("-C")
            .arg(self.path())
            .args(arguments)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap_or_else(|error| panic!("could not execute Git: {error}"))
    }

    fn service(&self) -> GitService<SystemGitRunner> {
        GitService::discover(SystemGitRunner, self.path())
            .unwrap_or_else(|error| panic!("could not discover test repository: {error}"))
    }

    fn commit_all(&self, message: &str) {
        self.git(&["add", "--all"]);
        self.git(&["commit", "-m", message]);
    }
}

#[test]
fn excludes_staged_only_and_shows_only_the_unstaged_part_of_mixed_changes() {
    let repository = TestRepository::new();
    repository.write("mixed.txt", b"version 1\n");
    repository.write("staged-only.txt", b"version 1\n");
    repository.commit_all("root");
    repository.write("mixed.txt", b"version 2\n");
    repository.write("staged-only.txt", b"version 2\n");
    repository.git(&["add", "mixed.txt", "staged-only.txt"]);
    repository.write("mixed.txt", b"version 3\n");

    let service = repository.service();
    let changes = service
        .changes()
        .unwrap_or_else(|error| panic!("could not read mixed status: {error}"));
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].path().display(), "mixed.txt");
    let diff = service
        .diff(&DiffTarget::Worktree {
            path: changes[0].path().clone(),
            untracked: false,
        })
        .unwrap_or_else(|error| panic!("could not read mixed diff: {error}"));
    let text = diff
        .lines()
        .iter()
        .map(|line| line.text())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("-version 2"));
    assert!(text.contains("+version 3"));
    assert!(!text.contains("version 1"));
}

#[test]
fn reports_conflicts_as_unmerged_changes() {
    let repository = TestRepository::new();
    repository.write("conflict.txt", b"base\n");
    repository.commit_all("root");
    repository.git(&["switch", "-c", "feature"]);
    repository.write("conflict.txt", b"feature\n");
    repository.commit_all("feature");
    repository.git(&["switch", "main"]);
    repository.write("conflict.txt", b"main\n");
    repository.commit_all("main");
    let merge = repository.git_output(&["merge", "feature"]);
    assert!(!merge.status.success(), "merge should conflict");

    let service = repository.service();
    let changes = service
        .changes()
        .unwrap_or_else(|error| panic!("could not read conflict status: {error}"));
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind(), ChangeKind::Unmerged);
    let diff = service
        .diff(&DiffTarget::Worktree {
            path: changes[0].path().clone(),
            untracked: false,
        })
        .unwrap_or_else(|error| panic!("could not read conflict diff: {error}"));
    assert!(matches!(diff, DiffDocument::Text { .. }));
}

#[test]
fn identifies_binary_and_truncates_oversized_diffs() {
    let repository = TestRepository::new();
    repository.write("binary.bin", &[0, 1, 2, 3]);
    repository.commit_all("binary root");
    repository.write("binary.bin", &[0, 1, 9, 3]);
    let service = repository.service();
    let binary = service
        .diff(&DiffTarget::Worktree {
            path: chronogit::domain::RepoPath::from_bytes(b"binary.bin".to_vec())
                .unwrap_or_else(|error| panic!("invalid path: {error}")),
            untracked: false,
        })
        .unwrap_or_else(|error| panic!("could not read binary diff: {error}"));
    assert!(matches!(binary, DiffDocument::Binary { .. }));

    repository.commit_all("binary update");
    let commit = service
        .commits(0, 1)
        .unwrap_or_else(|error| panic!("could not read binary commit: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("binary commit should exist"));
    let file = service
        .changed_files(commit.id(), &commit.baseline())
        .unwrap_or_else(|error| panic!("could not read binary commit files: {error}"))
        .into_iter()
        .find(|file| file.path().display() == "binary.bin")
        .unwrap_or_else(|| panic!("binary commit should contain binary.bin"));
    let committed_binary = service
        .diff(&DiffTarget::Commit {
            commit: commit.id().clone(),
            baseline: commit.baseline(),
            path: file.path().clone(),
        })
        .unwrap_or_else(|error| panic!("could not read committed binary diff: {error}"));
    assert!(matches!(committed_binary, DiffDocument::Binary { .. }));

    let mut large = vec![b'x'; 9 * 1024 * 1024];
    large.push(b'\n');
    repository.write("large.txt", &large);
    let oversized = service
        .diff(&DiffTarget::Worktree {
            path: chronogit::domain::RepoPath::from_bytes(b"large.txt".to_vec())
                .unwrap_or_else(|error| panic!("invalid path: {error}")),
            untracked: true,
        })
        .unwrap_or_else(|error| panic!("could not read oversized diff: {error}"));
    assert!(oversized.is_truncated());
}

#[test]
fn all_read_operations_preserve_head_status_and_worktree() {
    let repository = TestRepository::new();
    repository.write("tracked.txt", b"before\n");
    repository.commit_all("root");
    repository.write("tracked.txt", b"after\n");
    let before_head = repository.git(&["rev-parse", "HEAD"]).stdout;
    let before_status = repository.git(&["status", "--porcelain=v2", "-z"]).stdout;
    let before_file = fs::read(repository.path().join("tracked.txt"))
        .unwrap_or_else(|error| panic!("could not read worktree: {error}"));

    let service = repository.service();
    let changes = service
        .changes()
        .unwrap_or_else(|error| panic!("could not read changes: {error}"));
    let commits = service
        .commits(0, 10)
        .unwrap_or_else(|error| panic!("could not read commits: {error}"));
    let commit = &commits[0];
    let _message = service
        .commit_message(commit.id())
        .unwrap_or_else(|error| panic!("could not read message: {error}"));
    let files = service
        .changed_files(commit.id(), &commit.baseline())
        .unwrap_or_else(|error| panic!("could not read files: {error}"));
    let _tree = service
        .tree_entries(commit.id())
        .unwrap_or_else(|error| panic!("could not read tree: {error}"));
    let search_files = service
        .search_files("tracked")
        .unwrap_or_else(|error| panic!("could not search files: {error}"));
    let tracked_path = search_files
        .first()
        .map(|hit| hit.path().clone())
        .unwrap_or_else(|| panic!("tracked file should be searchable"));
    let _content_matches = service
        .search_content("after")
        .unwrap_or_else(|error| panic!("could not search content: {error}"));
    let _file_history = service
        .file_history(&tracked_path, 20)
        .unwrap_or_else(|error| panic!("could not read file history: {error}"));
    let _file_content = service
        .file_content(&tracked_path)
        .unwrap_or_else(|error| panic!("could not read file content: {error}"));
    let _worktree_diff = service
        .diff(&DiffTarget::Worktree {
            path: changes[0].path().clone(),
            untracked: false,
        })
        .unwrap_or_else(|error| panic!("could not read worktree diff: {error}"));
    let _commit_diff = service
        .diff(&DiffTarget::Commit {
            commit: commit.id().clone(),
            baseline: commit.baseline(),
            path: files[0].path().clone(),
        })
        .unwrap_or_else(|error| panic!("could not read commit diff: {error}"));

    assert_eq!(repository.git(&["rev-parse", "HEAD"]).stdout, before_head);
    assert_eq!(
        repository.git(&["status", "--porcelain=v2", "-z"]).stdout,
        before_status
    );
    assert_eq!(
        fs::read(repository.path().join("tracked.txt"))
            .unwrap_or_else(|error| panic!("could not re-read worktree: {error}")),
        before_file
    );
}

#[test]
fn searches_files_and_content_and_reads_file_history_and_current_content() {
    let repository = TestRepository::new();
    repository.write("src/needle.rs", b"fn first() { /* searchable needle */ }\n");
    repository.write("notes.txt", b"unrelated\n");
    repository.commit_all("add searchable file");
    repository.write(
        "src/needle.rs",
        b"fn second() { /* searchable needle */ }\n",
    );
    repository.commit_all("update searchable file");
    repository.write("untracked-needle.txt", b"searchable needle in worktree\n");

    let service = repository.service();
    let files = service
        .search_files("Needle")
        .unwrap_or_else(|error| panic!("could not search files: {error}"));
    assert!(
        files.is_empty(),
        "uppercase file search must be case-sensitive"
    );
    let files = service
        .search_files("needle")
        .unwrap_or_else(|error| panic!("could not search files: {error}"));
    assert_eq!(files.len(), 2);
    assert!(
        files
            .iter()
            .any(|hit| hit.path().display() == "untracked-needle.txt")
    );

    let matches = service
        .search_content("searchable needle")
        .unwrap_or_else(|error| panic!("could not grep repository: {error}"));
    assert_eq!(matches.len(), 2);
    assert!(matches.iter().all(|hit| hit.line() == Some(1)));

    let path = chronogit::domain::RepoPath::from_bytes(b"src/needle.rs".to_vec())
        .unwrap_or_else(|error| panic!("invalid path: {error}"));
    let history = service
        .file_history(&path, 20)
        .unwrap_or_else(|error| panic!("could not read file history: {error}"));
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].subject(), "update searchable file");

    let content = service
        .file_content(&path)
        .unwrap_or_else(|error| panic!("could not read current content: {error}"));
    assert_eq!(content.lines(), ["fn second() { /* searchable needle */ }"]);

    fs::remove_file(repository.path().join("notes.txt"))
        .unwrap_or_else(|error| panic!("could not remove fixture file: {error}"));
    let deleted = chronogit::domain::RepoPath::from_bytes(b"notes.txt".to_vec())
        .unwrap_or_else(|error| panic!("invalid path: {error}"));
    assert!(matches!(
        service.file_content(&deleted),
        Ok(FileDocument::Unavailable { .. })
    ));
}

#[cfg(unix)]
#[test]
fn current_file_reads_never_follow_symlinks_outside_the_worktree() {
    use std::os::unix::fs::symlink;

    let repository = TestRepository::new();
    repository.write("linked/secret.txt", b"indexed content\n");
    repository.commit_all("track nested file");

    let external = tempfile::tempdir()
        .unwrap_or_else(|error| panic!("could not create external directory: {error}"));
    let external_file = external.path().join("secret.txt");
    fs::write(&external_file, b"outside content\n")
        .unwrap_or_else(|error| panic!("could not write external file: {error}"));
    fs::remove_dir_all(repository.path().join("linked"))
        .unwrap_or_else(|error| panic!("could not replace tracked directory: {error}"));
    symlink(external.path(), repository.path().join("linked"))
        .unwrap_or_else(|error| panic!("could not create directory symlink: {error}"));

    let service = repository.service();
    let nested_path = chronogit::domain::RepoPath::from_bytes(b"linked/secret.txt".to_vec())
        .unwrap_or_else(|error| panic!("invalid nested path: {error}"));
    let nested = service
        .file_content(&nested_path)
        .unwrap_or_else(|error| panic!("could not inspect nested symlink path: {error}"));
    assert!(matches!(nested, FileDocument::Unavailable { .. }));

    symlink(&external_file, repository.path().join("direct-link"))
        .unwrap_or_else(|error| panic!("could not create file symlink: {error}"));
    let direct_path = chronogit::domain::RepoPath::from_bytes(b"direct-link".to_vec())
        .unwrap_or_else(|error| panic!("invalid direct path: {error}"));
    let direct = service
        .file_content(&direct_path)
        .unwrap_or_else(|error| panic!("could not inspect direct symlink: {error}"));
    assert!(matches!(direct, FileDocument::Symlink { .. }));
}

#[test]
fn discovers_linked_worktree_root() {
    let repository = TestRepository::new();
    repository.write("tracked.txt", b"content\n");
    repository.commit_all("root");
    let linked = tempfile::tempdir()
        .unwrap_or_else(|error| panic!("could not create linked worktree directory: {error}"));
    let linked_path = linked.path().join("worktree");
    let linked_text = linked_path
        .to_str()
        .unwrap_or_else(|| panic!("temporary path should be UTF-8"));
    repository.git(&["worktree", "add", "-b", "linked", linked_text]);
    let nested = linked_path.join("nested");
    fs::create_dir(&nested)
        .unwrap_or_else(|error| panic!("could not create nested worktree path: {error}"));
    let service = GitService::discover(SystemGitRunner, &nested)
        .unwrap_or_else(|error| panic!("could not discover linked worktree: {error}"));
    assert_eq!(
        service.root().as_path(),
        linked_path
            .canonicalize()
            .unwrap_or_else(|error| panic!("could not canonicalize linked path: {error}"))
    );
}

#[test]
fn handles_leading_dash_rename_delete_and_normal_commit_diffs() {
    let repository = TestRepository::new();
    repository.write("old name.txt", b"old\n");
    repository.write("delete-me.txt", b"delete\n");
    repository.commit_all("root");
    repository.git(&["mv", "old name.txt", "new name.txt"]);
    repository.git(&["rm", "delete-me.txt"]);
    repository.write("-danger.txt", b"safe path argument\n");

    let service = repository.service();
    let changes = service
        .changes()
        .unwrap_or_else(|error| panic!("could not read leading-dash path: {error}"));
    let dangerous = changes
        .iter()
        .find(|change| change.path().display() == "-danger.txt")
        .unwrap_or_else(|| panic!("leading-dash path should be listed"));
    let dangerous_diff = service
        .diff(&DiffTarget::Worktree {
            path: dangerous.path().clone(),
            untracked: true,
        })
        .unwrap_or_else(|error| panic!("could not diff leading-dash path: {error}"));
    assert!(matches!(dangerous_diff, DiffDocument::Text { .. }));

    repository.commit_all("rename and delete");
    let commits = service
        .commits(0, 10)
        .unwrap_or_else(|error| panic!("could not read normal commit: {error}"));
    let commit = &commits[0];
    let files = service
        .changed_files(commit.id(), &commit.baseline())
        .unwrap_or_else(|error| panic!("could not read rename/delete files: {error}"));
    let renamed = files
        .iter()
        .find(|file| file.kind() == ChangeKind::Renamed)
        .unwrap_or_else(|| panic!("expected renamed file"));
    assert_eq!(renamed.path().display(), "new name.txt");
    assert_eq!(
        renamed.original_path().map(|path| path.display()),
        Some("old name.txt".to_owned())
    );
    assert!(files.iter().any(|file| file.kind() == ChangeKind::Deleted));
    let diff = service
        .diff(&DiffTarget::Commit {
            commit: commit.id().clone(),
            baseline: commit.baseline(),
            path: renamed.path().clone(),
        })
        .unwrap_or_else(|error| panic!("could not read rename diff: {error}"));
    assert!(matches!(diff, DiffDocument::Text { .. }));
}

#[test]
fn detects_copies_from_a_modified_source() {
    let repository = TestRepository::new();
    repository.write(
        "source.txt",
        b"one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n",
    );
    repository.commit_all("root");
    repository.write(
        "copy.txt",
        b"one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n",
    );
    repository.write(
        "source.txt",
        b"changed\none\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n",
    );
    repository.commit_all("copy source");

    let service = repository.service();
    let commit = service
        .commits(0, 1)
        .unwrap_or_else(|error| panic!("could not read copy commit: {error}"))
        .remove(0);
    let files = service
        .changed_files(commit.id(), &commit.baseline())
        .unwrap_or_else(|error| panic!("could not detect copy: {error}"));
    let copied = files
        .iter()
        .find(|file| file.kind() == ChangeKind::Copied)
        .unwrap_or_else(|| panic!("expected copied file, got {files:?}"));
    assert_eq!(copied.path().display(), "copy.txt");
    assert_eq!(
        copied.original_path().map(|path| path.display()),
        Some("source.txt".to_owned())
    );
}

#[test]
fn reads_multiline_and_empty_commit_messages() {
    let repository = TestRepository::new();
    repository.write("message.txt", b"one\n");
    repository.git(&["add", "--all"]);
    repository.git(&[
        "commit",
        "-m",
        "subject",
        "-m",
        "body line one\nbody line two",
    ]);
    let service = repository.service();
    let multiline = service
        .commits(0, 1)
        .unwrap_or_else(|error| panic!("could not read multiline commit: {error}"))
        .remove(0);
    let message = service
        .commit_message(multiline.id())
        .unwrap_or_else(|error| panic!("could not read multiline message: {error}"));
    assert!(message.as_str().contains("body line one\nbody line two"));

    repository.write("message.txt", b"two\n");
    repository.git(&["add", "--all"]);
    repository.git(&["commit", "--allow-empty-message", "-m", ""]);
    let empty = service
        .commits(0, 1)
        .unwrap_or_else(|error| panic!("could not read empty-message commit: {error}"))
        .remove(0);
    let message = service
        .commit_message(empty.id())
        .unwrap_or_else(|error| panic!("could not read empty message: {error}"));
    assert!(message.as_str().trim().is_empty());
}

#[test]
fn reads_empty_deep_unicode_and_large_trees_lazily() {
    let empty_repository = TestRepository::new();
    empty_repository.git(&["commit", "--allow-empty", "-m", "empty tree"]);
    let empty_service = empty_repository.service();
    let empty_commit = empty_service
        .commits(0, 1)
        .unwrap_or_else(|error| panic!("could not read empty-tree commit: {error}"))
        .remove(0);
    assert!(
        empty_service
            .tree_entries(empty_commit.id())
            .unwrap_or_else(|error| panic!("could not read empty tree: {error}"))
            .is_empty()
    );

    let repository = TestRepository::new();
    let deep_path = format!(
        "{}/leaf.txt",
        (0..16)
            .map(|i| format!("d{i}"))
            .collect::<Vec<_>>()
            .join("/")
    );
    repository.write(&deep_path, b"deep\n");
    repository.write("日本語/履歴.txt", b"unicode\n");
    for index in 0..512 {
        repository.write(&format!("large/file-{index:04}.txt"), b"entry\n");
    }
    repository.commit_all("large deep tree");

    let service = repository.service();
    let commit = service
        .commits(0, 1)
        .unwrap_or_else(|error| panic!("could not read tree commit: {error}"))
        .remove(0);
    let root = service
        .tree_entries(commit.id())
        .unwrap_or_else(|error| panic!("could not read root tree: {error}"));
    assert!(root.iter().any(|entry| entry.name().display() == "日本語"));
    let large = root
        .iter()
        .find(|entry| entry.name().display() == "large")
        .unwrap_or_else(|| panic!("expected large directory"));
    assert_eq!(
        service
            .tree_entries(large.object_id())
            .unwrap_or_else(|error| panic!("could not read large directory: {error}"))
            .len(),
        512
    );

    let mut directory = root
        .iter()
        .find(|entry| entry.name().display() == "d0")
        .unwrap_or_else(|| panic!("expected deep directory"))
        .object_id()
        .clone();
    for depth in 1..16 {
        let children = service
            .tree_entries(&directory)
            .unwrap_or_else(|error| panic!("could not read depth {depth}: {error}"));
        directory = children
            .iter()
            .find(|entry| entry.name().display() == format!("d{depth}"))
            .unwrap_or_else(|| panic!("expected directory at depth {depth}"))
            .object_id()
            .clone();
    }
    let leaf = service
        .tree_entries(&directory)
        .unwrap_or_else(|error| panic!("could not read deep leaf: {error}"));
    assert_eq!(leaf[0].name().display(), "leaf.txt");
}

#[cfg(unix)]
#[test]
fn identifies_type_changes_symlinks_and_submodules() {
    use std::os::unix::fs::symlink;

    let repository = TestRepository::new();
    repository.write("target.txt", b"target\n");
    repository.write("kind.txt", b"regular\n");
    repository.commit_all("root");
    fs::remove_file(repository.path().join("kind.txt"))
        .unwrap_or_else(|error| panic!("could not remove regular file: {error}"));
    symlink("target.txt", repository.path().join("kind.txt"))
        .unwrap_or_else(|error| panic!("could not create symlink: {error}"));
    let service = repository.service();
    let changes = service
        .changes()
        .unwrap_or_else(|error| panic!("could not read type change: {error}"));
    assert!(
        changes
            .iter()
            .any(|change| change.kind() == ChangeKind::TypeChanged)
    );
    repository.commit_all("symlink");

    let head = repository.git(&["rev-parse", "HEAD"]);
    let head_text = String::from_utf8_lossy(&head.stdout).trim().to_owned();
    repository.git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("160000,{head_text},vendor/module"),
    ]);
    repository.git(&["commit", "-m", "gitlink"]);
    let commits = service
        .commits(0, 10)
        .unwrap_or_else(|error| panic!("could not read gitlink commit: {error}"));
    let tree = service
        .tree_entries(commits[0].id())
        .unwrap_or_else(|error| panic!("could not read tree kinds: {error}"));
    assert!(tree.iter().any(|entry| entry.kind() == TreeKind::Symlink));
    let vendor = tree
        .iter()
        .find(|entry| entry.name().display() == "vendor")
        .unwrap_or_else(|| panic!("expected vendor tree"));
    let vendor_children = service
        .tree_entries(vendor.object_id())
        .unwrap_or_else(|error| panic!("could not read vendor tree: {error}"));
    assert_eq!(vendor_children[0].kind(), TreeKind::Submodule);
}

#[test]
fn reads_worktree_history_message_diff_and_tree() {
    let repository = TestRepository::new();
    repository.write("src/lib.rs", b"pub fn value() -> u8 { 1 }\n");
    repository.commit_all("root commit");
    repository.write("src/lib.rs", b"pub fn value() -> u8 { 2 }\n");
    repository.write("space name.txt", b"untracked\n");

    let service = repository.service();
    let changes = service
        .changes()
        .unwrap_or_else(|error| panic!("could not read changes: {error}"));
    assert_eq!(changes.len(), 2);
    assert!(
        changes
            .iter()
            .any(|change| change.kind() == ChangeKind::Modified)
    );
    let untracked = changes
        .iter()
        .find(|change| change.kind() == ChangeKind::Untracked)
        .unwrap_or_else(|| panic!("expected untracked change"));
    let untracked_diff = service
        .diff(&DiffTarget::Worktree {
            path: untracked.path().clone(),
            untracked: true,
        })
        .unwrap_or_else(|error| panic!("could not read untracked diff: {error}"));
    assert!(matches!(untracked_diff, DiffDocument::Text { .. }));

    let commits = service
        .commits(0, 200)
        .unwrap_or_else(|error| panic!("could not read commits: {error}"));
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].baseline(), CommitBaseline::EmptyTree);
    let message = service
        .commit_message(commits[0].id())
        .unwrap_or_else(|error| panic!("could not read message: {error}"));
    assert_eq!(message.as_str().trim(), "root commit");
    let files = service
        .changed_files(commits[0].id(), &commits[0].baseline())
        .unwrap_or_else(|error| panic!("could not read changed files: {error}"));
    assert_eq!(files.len(), 1);
    let root_diff = service
        .diff(&DiffTarget::Commit {
            commit: commits[0].id().clone(),
            baseline: commits[0].baseline(),
            path: files[0].path().clone(),
        })
        .unwrap_or_else(|error| panic!("could not read root diff: {error}"));
    assert!(matches!(root_diff, DiffDocument::Text { .. }));
    let tree = service
        .tree_entries(commits[0].id())
        .unwrap_or_else(|error| panic!("could not read tree: {error}"));
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].kind(), TreeKind::Directory);
    let children = service
        .tree_entries(tree[0].object_id())
        .unwrap_or_else(|error| panic!("could not read child tree: {error}"));
    assert_eq!(children[0].name().display(), "lib.rs");
}

#[test]
fn preserves_space_tab_and_unicode_paths() {
    let repository = TestRepository::new();
    for path in ["space name.txt", "tab\tname.txt", "日本語.txt"] {
        repository.write(path, b"path content\n");
    }
    let service = repository.service();
    let changes = service
        .changes()
        .unwrap_or_else(|error| panic!("could not read unusual paths: {error}"));
    let paths = changes
        .iter()
        .map(|change| change.path().display())
        .collect::<Vec<_>>();
    assert!(paths.iter().any(|path| path == "space name.txt"));
    assert!(paths.iter().any(|path| path == "tab\tname.txt"));
    assert!(paths.iter().any(|path| path == "日本語.txt"));
    for change in &changes {
        let diff = service
            .diff(&DiffTarget::Worktree {
                path: change.path().clone(),
                untracked: true,
            })
            .unwrap_or_else(|error| panic!("could not diff unusual path: {error}"));
        assert!(matches!(diff, DiffDocument::Text { .. }));
    }
}

#[test]
fn merge_commit_uses_first_parent() {
    let repository = TestRepository::new();
    repository.write("base.txt", b"base\n");
    repository.commit_all("root");
    repository.git(&["switch", "-c", "feature"]);
    repository.write("feature.txt", b"feature\n");
    repository.commit_all("feature");
    repository.git(&["switch", "main"]);
    repository.write("main.txt", b"main\n");
    repository.commit_all("main");
    repository.git(&["merge", "--no-ff", "feature", "-m", "merge feature"]);

    let service = repository.service();
    let commits = service
        .commits(0, 10)
        .unwrap_or_else(|error| panic!("could not read commits: {error}"));
    let merge = &commits[0];
    assert_eq!(merge.parents().len(), 2);
    let CommitBaseline::FirstParent(parent) = merge.baseline() else {
        panic!("merge should use first parent");
    };
    assert_eq!(parent, merge.parents()[0]);
    let files = service
        .changed_files(merge.id(), &merge.baseline())
        .unwrap_or_else(|error| panic!("could not read merge files: {error}"));
    assert!(
        files
            .iter()
            .any(|file| file.path().display() == "feature.txt")
    );
}

#[test]
fn reads_unborn_repository_and_preserves_semantic_state() {
    let repository = TestRepository::new();
    repository.write("untracked.txt", b"content\n");
    let service = repository.service();
    let before_status = repository.git(&["status", "--porcelain=v2", "-z"]).stdout;
    let commits = service
        .commits(0, 10)
        .unwrap_or_else(|error| panic!("could not read unborn history: {error}"));
    assert!(commits.is_empty());
    let changes = service
        .changes()
        .unwrap_or_else(|error| panic!("could not read unborn changes: {error}"));
    assert_eq!(changes.len(), 1);
    assert_eq!(
        service
            .search_files("untracked")
            .unwrap_or_else(|error| panic!("could not search unborn files: {error}"))
            .len(),
        1
    );
    assert_eq!(
        service
            .search_content("content")
            .unwrap_or_else(|error| panic!("could not search unborn content: {error}"))
            .len(),
        1
    );
    let _diff = service
        .diff(&DiffTarget::Worktree {
            path: changes[0].path().clone(),
            untracked: true,
        })
        .unwrap_or_else(|error| panic!("could not read diff: {error}"));
    let after_status = repository.git(&["status", "--porcelain=v2", "-z"]).stdout;
    assert_eq!(before_status, after_status);
    assert_eq!(
        fs::read(repository.path().join("untracked.txt"))
            .unwrap_or_else(|error| panic!("could not read worktree file: {error}")),
        b"content\n"
    );
}

#[test]
fn reads_detached_head_and_intent_to_add_changes() {
    let repository = TestRepository::new();
    repository.write("tracked.txt", b"root\n");
    repository.commit_all("root");
    repository.git(&["switch", "--detach"]);
    repository.write("intent.txt", b"intent to add\n");
    repository.git(&["add", "--intent-to-add", "intent.txt"]);

    let service = repository.service();
    let commits = service
        .commits(0, 10)
        .unwrap_or_else(|error| panic!("could not read detached history: {error}"));
    assert_eq!(commits.len(), 1);
    let changes = service
        .changes()
        .unwrap_or_else(|error| panic!("could not read intent-to-add status: {error}"));
    let intent = changes
        .iter()
        .find(|change| change.path().display() == "intent.txt")
        .unwrap_or_else(|| panic!("intent-to-add file should be listed"));
    assert_eq!(intent.kind(), ChangeKind::Added);
    let diff = service
        .diff(&DiffTarget::Worktree {
            path: intent.path().clone(),
            untracked: false,
        })
        .unwrap_or_else(|error| panic!("could not read intent-to-add diff: {error}"));
    assert!(matches!(diff, DiffDocument::Text { .. }));
}

#[test]
fn reports_corrupt_head_and_a_removed_untracked_diff_as_errors() {
    let repository = TestRepository::new();
    repository.write("tracked.txt", b"root\n");
    repository.commit_all("root");
    repository.write("temporary.txt", b"temporary\n");
    let service = repository.service();
    let temporary = chronogit::domain::RepoPath::from_bytes(b"temporary.txt".to_vec())
        .unwrap_or_else(|error| panic!("invalid temporary path: {error}"));
    fs::remove_file(repository.path().join("temporary.txt"))
        .unwrap_or_else(|error| panic!("could not remove temporary file: {error}"));
    assert!(
        service
            .diff(&DiffTarget::Worktree {
                path: temporary,
                untracked: true,
            })
            .is_err()
    );

    fs::write(
        repository.path().join(".git/refs/heads/main"),
        format!("{}\n", "f".repeat(40)),
    )
    .unwrap_or_else(|error| panic!("could not corrupt HEAD fixture: {error}"));
    assert!(service.commits(0, 10).is_err());
}

#[cfg(unix)]
#[test]
fn preserves_non_utf8_worktree_paths() {
    let repository = TestRepository::new();
    let name = OsString::from_vec(vec![b'f', b'i', b'l', b'e', b'-', 0xff]);
    let path = repository.path().join(PathBuf::from(name));
    fs::write(&path, b"raw path\n")
        .unwrap_or_else(|error| panic!("could not write non-UTF-8 path: {error}"));
    let service = repository.service();
    let changes = service
        .changes()
        .unwrap_or_else(|error| panic!("could not read non-UTF-8 status: {error}"));
    assert_eq!(changes.len(), 1);
    assert_eq!(
        changes[0].path().as_bytes(),
        &[b'f', b'i', b'l', b'e', b'-', 0xff]
    );
    repository.commit_all("raw tree path");
    let commit = service
        .commits(0, 1)
        .unwrap_or_else(|error| panic!("could not read raw-path commit: {error}"))
        .remove(0);
    let tree = service
        .tree_entries(commit.id())
        .unwrap_or_else(|error| panic!("could not read raw tree path: {error}"));
    assert_eq!(tree[0].name().as_bytes(), changes[0].path().as_bytes());
}
