fn main() {
    use serde_json::Value;

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

    struct Serializer {
        v: Vec<u8>,
    }

    impl Serializer {
        fn new() -> Self {
            Self { v: Vec::new() }
        }

        #[inline]
        fn serialize_len(&mut self, len: usize) {
            // All of the strings and arrays in the current data are less than 256, so just length prefix with a u8
            let Ok(len) = len.try_into() else {
                panic!("{len} is too long")
            };

            self.v.push(len);
        }

        #[inline]
        fn serialize_str(&mut self, s: &str) {
            self.serialize_len(s.len());
            self.v.extend_from_slice(s.as_bytes());
        }

        fn serialize_api(&mut self, path: &str, mut api: Value) {
            let Some(api) = api.as_object_mut() else {
                panic!("{path} is not an object");
            };

            let Some(p) = api.remove("path") else {
                panic!("{path}.path is missing");
            };
            let Some(u) = api.remove("url") else {
                panic!("{path}.url is missing");
            };

            let Some(p) = p.as_str() else {
                panic!("{path}.path is not a string - {p:?}");
            };
            let Some(u) = u.as_str() else {
                panic!("{path}.url is not a string - {u:?}");
            };

            self.serialize_str(p);
            self.serialize_str(u);
        }

        fn serialize_api_replacements(&mut self, path: &str, apis: &mut Vec<Value>) {
            for (i, api) in apis.iter_mut().enumerate() {
                let Some(api) = api.as_object_mut() else {
                    panic!("{path}[{i}] is not an object");
                };

                let Some(c) = api.remove("crate") else {
                    panic!("{path}[{i}].crate is missing");
                };
                let Some(s) = api.remove("std") else {
                    panic!("{path}[{i}].std is missing");
                };

                self.serialize_api(&format!("{path}[{i}].crate"), c);
                self.serialize_api(&format!("{path}[{i}].std"), s);
            }
        }

        fn serialize_stable(&mut self, name: &str, v: &mut serde_json::Map<String, Value>) {
            for (k, v) in v {
                let Ok(minor) = k.parse::<u8>() else {
                    panic!("failed to parse {name}.stable.{k} minor version");
                };

                self.v.push(minor);

                let mut arr = v.take();
                let Some(arr) = arr.as_array_mut() else {
                    panic!("{name}.stable.{k} is not an array");
                };
                self.serialize_len(arr.len());
                self.serialize_api_replacements(&format!("{name}.stable.{k}"), arr);
            }
        }

        fn serialize(&mut self, map: serde_json::Map<String, Value>) {
            self.v = Vec::with_capacity(32 * 1024);
            // version + magic number
            self.v.extend_from_slice(&[0xcd, 0xcd, 0xcd, 1]);

            let count = u32::try_from(map.len())
                .expect("too many replacements")
                .to_le_bytes();
            self.v.extend_from_slice(&count);
            self.v.resize(8 + map.len() * 4, 0);

            let mut offset = 8;

            for (k, mut v) in map {
                let h = u32::try_from(self.v.len())
                    .expect("offset too large")
                    .to_le_bytes();
                self.v[offset..offset + 4].copy_from_slice(&h);
                offset += 4;

                self.serialize_str(&k);
                let entry = v.as_object_mut().unwrap();

                let Some(mut stable) = entry.remove("stable") else {
                    panic!("no stable entries for '{k}'");
                };

                // Serialize the stable entries, we must have at least 1
                let Some(v) = stable.as_object_mut() else {
                    panic!("{k}.stable was not a map - {v:?}")
                };

                self.serialize_len(v.len());

                if let Some(unstable) = entry.get("unstable") {
                    let Some(apis) = unstable.as_array() else {
                        panic!("{k}.unstable is not an array - {unstable:?}")
                    };
                    self.serialize_len(apis.len());
                } else {
                    self.v.push(0);
                };

                if let Some(unavailable) = entry.get("unavailable") {
                    let Some(apis) = unavailable.as_array() else {
                        panic!("{k}.unavailable is not an array - {unavailable:?}");
                    };

                    self.serialize_len(apis.len());
                } else {
                    self.v.push(0);
                }

                self.serialize_stable(&k, v);

                if let Some(mut unstable) = entry.remove("unstable") {
                    let Some(apis) = unstable.as_array_mut() else {
                        panic!("{k}.unstable is not an array - {unstable:?}")
                    };
                    self.serialize_api_replacements(&format!("{k}.unstable"), apis);
                }

                if let Some(mut unavailable) = entry.remove("unavailable") {
                    let Some(apis) = unavailable.as_array_mut() else {
                        panic!("{k}.unavailable is not an array - {unavailable:?}");
                    };

                    for (i, api) in std::mem::take(apis).into_iter().enumerate() {
                        self.serialize_api(&format!("{k}.unavailable[{i}]"), api);
                    }
                }
            }
        }
    }

    let mut s = Serializer::new();
    s.serialize(root);

    if !std::env::args().any(|a| a == "--commit") {
        println!("validated - {} bytes when serialized", s.v.len());
        std::fs::write("all.bin", &s.v).expect("failed to create output file");
        return;
    }

    let mut cmd = std::process::Command::new("git");
    cmd.args(["checkout", "collated"]);
    cmd.status().expect("failed to checkout 'collated'");

    std::fs::write("all.bin", &s.v).expect("failed to create output file");

    let mut cmd = std::process::Command::new("git");
    cmd.args(["add", "all.bin"]);
    cmd.status().expect("failed to add 'all.bin'");

    let mut cmd = std::process::Command::new("git");
    cmd.args(["commit", "--message", "update all.bin"]);
    cmd.status().expect("failed to commit 'all.bin'");

    let mut cmd = std::process::Command::new("git");
    cmd.args(["push", "-f"]);
    cmd.status().expect("failed to push 'all.bin'");

    let mut cmd = std::process::Command::new("git");
    cmd.args(["checkout", "-"]);
    cmd.status().expect("failed to return to previous branch");
}
