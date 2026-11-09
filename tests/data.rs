//! The data check on the examples, as a user runs it.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_quranic-data");

fn run(args: &[&str]) -> (i32, String) {
    let o = Command::new(BIN).args(args).current_dir(env!("CARGO_MANIFEST_DIR")).output().unwrap();
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into())
}

#[test]
fn the_examples_pass_and_fail_as_described() {
    let (code, out) = run(&["examples/data/passing"]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["examples/data/failing"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("`hafsh` is not a riwayah") && out.contains("ayah numbers that are not Kufi"), "{out}");
}

#[test]
fn json_names_every_body_of_quran_data() {
    let (_, out) = run(&["--format", "json", "examples/data/failing"]);
    let result: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(result["quran_data"][0]["ayahs"], 285);
    assert_eq!(result["summary"]["errors"], 2);
}

#[test]
fn the_registries_check_themselves() {
    let (code, out) = run(&["registry"]);
    assert_eq!(code, 0);
    assert!(out.contains("sajdah places"));
}
