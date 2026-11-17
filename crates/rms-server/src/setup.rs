//! First-run setup: the vault key and the admin password live in `.env`. If either is
//! missing it is generated and written there, and the password is shown once.

use std::io::Write;

fn random_secret() -> String {
    // Two random UUIDs: 244 bits from the operating system's random source.
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

fn read_env_file() -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    if let Ok(text) = std::fs::read_to_string(".env") {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                out.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    out
}

pub struct Secrets {
    pub vault_key: String,
    pub admin_password: String,
    /// Set only on the run that created the password, so it can be shown once.
    pub new_password: Option<String>,
}

/// Loads `.env` into the process environment (real variables win), then makes sure the two
/// secrets exist. Returns them.
pub fn ensure() -> Secrets {
    let file = read_env_file();
    for (k, v) in &file {
        if std::env::var_os(k).is_none() {
            std::env::set_var(k, v);
        }
    }
    let have = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    let mut add = String::new();
    let vault_key = have("RMS_VAULT_KEY").unwrap_or_else(|| {
        let k = random_secret();
        add.push_str(&format!("RMS_VAULT_KEY={k}\n"));
        k
    });
    let mut new_password = None;
    let admin_password = have("RMS_ADMIN_PASSWORD").unwrap_or_else(|| {
        let p = random_secret()[..20].to_string();
        add.push_str(&format!("RMS_ADMIN_PASSWORD={p}\n"));
        new_password = Some(p.clone());
        p
    });
    if !add.is_empty() {
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(".env").expect("cannot write .env");
        if std::fs::metadata(".env").map(|m| m.len() > 0).unwrap_or(false) && !read_env_text_ends_with_newline() {
            let _ = f.write_all(b"\n");
        }
        f.write_all(add.as_bytes()).expect("cannot write .env");
    }
    Secrets { vault_key, admin_password, new_password }
}

fn read_env_text_ends_with_newline() -> bool {
    std::fs::read_to_string(".env").map(|t| t.is_empty() || t.ends_with('\n')).unwrap_or(true)
}
