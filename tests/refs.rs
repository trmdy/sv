mod support;

use git2::BranchType;
use sv::refs;

use support::TestRepo;

#[test]
fn create_branch_from_ref_creates_branch() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.write_file("file.txt", "base")?;
    repo.commit_all("base")?;

    refs::create_branch_from_ref(repo.repo(), "feature", "HEAD", false)?;
    repo.repo().find_branch("feature", BranchType::Local)?;
    Ok(())
}

#[test]
fn delete_branch_removes_branch() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.write_file("file.txt", "base")?;
    repo.commit_all("base")?;

    refs::create_branch_from_ref(repo.repo(), "feature", "HEAD", false)?;
    refs::delete_branch(repo.repo(), "feature")?;
    let deleted = repo.repo().find_branch("feature", BranchType::Local);
    assert!(deleted.is_err());
    Ok(())
}

#[test]
fn resolve_ref_oid_matches_head() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.write_file("file.txt", "base")?;
    let head_oid = repo.commit_all("base")?;

    let resolved = refs::resolve_ref_oid(repo.repo(), "HEAD")?;
    assert_eq!(resolved, head_oid);
    Ok(())
}

#[test]
fn move_branch_ref_updates_target() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.write_file("file.txt", "base")?;
    repo.commit_all("base")?;

    refs::create_branch_from_ref(repo.repo(), "feature", "HEAD", false)?;
    repo.write_file("file.txt", "next")?;
    let new_oid = repo.commit_all("next")?;

    refs::move_branch_ref(repo.repo(), "feature", new_oid)?;
    let branch = repo.repo().find_branch("feature", BranchType::Local)?;
    let target = branch.get().target().expect("branch target");
    assert_eq!(target, new_oid);
    Ok(())
}

#[test]
fn list_branches_filters_by_pattern() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.write_file("file.txt", "base")?;
    repo.commit_all("base")?;

    refs::create_branch_from_ref(repo.repo(), "alpha", "HEAD", false)?;
    refs::create_branch_from_ref(repo.repo(), "beta", "HEAD", false)?;

    let all = refs::list_branches(repo.repo(), None)?;
    assert!(all.contains(&"alpha".to_string()));
    assert!(all.contains(&"beta".to_string()));

    let filtered = refs::list_branches(repo.repo(), Some("a*"))?;
    assert!(filtered.contains(&"alpha".to_string()));
    assert!(!filtered.contains(&"beta".to_string()));
    Ok(())
}
