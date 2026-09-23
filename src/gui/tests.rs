use super::title_candidates;

#[test]
fn title_candidates_no_workspace() {
    assert_eq!(title_candidates(None), vec!["GG - Gui for JJ"]);
}

#[test]
#[cfg(unix)]
fn title_candidates_elide_leading_directories() {
    assert_eq!(
        title_candidates(Some("/opt/src/foo/bar")),
        vec![
            "GG - /opt/src/foo/bar",
            "GG - …/src/foo/bar",
            "GG - …/foo/bar",
            "GG - bar",
        ]
    );
}

#[test]
#[cfg(unix)]
fn title_candidates_abbreviate_home() {
    let home = etcetera::home_dir().unwrap();
    let path = home.join("Documents/Agile/framework");
    let path = path.to_str().unwrap();

    assert_eq!(
        title_candidates(Some(path)),
        vec![
            format!("GG - {path}"),
            String::from("GG - ~/Documents/Agile/framework"),
            String::from("GG - …/Agile/framework"),
            String::from("GG - framework"),
        ]
    );
}

#[test]
#[cfg(unix)]
fn title_candidates_root() {
    assert_eq!(title_candidates(Some("/")), vec!["GG - /"]);
}
