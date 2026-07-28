use std::fs;

use encoding_rs::UTF_8;
use slix::output::{render_csv, render_json, render_raw};
use slix::parser::parse_message;
use slix::spec::Spec;

#[test]
fn parses_slicer_grammar_and_rejects_invalid_specs() {
    let spec = Spec::parse(
        "# owner: ops\nservice_code 3 text # dispatch\n@repeat lines 2\nitem 2\n@end\n",
        "sample.spec",
    )
    .unwrap();
    assert_eq!(spec.metadata["owner"], "ops");
    assert_eq!(spec.byte_len(), 7);

    assert!(Spec::parse("a 1\na 2\n", "duplicate.spec").is_err());
    assert!(Spec::parse("@repeat x 2\n@end\n", "empty.spec").is_err());
    assert!(Spec::parse("", "empty.spec").is_err());
}

#[test]
fn dispatches_and_preserves_offsets_and_repeat_structure() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("body")).unwrap();
    fs::write(root.path().join("header.spec"), "service_code 4\nflag 1\n").unwrap();
    fs::write(
        root.path().join("body").join("#ABC.spec"),
        "@repeat rows 2\nvalue 2\n@end\n",
    )
    .unwrap();

    let result = parse_message(b"ABC X0102", root.path(), UTF_8).unwrap();
    assert_eq!(result.schema_version, 1);
    assert_eq!(result.service.code.as_deref(), Some("ABC "));
    assert_eq!(result.service.dispatch_code.as_deref(), Some("ABC"));
    assert_eq!(result.header.fields[0].value, "ABC ");
    assert_eq!((result.fields[0].start, result.fields[0].end), (0, 3));
    assert_eq!(result.body.as_ref().unwrap().groups[0].occurrences.len(), 2);
    assert_eq!(result.fields[3].path, "body.rows[02].value");
    assert!(result.errors.is_empty());
}

#[test]
fn returns_partial_results_and_renders_formats() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("body")).unwrap();
    fs::write(root.path().join("header.spec"), "service_code 2\nmore 3\n").unwrap();

    let short = parse_message(b"A ", root.path(), UTF_8).unwrap();
    assert_eq!(short.errors[0].code, "truncated_field");
    assert_eq!(short.warnings[0].code, "missing_body_spec");

    fs::write(root.path().join("body").join("A.spec"), "body 1\n").unwrap();
    let long = parse_message(b"A 123ZY", root.path(), UTF_8).unwrap();
    assert_eq!(long.unparsed_tail.as_deref(), Some("Y"));
    assert_eq!(long.warnings[0].code, "overflow");
    assert!(
        render_json(&long)
            .unwrap()
            .contains("\"schema_version\": 1")
    );
    assert!(render_csv(&long).starts_with("path,value,start,end"));
    assert!(render_raw(&long).contains("header.service_code=A "));
}
