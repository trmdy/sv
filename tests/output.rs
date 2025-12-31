use sv::output::{format_human, HumanOutput};

#[test]
fn format_human_includes_sections() {
    let mut human = HumanOutput::new("sv init: initialized");
    human.push_summary("repo", "/tmp/repo");
    human.push_detail("created .sv.toml");
    human.push_warning("staged files match protected patterns");
    human.push_next_step("sv status");

    let rendered = format_human(&human);
    assert!(rendered.contains("sv init: initialized"));
    assert!(rendered.contains("Summary:"));
    assert!(rendered.contains("- repo: /tmp/repo"));
    assert!(rendered.contains("Details:"));
    assert!(rendered.contains("- created .sv.toml"));
    assert!(rendered.contains("Warnings:"));
    assert!(rendered.contains("- staged files match protected patterns"));
    assert!(rendered.contains("Next steps:"));
    assert!(rendered.contains("- sv status"));
}

#[test]
fn format_human_omits_empty_sections() {
    let human = HumanOutput::new("sv init: already initialized");
    let rendered = format_human(&human);
    assert_eq!(rendered, "sv init: already initialized");
}
