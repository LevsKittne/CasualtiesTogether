use ct_net::Addr;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ban {
    pub key: String,
    pub name: String,
    pub reason: String,
}

pub struct BanList {
    path: Option<PathBuf>,
    pub entries: Vec<Ban>,
}

pub fn key_of(addr: &Addr) -> String {
    match addr {
        Addr::Steam(id) => format!("steam:{id}"),
        Addr::Udp(a) => format!("ip:{}", a.ip()),
    }
}

impl BanList {
    pub fn load(path: Option<&Path>) -> Self {
        let mut entries = Vec::new();
        if let Some(p) = path {
            for line in std::fs::read_to_string(p).unwrap_or_default().lines() {
                let mut it = line.splitn(3, '\t');
                if let (Some(key), Some(name)) = (it.next(), it.next()) {
                    if !key.trim().is_empty() {
                        entries.push(Ban { key: key.trim().to_owned(), name: name.to_owned(), reason: it.next().unwrap_or("").to_owned() });
                    }
                }
            }
        }
        Self { path: path.map(Path::to_path_buf), entries }
    }

    fn save(&self) {
        if let Some(p) = &self.path {
            let s: String = self.entries.iter().map(|b| format!("{}\t{}\t{}\r\n", b.key, b.name, b.reason)).collect();
            let _ = std::fs::write(p, s);
        }
    }

    pub fn find(&self, addr: &Addr) -> Option<&Ban> {
        let key = key_of(addr);
        self.entries.iter().find(|b| b.key == key)
    }

    pub fn add(&mut self, addr: &Addr, name: &str, reason: &str) {
        let key = key_of(addr);
        self.entries.retain(|b| b.key != key);
        self.entries.push(Ban { key, name: name.to_owned(), reason: reason.to_owned() });
        self.save();
    }

    pub fn remove(&mut self, query: &str) -> Vec<Ban> {
        let q = query.trim();
        let (gone, keep): (Vec<Ban>, Vec<Ban>) = std::mem::take(&mut self.entries)
            .into_iter()
            .partition(|b| b.key.eq_ignore_ascii_case(q) || b.name.eq_ignore_ascii_case(q) || b.key.ends_with(&format!(":{q}")));
        self.entries = keep;
        if !gone.is_empty() {
            self.save();
        }
        gone
    }
}
