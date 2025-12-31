use std::fs;
use std::path::{Path, PathBuf};

use git2::{IndexAddOption, Oid, Repository, Signature, WorktreeAddOptions};
use sv::lease::Lease;
use tempfile::TempDir;

pub struct TestRepo {
    dir: TempDir,
    repo: Repository,
}

impl TestRepo {
    pub fn init() -> Result<Self, git2::Error> {
        let dir = tempfile::tempdir().expect("failed to create tempdir");
        let repo = Repository::init(dir.path())?;
        set_identity(&repo)?;
        Ok(Self { dir, repo })
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn write_file(&self, rel_path: &str, contents: &str) -> std::io::Result<PathBuf> {
        let path = self.dir.path().join(rel_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, contents)?;
        Ok(path)
    }

    pub fn init_sv_dirs(&self) -> std::io::Result<()> {
        fs::create_dir_all(self.dir.path().join(".sv"))?;
        fs::create_dir_all(self.dir.path().join(".git").join("sv"))?;
        Ok(())
    }

    pub fn write_sv_config(&self, contents: &str) -> std::io::Result<PathBuf> {
        self.write_file(".sv.toml", contents)
    }

    pub fn sv_dir(&self) -> PathBuf {
        self.dir.path().join(".sv")
    }

    pub fn git_sv_dir(&self) -> PathBuf {
        self.dir.path().join(".git").join("sv")
    }

    pub fn read_leases(&self) -> Result<Vec<Lease>, Box<dyn std::error::Error>> {
        let path = self.git_sv_dir().join("leases.jsonl");
        if !path.exists() {
            return Ok(Vec::new());
        }

        let contents = fs::read_to_string(&path)?;
        let mut leases = Vec::new();
        for line in contents.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let lease: Lease = serde_json::from_str(trimmed)?;
            leases.push(lease);
        }
        Ok(leases)
    }

    pub fn commit_all(&self, message: &str) -> Result<Oid, git2::Error> {
        let mut index = self.repo.index()?;
        index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
        index.write()?;

        let tree_id = index.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;
        let sig = Signature::now("sv-test", "sv-test@example.com")?;

        let parent = self
            .repo
            .head()
            .ok()
            .and_then(|head| head.target())
            .and_then(|oid| self.repo.find_commit(oid).ok());

        let oid = match parent {
            Some(parent) => self
                .repo
                .commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])?,
            None => self
                .repo
                .commit(Some("HEAD"), &sig, &sig, message, &tree, &[])?,
        };

        Ok(oid)
    }

    pub fn stage_path(&self, rel_path: &str) -> Result<(), git2::Error> {
        let mut index = self.repo.index()?;
        index.add_path(Path::new(rel_path))?;
        index.write()?;
        Ok(())
    }

    pub fn commit_file(
        &self,
        rel_path: &str,
        contents: &str,
        message: &str,
    ) -> Result<Oid, Box<dyn std::error::Error>> {
        self.write_file(rel_path, contents)?;
        Ok(self.commit_all(message)?)
    }

    pub fn create_branch(&self, name: &str) -> Result<(), git2::Error> {
        let commit = self.repo.head()?.peel_to_commit()?;
        self.repo.branch(name, &commit, false)?;
        Ok(())
    }

    pub fn checkout_branch(&self, name: &str) -> Result<(), git2::Error> {
        let refname = format!("refs/heads/{name}");
        self.repo.set_head(&refname)?;
        self.repo.checkout_head(None)?;
        Ok(())
    }

    pub fn create_worktree(
        &self,
        name: &str,
        branch: Option<&str>,
    ) -> Result<PathBuf, git2::Error> {
        let path = self.dir.path().join(name);
        let mut opts = WorktreeAddOptions::new();
        let worktree = if let Some(branch) = branch {
            let reference = self.repo.find_reference(&format!("refs/heads/{branch}"))?;
            opts.reference(Some(&reference));
            self.repo.worktree(name, &path, Some(&opts))?
        } else {
            self.repo.worktree(name, &path, Some(&opts))?
        };
        Ok(worktree.path().to_path_buf())
    }

    pub fn repo(&self) -> &Repository {
        &self.repo
    }
}

fn set_identity(repo: &Repository) -> Result<(), git2::Error> {
    let mut cfg = repo.config()?;
    cfg.set_str("user.name", "sv-test")?;
    cfg.set_str("user.email", "sv-test@example.com")?;
    Ok(())
}
