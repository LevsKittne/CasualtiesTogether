use crate::rules::{self, Presets, Rules, SCHEMA};
use crate::session::{Level, Session};

type Out = Vec<(Level, String)>;

const HELP: &[(&str, &str, bool)] = &[
    ("help", "list commands", false),
    ("players", "list players with id, ping and role", false),
    ("rules [filter]", "show rules (only changed ones without filter, * for all)", false),
    ("rule <name> [value]", "show or set a rule", true),
    ("rules_reset", "reset all rules to defaults", true),
    ("rules_save <preset>", "save current rules as a preset (host)", true),
    ("rules_load <preset>", "load a preset (host)", true),
    ("rules_presets", "list presets (host)", true),
    ("maxplayers <n>", "set PLAYER_COUNT_LIMIT", true),
    ("kick <target> [reason]", "disconnect a player", true),
    ("ban <target> [reason]", "ban and disconnect a player", true),
    ("unban <name|steam:id|ip:addr>", "remove a ban", true),
    ("banlist", "list bans", true),
    ("admin <target>", "give admin rights (host)", true),
    ("deadmin <target>", "remove admin rights (host)", true),
    ("password [new]", "show or set the join password (host)", true),
    ("adminpass [new]", "show or set the admin login password (host)", true),
    ("givehost <target>", "hand the host role to a player (host)", true),
    ("name <new name>", "change your nickname", false),
    ("stash", "list the shared stash", false),
    ("take <id>", "take an item from the shared stash", false),
    ("login <password>", "become admin (clients)", false),
];

impl Session {
    pub(crate) fn exec(&mut self, caller: u16, line: &str) -> Out {
        let line = line.trim();
        let mut parts = line.splitn(2, char::is_whitespace);
        let verb = parts.next().unwrap_or("").to_ascii_lowercase();
        let rest = parts.next().unwrap_or("").trim();
        let is_host = caller == self.host_id;
        let is_admin = is_host || self.players.get(&caller).is_some_and(|p| p.admin);
        let needs_admin = HELP.iter().any(|(sig, _, admin)| *admin && sig.split(' ').next() == Some(verb.as_str()));
        if needs_admin && !is_admin {
            return err(format!("{verb}: admin only, use login <password>"));
        }
        let host_only = matches!(verb.as_str(), "admin" | "deadmin" | "password" | "adminpass" | "rules_save" | "rules_load" | "rules_presets");
        if host_only && !is_host {
            return err(format!("{verb}: host only"));
        }
        match verb.as_str() {
            "" | "help" => HELP
                .iter()
                .filter(|(_, _, admin)| is_admin || !admin)
                .map(|(sig, desc, _)| (Level::Info, format!("{sig} - {desc}")))
                .collect(),
            "players" => self.cmd_players(),
            "rules" => self.cmd_rules(rest),
            "rule" => self.cmd_rule(caller, rest),
            "rules_reset" => {
                self.rules = Rules::default();
                self.rules_changed();
                self.system_chat(&format!("{} reset all rules", self.name_of(caller)));
                ok("rules reset")
            }
            "rules_save" => self.cmd_preset_save(rest),
            "rules_load" => self.cmd_preset_load(rest),
            "rules_presets" => {
                let Some(p) = self.data_path("mp_rule_presets.ini") else { return err("no data dir") };
                let names: Vec<String> = Presets::load(&p).map.keys().cloned().collect();
                ok(if names.is_empty() { "no presets".to_owned() } else { names.join(", ") })
            }
            "maxplayers" => self.cmd_rule(caller, &format!("PLAYER_COUNT_LIMIT {rest}")),
            "kick" | "ban" => self.cmd_kick(caller, &verb, rest),
            "name" => {
                let wanted = crate::proto::sanitize_name(rest);
                if rest.trim().is_empty() {
                    return err("usage: name <new name>");
                }
                if self.players.get(&caller).is_some_and(|p| p.name == wanted) {
                    return ok("that is already your name");
                }
                let name = self.unique_name(&wanted);
                self.ep.broadcast(ct_net::channel::CONTROL, &crate::proto::Msg::Renamed { id: caller, name: name.clone() }.encode(), None);
                self.apply_rename(caller, name);
                Vec::new()
            }
            "stash" => {
                if !self.rules.flag(crate::rules::Id::SharedStash) {
                    return err("shared stash is disabled");
                }
                if self.mods.stash.is_empty() {
                    return ok("the shared stash is empty");
                }
                self.mods.stash.iter().map(|s| (Level::Info, format!("#{} {} {:.0}% from {}", s.id, s.prefab, s.condition * 100.0, s.by))).collect()
            }
            "take" => match rest.parse::<u32>() {
                Ok(id) => {
                    self.on_stash_take(caller, id);
                    Vec::new()
                }
                Err(_) => err("usage: take <id>"),
            },
            "givehost" => {
                if caller != self.local_id || !self.is_host() {
                    return err("only the host can hand over the host role");
                }
                let ids = match self.targets(caller, rest) {
                    Ok(t) => t,
                    Err(e) => return err(e),
                };
                let [id] = ids[..] else { return err("pick exactly one player") };
                match self.transfer_host(id) {
                    Ok(()) => ok(format!("handing the host role to {}", self.name_of(id))),
                    Err(e) => err(e),
                }
            }
            "unban" => {
                let gone = self.bans.remove(rest);
                if gone.is_empty() {
                    err(format!("no ban matches {rest}"))
                } else {
                    gone.into_iter().map(|b| (Level::Info, format!("unbanned {} ({})", b.name, b.key))).collect()
                }
            }
            "banlist" => {
                if self.bans.entries.is_empty() {
                    ok("ban list is empty")
                } else {
                    self.bans.entries.iter().map(|b| (Level::Info, format!("{} {} {}", b.key, b.name, b.reason))).collect()
                }
            }
            "admin" | "deadmin" => {
                let ids = match self.targets(caller, rest) {
                    Ok(t) => t,
                    Err(e) => return err(e),
                };
                let on = verb == "admin";
                let host = self.host_id;
                ids.into_iter()
                    .filter(|id| *id != host)
                    .map(|id| {
                        let name = self.name_of(id);
                        if self.set_admin(id, on) { (Level::Info, format!("{name}: admin = {on}")) } else { (Level::Info, format!("{name}: unchanged")) }
                    })
                    .collect()
            }
            "password" => {
                if rest.is_empty() {
                    ok(if self.password.is_empty() { "no password".to_owned() } else { format!("password: {}", self.password) })
                } else {
                    self.password = if rest == "-" { String::new() } else { rest.to_owned() };
                    ok("password updated")
                }
            }
            "adminpass" => {
                if rest.is_empty() {
                    ok(if self.admin_password.is_empty() { "admin login disabled".to_owned() } else { format!("admin password: {}", self.admin_password) })
                } else {
                    self.admin_password = if rest == "-" { String::new() } else { rest.to_owned() };
                    ok("admin password updated")
                }
            }
            "login" => ok("already on the server side"),
            _ => err(format!("unknown command {verb}, try help")),
        }
    }

    fn cmd_players(&self) -> Out {
        self.players
            .values()
            .map(|p| {
                let role = if p.id == self.host_id { " [host]" } else if p.admin { " [admin]" } else { "" };
                (Level::Info, format!("[{}] {}{role} - {} ms", p.id, p.name, p.ping_ms))
            })
            .collect()
    }

    fn cmd_rules(&self, filter: &str) -> Out {
        let all = filter == "*";
        let f = filter.to_ascii_lowercase();
        let rows: Out = SCHEMA
            .iter()
            .enumerate()
            .filter(|(i, r)| {
                if all {
                    true
                } else if f.is_empty() {
                    self.rules.at(*i) != r.default
                } else {
                    r.name.to_ascii_lowercase().contains(&f)
                }
            })
            .map(|(i, r)| (Level::Info, format!("{} = {}{} (default {})", r.name, self.rules.at(i), r.unit, r.default)))
            .collect();
        if rows.is_empty() { ok(if f.is_empty() { "all rules are default, use rules * to list" } else { "no matching rules" }) } else { rows }
    }

    fn cmd_rule(&mut self, caller: u16, rest: &str) -> Out {
        let mut it = rest.split_whitespace();
        let Some(name) = it.next() else { return err("usage: rule <name> [value]") };
        let Some(idx) = rules::find(name) else { return err(format!("unknown rule {name}")) };
        let r = &SCHEMA[idx];
        let Some(value) = it.next() else {
            return ok(format!("{} = {}{} (range {}..{})", r.name, self.rules.at(idx), r.unit, r.min, r.max));
        };
        let is_admin = caller == self.host_id || self.players.get(&caller).is_some_and(|p| p.admin);
        if !is_admin {
            return err("rule: admin only");
        }
        let v = match rules::parse(idx, value) {
            Ok(v) => v,
            Err(e) => return err(e),
        };
        if !self.rules.set(idx, v) {
            return ok(format!("{} is already {v}", r.name));
        }
        self.rules_changed();
        self.system_chat(&format!("{} set {} = {v}", self.name_of(caller), r.name));
        ok(format!("{} = {v}", r.name))
    }

    fn cmd_preset_save(&mut self, name: &str) -> Out {
        if name.is_empty() || name.contains(['[', ']', '\n']) {
            return err("usage: rules_save <preset>");
        }
        let Some(p) = self.data_path("mp_rule_presets.ini") else { return err("no data dir") };
        let mut presets = Presets::load(&p);
        let body: String = self.rules.diff().into_iter().map(|(i, v)| format!("{} = {v}\n", SCHEMA[i].name)).collect();
        presets.map.insert(name.to_owned(), body);
        match presets.save(&p) {
            Ok(()) => ok(format!("preset {name} saved")),
            Err(e) => err(format!("cannot save presets: {e}")),
        }
    }

    fn cmd_preset_load(&mut self, name: &str) -> Out {
        let Some(p) = self.data_path("mp_rule_presets.ini") else { return err("no data dir") };
        let presets = Presets::load(&p);
        let Some(body) = presets.map.get(name) else { return err(format!("no preset {name}")) };
        let mut r = Rules::default();
        r.apply_ini(body.lines());
        self.rules = r;
        self.rules_changed();
        self.system_chat(&format!("rules preset {name} loaded"));
        ok(format!("preset {name} loaded"))
    }

    fn cmd_kick(&mut self, caller: u16, verb: &str, rest: &str) -> Out {
        let mut parts = rest.splitn(2, char::is_whitespace);
        let spec = parts.next().unwrap_or("");
        let reason = parts.next().unwrap_or("").trim();
        let ids = match self.targets(caller, spec) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        let mut out = Out::new();
        for id in ids {
            let name = self.name_of(id);
            if id == self.host_id {
                out.push((Level::Warning, "cannot kick the host".into()));
                continue;
            }
            if caller != self.host_id && self.players.get(&id).is_some_and(|p| p.admin) {
                out.push((Level::Warning, format!("{name} is an admin, only the host can do that")));
                continue;
            }
            let label = if verb == "ban" { "banned" } else { "kicked" };
            if verb == "ban" {
                if let Some(addr) = self.peer_addr(id) {
                    self.bans.add(&addr, &name, reason);
                }
            }
            let msg = if reason.is_empty() { label.to_owned() } else { format!("{label}: {reason}") };
            if self.kick(id, &msg) {
                self.system_chat(&format!("{name} was {msg}"));
                out.push((Level::Info, format!("{name} {msg}")));
            }
        }
        out
    }

    pub(crate) fn targets(&self, caller: u16, spec: &str) -> Result<Vec<u16>, String> {
        let spec = spec.trim();
        if spec.is_empty() {
            return Err("missing target (name, id:N, @a, @r, @m)".into());
        }
        match spec {
            "@a" => return Ok(self.players.keys().copied().collect()),
            "@m" => return Ok(vec![caller]),
            "@r" => {
                let ids: Vec<u16> = self.players.keys().copied().collect();
                return Ok(vec![ids[ct_net::random_u32() as usize % ids.len()]]);
            }
            _ => {}
        }
        if let Some(n) = spec.strip_prefix("id:") {
            let id: u16 = n.parse().map_err(|_| format!("bad id {n}"))?;
            return if self.players.contains_key(&id) { Ok(vec![id]) } else { Err(format!("no player with id {id}")) };
        }
        if let Some(p) = self.players.values().find(|p| p.name.eq_ignore_ascii_case(spec)) {
            return Ok(vec![p.id]);
        }
        let low = spec.to_ascii_lowercase();
        let hits: Vec<u16> = self.players.values().filter(|p| p.name.to_ascii_lowercase().starts_with(&low)).map(|p| p.id).collect();
        match hits.len() {
            0 => Err(format!("no player matches {spec}")),
            1 => Ok(hits),
            _ => Err(format!("{spec} is ambiguous, use id:N")),
        }
    }
}

fn ok(s: impl Into<String>) -> Out {
    vec![(Level::Info, s.into())]
}

fn err(s: impl Into<String>) -> Out {
    vec![(Level::Warning, s.into())]
}
