#![expect(clippy::unwrap_used, reason = "Explicit fixture assertions")]
use rustris_core::{
    bytes::OsBytes,
    domain::{EnvDelta, ProfileInput, Runner},
    paths::Paths,
};
use std::{fs, os::unix::fs::DirBuilderExt, path::PathBuf};
use uuid::Uuid;

pub struct Fixture {
    pub root: PathBuf,
    pub paths: Paths,
}
impl Fixture {
    pub fn new() -> Self {
        let root = std::env::temp_dir().join(format!("rustris-core-{}", Uuid::now_v7()));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        Self {
            paths: Paths::in_root(&root.join("owned")),
            root,
        }
    }
    pub fn input(&self) -> ProfileInput {
        let executable = self.root.join("foreign-game");
        fs::write(&executable, b"fixture installation; never execute").unwrap();
        use std::os::unix::ffi::OsStrExt;
        ProfileInput {
            name: "Café 🦀".into(),
            executable: OsBytes::new(executable.as_os_str().as_bytes()).unwrap(),
            cwd: OsBytes::new(self.root.as_os_str().as_bytes()).unwrap(),
            arguments: vec![
                OsBytes::new(Vec::new()).unwrap(),
                OsBytes::new(b"x\xff ;$(touch never)".to_vec()).unwrap(),
            ],
            environment: vec![
                EnvDelta {
                    key: OsBytes::new(b"KEY_\xff".to_vec()).unwrap(),
                    value: Some(OsBytes::new(b"value_\xfe".to_vec()).unwrap()),
                },
                EnvDelta {
                    key: OsBytes::new(b"EMPTY".to_vec()).unwrap(),
                    value: Some(OsBytes::new(Vec::new()).unwrap()),
                },
                EnvDelta {
                    key: OsBytes::new(b"UNSET".to_vec()).unwrap(),
                    value: None,
                },
            ],
            runner: Runner::Native,
            prefix: None,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
