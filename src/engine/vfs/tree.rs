use std::collections::BTreeMap;

pub struct FsDir {
    pub dirs: BTreeMap<String, FsDir>,
    pub files: Vec<String>,
}

impl FsDir {
    pub fn build_from_vfs(vfs: &super::Vfs) -> Self {
        let mut root = FsDir {
            dirs: BTreeMap::new(),
            files: Vec::new(),
        };
        for path in vfs.paths() {
            let parts: Vec<&str> = path
                .trim_start_matches('/')
                .split('/')
                .filter(|s| !s.is_empty())
                .collect();
            root.insert(&parts);
        }
        root
    }

    fn insert(&mut self, parts: &[&str]) {
        match parts {
            [] => {}
            [file] => self.files.push(file.to_string()),
            [dir, rest @ ..] => self
                .dirs
                .entry(dir.to_string())
                .or_insert_with(|| FsDir {
                    dirs: BTreeMap::new(),
                    files: Vec::new(),
                })
                .insert(rest),
        }
    }
}
