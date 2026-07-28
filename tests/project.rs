use std::fs;

use slix::config::{GlobalConfig, load_global_config_from, save_global_config_to};
use slix::project::{
    import_project_from, list_projects_from, resolve_active_project_from, set_active_project_from,
    validate_project_name,
};

#[test]
fn global_config_defaults_and_round_trips() {
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(
        load_global_config_from(temp.path()).unwrap(),
        GlobalConfig::default()
    );

    let config = GlobalConfig {
        active_project: Some("alpha".into()),
        default_output: Some("json".into()),
        editor: Some("nvim".into()),
    };
    save_global_config_to(temp.path(), &config).unwrap();
    assert_eq!(load_global_config_from(temp.path()).unwrap(), config);
}

#[test]
fn imports_lists_and_selects_projects() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir_all(source.join("body/nested")).unwrap();
    fs::create_dir_all(source.join("reff")).unwrap();
    fs::write(source.join("header.spec"), "header").unwrap();
    fs::write(source.join("body/nested/item.spec"), "body").unwrap();
    fs::write(source.join("reff/ref.spec"), "ref").unwrap();
    fs::write(source.join("ignored.txt"), "ignored").unwrap();

    let beta = import_project_from(temp.path(), "beta", &source).unwrap();
    let alpha = import_project_from(temp.path(), "alpha", &source).unwrap();
    assert_eq!(beta.config.encoding, "utf-8");
    assert_eq!(beta.config.specs, "specs");
    assert_eq!(
        fs::read_to_string(alpha.specs_dir().join("body/nested/item.spec")).unwrap(),
        "body"
    );
    assert!(!alpha.specs_dir().join("ignored.txt").exists());

    let names: Vec<_> = list_projects_from(temp.path())
        .unwrap()
        .into_iter()
        .map(|project| project.config.name)
        .collect();
    assert_eq!(names, ["alpha", "beta"]);

    set_active_project_from(temp.path(), "beta").unwrap();
    assert_eq!(
        resolve_active_project_from(temp.path(), None)
            .unwrap()
            .config
            .name,
        "beta"
    );
    assert_eq!(
        resolve_active_project_from(temp.path(), Some("alpha"))
            .unwrap()
            .config
            .name,
        "alpha"
    );
}

#[test]
fn rejects_invalid_names_and_existing_projects() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();

    for invalid in ["", "two words", "../escape", "caf\u{e9}"] {
        assert!(validate_project_name(invalid).is_err());
    }
    import_project_from(temp.path(), "valid-_1", &source).unwrap();
    assert!(import_project_from(temp.path(), "valid-_1", &source).is_err());
}

#[cfg(unix)]
#[test]
fn import_refuses_symlinks() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir_all(source.join("body")).unwrap();
    fs::write(source.join("outside.spec"), "outside").unwrap();
    symlink(source.join("outside.spec"), source.join("body/link.spec")).unwrap();

    assert!(import_project_from(temp.path(), "linked", &source).is_err());
    assert!(!temp.path().join("projects/linked").exists());
}
