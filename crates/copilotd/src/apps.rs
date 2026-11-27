use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct App {
    pub id: String,

    pub name: String,
    pub path: PathBuf,
    generic: String,
    keywords: Vec<String>,
    exec: String,
}

fn application_dirs() -> Vec<PathBuf> {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"));
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    let mut dirs = vec![data_home.join("applications")];
    dirs.push(data_home.join("flatpak/exports/share/applications"));
    dirs.extend(data_dirs.split(':').filter(|d| !d.is_empty()).map(|d| Path::new(d).join("applications")));
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
    dirs
}

fn locales() -> Vec<String> {
    let raw = std::env::var("LC_MESSAGES")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default();
    let base = raw.split('.').next().unwrap_or("").to_string();
    let lang = base.split('_').next().unwrap_or("").to_string();
    [base, lang].into_iter().filter(|s| !s.is_empty() && s != "C").collect()
}

fn parse(path: &Path, id: String, locales: &[String]) -> Option<App> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut in_entry = false;
    let mut get = std::collections::HashMap::<String, String>::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            get.entry(k.trim().to_string()).or_insert_with(|| v.trim().to_string());
        }
    }
    if get.get("Type").map(String::as_str) != Some("Application")
        || get.get("NoDisplay").map(String::as_str) == Some("true")
        || get.get("Hidden").map(String::as_str) == Some("true")
    {
        return None;
    }
    let localized = |key: &str| {
        locales
            .iter()
            .find_map(|l| get.get(&format!("{key}[{l}]")))
            .or_else(|| get.get(key))
            .cloned()
            .unwrap_or_default()
    };
    let name = localized("Name");
    if name.is_empty() {
        return None;
    }
    let mut keywords: Vec<String> = localized("Keywords")
        .split(';')
        .chain(get.get("Keywords").map(String::as_str).unwrap_or("").split(';'))
        .filter(|k| !k.is_empty())
        .map(normalize)
        .collect();
    keywords.push(normalize(get.get("Name").map(String::as_str).unwrap_or("")));
    let exec = get
        .get("Exec")
        .and_then(|e| e.split_whitespace().next())
        .and_then(|e| e.rsplit('/').next())
        .map(normalize)
        .unwrap_or_default();
    Some(App {
        id,
        generic: normalize(&localized("GenericName")),
        name,
        path: path.to_path_buf(),
        keywords,
        exec,
    })
}

pub fn all() -> Vec<App> {
    let locales = locales();
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for dir in application_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(file) = path.file_name().and_then(|f| f.to_str()) else { continue };
            let Some(id) = file.strip_suffix(".desktop") else { continue };

            if !seen.insert(id.to_string()) {
                continue;
            }
            if let Some(app) = parse(&path, id.to_string(), &locales) {
                apps.push(app);
            }
        }
    }
    apps
}

pub fn normalize(s: &str) -> String {
    s.trim()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            'ñ' | 'Ñ' => 'n',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

fn score(app: &App, query: &str) -> u32 {
    let name = normalize(&app.name);
    let id_tail = normalize(app.id.rsplit('.').next().unwrap_or(&app.id));
    if name == query || id_tail == query || app.exec == query {
        100
    } else if name.starts_with(query) {
        80
    } else if app.keywords.iter().any(|k| k == query) {
        70
    } else if name.split_whitespace().any(|w| w == query) {
        65
    } else if name.contains(query) || id_tail.contains(query) {
        55
    } else if !app.generic.is_empty() && (app.generic == query || app.generic.contains(query)) {
        45
    } else if app.keywords.iter().any(|k| k.starts_with(query)) {
        35
    } else {
        0
    }
}

pub fn best_match(apps: &[App], query: &str) -> Option<App> {
    let query = normalize(query);
    if query.is_empty() {
        return None;
    }
    apps.iter()
        .map(|a| (score(a, &query), a))
        .filter(|(s, _)| *s > 0)

        .max_by(|(sa, a), (sb, b)| sa.cmp(sb).then(b.name.len().cmp(&a.name.len())))
        .map(|(_, a)| a.clone())
}

pub fn find(query: &str) -> Option<App> {
    best_match(&all(), query)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str, generic: &str, keywords: &[&str]) -> App {
        App {
            id: id.into(),
            name: name.into(),
            path: PathBuf::new(),
            generic: normalize(generic),
            keywords: keywords.iter().map(|k| normalize(k)).collect(),
            exec: String::new(),
        }
    }

    #[test]
    fn matches_by_name_id_and_keyword() {
        let apps = vec![
            app("org.mozilla.firefox", "Firefox", "Web Browser", &["internet", "navegador"]),
            app("org.gnome.Nautilus", "Arquivos", "File Manager", &["pasta", "folder"]),
            app("org.gnome.Ptyxis", "Terminal", "Terminal", &["shell", "console"]),
            app("org.gnome.TextEditor", "Editor de texto", "Text Editor", &[]),
        ];
        assert_eq!(best_match(&apps, "firefox").unwrap().name, "Firefox");
        assert_eq!(best_match(&apps, "Navegador").unwrap().name, "Firefox");
        assert_eq!(best_match(&apps, "nautilus").unwrap().name, "Arquivos");
        assert_eq!(best_match(&apps, "arquivos").unwrap().name, "Arquivos");
        assert_eq!(best_match(&apps, "terminal").unwrap().name, "Terminal");
        assert_eq!(best_match(&apps, "editor").unwrap().name, "Editor de texto");
        assert!(best_match(&apps, "photoshop").is_none());
    }

    #[test]
    fn strips_accents() {
        assert_eq!(normalize("Câmera Música"), "camera musica");
    }
}
