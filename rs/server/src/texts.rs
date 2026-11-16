//! Every text file the installer writes (systemd units, Caddyfile, Dockerfile, compose). Pure functions, so they are tested for exact output.

/// One word of an ExecStart line, quoted the way systemd wants (and % and $ doubled so systemd does not expand them).
pub fn sd_word(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "_./:=@+,-".contains(c)) { return s.to_string(); }
    let mut o = String::from("\"");
    for c in s.chars() {
        match c { '"' => o.push_str("\\\""), '\\' => o.push_str("\\\\"), '%' => o.push_str("%%"), '$' => o.push_str("$$"), '\n' => o.push_str("\\n"), c => o.push(c) }
    }
    o.push('"'); o
}

pub struct UnitArgs<'a> { pub user: &'a str, pub home: &'a str, pub relay_bin: &'a str, pub work_dir: &'a str, pub cfg_dir: &'a str, pub projects: &'a str, pub url: &'a str, pub name: &'a str, pub host: &'a str, pub port: u16 }

/// The system service of a server install: runs the Rust `hearth-relay` directly (no Node).
pub fn unit_text(a: &UnitArgs) -> String {
    let exec = [a.relay_bin.to_string(), "--host".into(), a.host.into(), "--port".into(), a.port.to_string(), "--projects".into(), a.projects.into(), "--dir".into(), a.cfg_dir.into(), "--url".into(), a.url.into(), "--name".into(), a.name.into()]
        .iter().map(|w| sd_word(w)).collect::<Vec<_>>().join(" ");
    format!("[Unit]
Description=Hearth server (Claude Code chats for your phone and computers)
After=network-online.target
Wants=network-online.target

[Service]
User={user}
WorkingDirectory={work}
ExecStart={exec}
Environment=HOME={home}
Environment=PATH=/usr/local/bin:/usr/bin:/bin:{home}/.local/bin
Restart=always
RestartSec=3
NoNewPrivileges=yes
PrivateTmp=yes
ProtectSystem=full
ProtectKernelTunables=yes
ProtectControlGroups=yes

[Install]
WantedBy=multi-user.target
", user = a.user, work = a.work_dir, home = a.home)
}

/// The user service of a laptop/desktop install (install-node.sh): LAN pairing by code, optional voice engine.
pub fn local_unit_text(relay_bin: &str, home: &str, cfg: &str, seed_and_voice: &[String]) -> String {
    let mut words: Vec<String> = vec![relay_bin.into(), "--lan".into(), "--host".into(), "0.0.0.0".into(), "--port".into(), "47601".into(), "--dir".into(), cfg.into(), "--move-root".into(), format!("{home}/projects")];
    words.extend(seed_and_voice.iter().cloned());
    format!("[Unit]
Description=Hearth chat relay (local network)
After=network-online.target

[Service]
ExecStart={}
Environment=PATH={home}/.local/bin:/usr/local/bin:/usr/bin
Restart=always
RestartSec=3

[Install]
WantedBy=default.target
", words.iter().map(|w| sd_word(w)).collect::<Vec<_>>().join(" "))
}

pub fn caddy_text(domain: &str, upstream: &str, import_dir: Option<&str>) -> String {
    let import = import_dir.map(|d| format!("\timport {d}/*.caddy\n")).unwrap_or_default();
    format!("# Written by the Hearth installer
{domain} {{
\theader {{
\t\tStrict-Transport-Security \"max-age=31536000\"
\t\tX-Content-Type-Options \"nosniff\"
\t\t-Server
\t}}
{import}\treverse_proxy {upstream} {{
\t\tflush_interval -1
\t}}
}}
")
}

fn json_str(s: &str) -> String { serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into()) }

/// The relay is a static binary; Node is only here because Claude Code itself is installed with npm.
pub fn dockerfile_text(port: u16) -> String {
    format!("FROM node:22-slim
RUN apt-get update && apt-get install -y --no-install-recommends git ca-certificates && rm -rf /var/lib/apt/lists/* \\
 && npm install -g @anthropic-ai/claude-code
COPY --chmod=755 hearth-relay /usr/local/bin/hearth-relay
USER node
ENV HOME=/home/node
EXPOSE {port}
CMD [\"hearth-relay\", \"--host\", \"0.0.0.0\", \"--port\", \"{port}\", \"--projects\", \"/data/projects\", \"--dir\", \"/data/config\"]
")
}

pub fn compose_text(url: &str, name: &str, with_caddy: bool, port: u16) -> String {
    let ports = if with_caddy { format!("    ports:\n      - \"127.0.0.1:{port}:{port}\"\n") } else { format!("    ports:\n      - \"{port}:{port}\"\n") };
    let caddy = if with_caddy { "  caddy:
    image: caddy:2
    restart: unless-stopped
    ports:
      - \"80:80\"
      - \"443:443\"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile:ro
      - ./data/caddy:/data
    depends_on:
      - hearth
" } else { "" };
    format!("# Written by the Hearth installer. Start: docker compose up -d
services:
  hearth:
    build: .
    restart: unless-stopped
    command: [\"hearth-relay\", \"--host\", \"0.0.0.0\", \"--port\", \"{port}\", \"--projects\", \"/data/projects\", \"--dir\", \"/data/config\", \"--url\", {}, \"--name\", {}]
    volumes:
      - ./data/projects:/data/projects
      - ./data/config:/data/config
      - ./data/claude:/home/node/.claude
      - ./data/claude.json:/home/node/.claude.json
{ports}{caddy}", json_str(url), json_str(name))
}

/// The wrapper in /usr/local/bin: `hearth-server` (or `hearth-<name>-server`) always talks to its own install.
pub fn wrapper_text(install_dir: &str, instance: &str) -> String {
    format!("#!/bin/sh\nexec {install_dir}/hearth-server {}\"$@\"\n", if instance == "hearth" { String::new() } else { format!("--instance {instance} ") })
}

/// The optional voice engine (Python) as a user service on 127.0.0.1:47602.
pub fn voice_unit_text(py: &str, dest: &str, home: &str) -> String {
    format!("[Unit]
Description=Hearth relay (LAN only)
After=network-online.target

[Service]
ExecStart={py} {dest}/claude_relay.py
Environment=PATH={home}/.local/bin:/usr/local/bin:/usr/bin
Environment=CM_PORT=47602
Environment=CM_SIDECAR=1
Restart=always
RestartSec=3

[Install]
WantedBy=default.target
")
}

pub fn dutch_unit_text(venv: &str, base: &str) -> String {
    format!("[Unit]
Description=Hearth Dutch voice (XTTS-v2)

[Service]
Environment=COQUI_TOS_AGREED=1
ExecStart={venv}/bin/python {base}/nl_tts_server.py
Restart=always
RestartSec=5
Nice=5

[Install]
WantedBy=default.target
")
}
