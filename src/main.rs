fn main() {
    let mut root = serde_json::Map::new();

    for entry in std::fs::read_dir("data").expect("failed to read data dir") {
        let entry = entry.expect("failed to read directory entry");
        let path = entry.path();

        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let contents = std::fs::read_to_string(&path).expect("failed to read file");
        let v: serde_json::Value =
            serde_json::from_str(&contents).expect("failed to deserialize json");

        root.insert(
            path.file_stem()
                .and_then(|fs| fs.to_str())
                .expect("expected utf-8 file stem")
                .to_owned(),
            v,
        );
    }

    let mut cmd = std::process::Command::new("git");
    cmd.args(["checkout", "collated"]);
    cmd.status().expect("failed to checkout 'collated'");

    {
        let mut out = std::fs::File::create("all.json").expect("failed to create output file");
        serde_json::to_writer_pretty(&mut out, &root).expect("failed to serialize output");
    }

    let mut cmd = std::process::Command::new("git");
    cmd.args(["add", "all.json"]);
    cmd.status().expect("failed to add 'all.json'");

    let mut cmd = std::process::Command::new("git");
    cmd.args(["commit", "--message", "update all.json"]);
    cmd.status().expect("failed to commit 'all.json'");

    let mut cmd = std::process::Command::new("git");
    cmd.args(["push", "-f"]);
    cmd.status().expect("failed to push 'all.json'");

    let mut cmd = std::process::Command::new("git");
    cmd.args(["checkout", "-"]);
    cmd.status().expect("failed to return to previous branch");
}
