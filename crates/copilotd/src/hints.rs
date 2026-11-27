use std::sync::LazyLock;

use cactus::FunctionCall;
use regex::{Regex, RegexBuilder};
use serde_json::Value;

use crate::actions::{ACTIONS, Action};
use crate::apps::normalize;

fn re(pattern: &str) -> Regex {
    RegexBuilder::new(pattern).case_insensitive(true).build().expect("valid pattern")
}

struct Compiled {
    name: &'static str,
    triggers: Vec<Regex>,
}

static TRIGGERS: LazyLock<Vec<Compiled>> = LazyLock::new(|| {
    ACTIONS
        .iter()
        .map(|a: &Action| Compiled { name: a.name, triggers: a.triggers.iter().map(|t| re(t)).collect() })
        .collect()
});

pub fn triggered(query: &str, action: &str) -> bool {
    TRIGGERS
        .iter()
        .find(|c| c.name == action)
        .is_some_and(|c| c.triggers.iter().any(|r| r.is_match(query)))
}

fn has(query: &str, pattern: &str) -> bool {
    re(pattern).is_match(query)
}

pub fn refine(query: &str, call: &mut FunctionCall) -> bool {
    let q = normalize(query);
    let before = call.arguments.clone();
    let set = |call: &mut FunctionCall, key: &str, value: Value| {
        call.arguments.insert(key.into(), value);
    };
    let off = has(&q, r"\b(desliga\w*|desativa\w*|tira\w*|remove\w*|off|disable|turn off|sem)\b");
    let on = has(&q, r"\b(liga|ligar|ativa|ativar|coloca|poe|bota|on|enable|turn on)\b");

    match call.name.as_str() {
        "set_dark_mode" => {
            if has(&q, r"\b(claro|clara|light)\b") {
                set(call, "enabled", Value::Bool(false));
            } else if has(&q, r"\b(escuro|escura|dark)\b") {
                set(call, "enabled", Value::Bool(!off));
            }
        }
        "set_night_light" | "set_do_not_disturb" => {
            if off {
                set(call, "enabled", Value::Bool(false));
            } else if on {
                set(call, "enabled", Value::Bool(true));
            }
        }
        "set_mute" => {
            if has(&q, r"\b(tira\w* (o )?mudo|desmut\w*|unmute|ativa\w* (o )?som|liga\w* (o )?som|volta\w* (o )?som)") {
                set(call, "muted", Value::Bool(false));
            } else if has(&q, r"\b(silenci\w*|mute|mudo)\b") {
                set(call, "muted", Value::Bool(true));
            }
        }
        "set_volume" => {
            if has(&q, r"\b(maximo|max|todo|100)\b") && !has(&q, r"\d{1,2}\b") {
                set(call, "percent", Value::from(100));
            } else if has(&q, r"\b(minimo|zero)\b") {
                set(call, "percent", Value::from(0));
            } else if let Some(n) = re(r"\b(\d{1,3})\s*(%|por ?cento|percent)?").captures(&q)
                .and_then(|c| c[1].parse::<u64>().ok())
            {
                set(call, "percent", Value::from(n.min(100)));
            }
        }
        "power" => {
            let op = if has(&q, r"\b(sair|sessao|log ?out|logout|sign out|encerrar a sessao)\b") {
                Some("log_out")
            } else if has(&q, r"\b(reinici\w*|restart|reboot)\b") {
                Some("restart")
            } else if has(&q, r"\b(deslig\w*|shut ?down|power ?off)\b") {
                Some("power_off")
            } else {
                None
            };
            if let Some(op) = op {
                set(call, "operation", Value::from(op));
            }
        }
        "open_settings" => {
            if let Some(page) = settings_page(&q) {
                set(call, "page", Value::from(page));
            }
        }
        "open_folder" => {
            if let Some(folder) = folder(&q) {
                set(call, "folder", Value::from(folder));
            }
        }
        "open_url" => {
            if let Some(url) = url_in(query) {
                set(call, "url", Value::from(url));
            } else if let Some(Value::String(url)) = call.arguments.get("url") {
                let known = known_site(&normalize(url)).or_else(|| known_site(&q));
                if let Some(site) = known {
                    set(call, "url", Value::from(site));
                } else if !url.contains('.') && !url.contains(' ') {
                    let fixed = format!("{url}.com");
                    set(call, "url", Value::from(fixed));
                }
            }
        }
        _ => {}
    }
    call.arguments != before
}

fn settings_page(q: &str) -> Option<&'static str> {
    const MAP: &[(&str, &str)] = &[
        (r"papel de parede|plano de fundo|wallpaper|background|fundo", "background"),
        (r"wi-?fi|wireless|sem fio", "wifi"),
        (r"bluetooth", "bluetooth"),
        (r"\b(som|audio|sound|alto-?falante|microfone)\b", "sound"),
        (r"\b(tela|monitor|display|resolucao|telas)\b", "display"),
        (r"\b(energia|bateria|battery|power)\b", "power"),
        (r"notifica", "notifications"),
        (r"teclado|keyboard|atalho", "keyboard"),
        (r"mouse|touchpad", "mouse"),
        (r"impressora|printer", "printers"),
        (r"privacidade|privacy", "privacy"),
        (r"\b(rede|network|vpn|proxy|ethernet)\b", "network"),
        (r"usuario|users|conta de usuario", "users"),
        (r"acessibilidade|accessibility", "accessibility"),
        (r"multitarefa|multitasking", "multitasking"),
        (r"contas online|online accounts", "online-accounts"),
        (r"\b(aplicativos|apps)\b", "apps"),
        (r"\b(sistema|system|sobre|about)\b", "system"),
    ];
    MAP.iter().find(|(p, _)| has(q, p)).map(|(_, page)| *page)
}

fn folder(q: &str) -> Option<&'static str> {
    const MAP: &[(&str, &str)] = &[
        (r"download", "downloads"),
        (r"documento|document", "documents"),
        (r"\b(imagens|fotos|pictures|photos)\b", "pictures"),
        (r"musica|music", "music"),
        (r"video", "videos"),
        (r"area de trabalho|desktop", "desktop"),
        (r"\b(pessoal|home|inicio)\b", "home"),
    ];
    MAP.iter().find(|(p, _)| has(q, p)).map(|(_, f)| *f)
}

fn known_site(q: &str) -> Option<&'static str> {
    const MAP: &[(&str, &str)] = &[
        ("youtube", "https://www.youtube.com"),
        ("gmail", "https://mail.google.com"),
        ("google", "https://www.google.com"),
        ("github", "https://github.com"),
        ("wikipedia", "https://www.wikipedia.org"),
        ("whatsapp", "https://web.whatsapp.com"),
    ];
    MAP.iter().find(|(k, _)| q.contains(k)).map(|(_, u)| *u)
}

fn url_in(query: &str) -> Option<String> {
    static URL: LazyLock<Regex> = LazyLock::new(|| {
        re(r"(https?://)?([a-z0-9-]+\.)+(com|org|net|br|io|dev|app|gov|edu|me|tv)(\.[a-z]{2})?(/\S*)?")
    });
    let m = URL.find(query)?;
    let url = m.as_str().trim_end_matches(['.', ',', '!', '?']);
    Some(if url.contains("://") { url.into() } else { format!("https://{url}") })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(name: &str, args: Value) -> FunctionCall {
        FunctionCall { name: name.into(), arguments: args.as_object().unwrap().clone() }
    }

    fn refined(query: &str, name: &str, args: Value) -> Value {
        let mut c = call(name, args);
        refine(query, &mut c);
        Value::Object(c.arguments)
    }

    #[test]
    fn triggers_route() {
        assert!(triggered("bloqueia a tela", "lock_screen"));
        assert!(triggered("abre as configurações de wi-fi", "open_settings"));
        assert!(triggered("desliga o computador", "power"));
        assert!(!triggered("desliga a luz noturna", "power"));
        assert!(!triggered("qual a capital da frança?", "power"));
    }

    #[test]
    fn explicit_words_win() {
        assert_eq!(refined("muda para o tema claro", "set_dark_mode", json!({"enabled": true}))["enabled"], false);
        assert_eq!(refined("tira o mudo", "set_mute", json!({"muted": true}))["muted"], false);
        assert_eq!(refined("silencia o som", "set_mute", json!({"muted": false}))["muted"], true);
        assert_eq!(refined("desliga a luz noturna", "set_night_light", json!({"enabled": true}))["enabled"], false);
        assert_eq!(refined("sair da sessão", "power", json!({"operation": "power_off"}))["operation"], "log_out");
        assert_eq!(refined("quero mudar o papel de parede", "open_settings", json!({"page": "wifi"}))["page"], "background");
        assert_eq!(refined("vai para github.com", "open_url", json!({"url": "https://github.com/videos/"}))["url"], "https://github.com");
        assert_eq!(refined("abre o youtube", "open_url", json!({"url": "youtube"}))["url"], "https://www.youtube.com");
        assert_eq!(refined("volume no máximo", "set_volume", json!({"percent": 30}))["percent"], 100);
        assert_eq!(refined("volume em 35%", "set_volume", json!({"percent": 30}))["percent"], 35);
    }
}
