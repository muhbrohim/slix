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
    assert_eq!(spec.char_len(), 7);

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
    assert_eq!(result.schema_version, 2);
    assert_eq!(result.service.code.as_deref(), Some("ABC "));
    assert_eq!(result.service.dispatch_code.as_deref(), Some("ABC"));
    assert_eq!(result.header.fields[0].value, "ABC ");
    assert_eq!((result.fields[0].start, result.fields[0].end), (0, 3));
    assert_eq!(result.body.as_ref().unwrap().groups[0].occurrences.len(), 2);
    assert_eq!(result.fields[3].path, "body.rows[02].value");
    assert!(result.errors.is_empty());
}

#[test]
fn parses_fields_and_offsets_by_rust_char() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("body")).unwrap();
    fs::write(root.path().join("header.spec"), "service_code 2\nflag 1\n").unwrap();
    fs::write(root.path().join("body").join("éA.spec"), "payload 1\n").unwrap();

    let result = parse_message("éAZ猫!".as_bytes(), root.path(), UTF_8).unwrap();

    assert_eq!(result.schema_version, 2);
    assert_eq!(result.header.fields[0].value, "éA");
    assert_eq!(
        (result.header.fields[0].start, result.header.fields[0].end),
        (0, 1)
    );
    assert_eq!(result.header.fields[1].value, "Z");
    assert_eq!(
        (result.header.fields[1].start, result.header.fields[1].end),
        (2, 2)
    );
    assert_eq!(result.body.as_ref().unwrap().fields[0].value, "猫");
    assert_eq!(
        (
            result.body.as_ref().unwrap().fields[0].start,
            result.body.as_ref().unwrap().fields[0].end
        ),
        (3, 3)
    );
    assert_eq!(result.unparsed_tail.as_deref(), Some("!"));
    assert_eq!(
        result.warnings[0].message,
        "1 character(s) remain after the selected specifications"
    );
}

#[test]
fn detects_truncation_by_rust_char() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("header.spec"),
        "service_code 1\npayload 2\n",
    )
    .unwrap();

    let result = parse_message("é".as_bytes(), root.path(), UTF_8).unwrap();

    assert_eq!(result.header.fields[0].value, "é");
    assert_eq!(result.errors[0].code, "truncated_field");
    assert_eq!(
        result.errors[0].message,
        "field `header.payload` requires 2 character(s), but only 0 remain"
    );
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
    let json: serde_json::Value = serde_json::from_str(&render_json(&long).unwrap()).unwrap();
    assert_eq!(json["service_code"], "A");
    assert_eq!(json["header"]["more"], "123");
    assert_eq!(json["body"]["body"], "Z");
    assert!(json.get("schema_version").is_none());
    assert!(render_csv(&long).starts_with("path,value,start,end"));
    assert!(render_raw(&long).contains("header.service_code=A "));
}

#[test]
fn renders_repeat_groups_as_clean_json_arrays() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("body")).unwrap();
    fs::write(root.path().join("header.spec"), "service_code 3\n").unwrap();
    fs::write(
        root.path().join("body").join("ABC.spec"),
        "# endpoint: /customer\n@repeat rows 2\nvalue 2\n@end\n",
    )
    .unwrap();

    let result = parse_message(b"ABC0102", root.path(), UTF_8).unwrap();
    let json: serde_json::Value = serde_json::from_str(&render_json(&result).unwrap()).unwrap();
    assert_eq!(json["endpoint"], "/customer");
    assert_eq!(json["body"]["rows"][0]["value"], "01");
    assert_eq!(json["body"]["rows"][1]["value"], "02");

    let rendered = render_json(&result).unwrap();
    let service = rendered.find("\"service_code\"").unwrap();
    let endpoint = rendered.find("\"endpoint\"").unwrap();
    let header = rendered.find("\"header\"").unwrap();
    let body = rendered.find("\"body\"").unwrap();
    let tail = rendered.find("\"unparsed_tail\"").unwrap();
    assert!(service < endpoint && endpoint < header && header < body && body < tail);
    assert!(rendered.find("\"rows\"").unwrap() > rendered.find("\"service_code\"").unwrap());
}
