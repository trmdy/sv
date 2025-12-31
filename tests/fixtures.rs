mod support;

use support::TestRepo;

#[test]
fn creates_repo_and_commit() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;
    repo.write_file("README.md", "# sv test\n")?;
    repo.commit_all("initial commit")?;
    Ok(())
}

#[test]
fn creates_branch_and_worktree() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;
    repo.commit_file("README.md", "base\n", "initial commit")?;
    repo.create_branch("feature")?;
    let worktree_path = repo.create_worktree("wt-feature", Some("feature"))?;
    assert!(worktree_path.exists());
    Ok(())
}
