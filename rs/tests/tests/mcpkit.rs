// The MCP server kit: packed copy is current, the kit passes its own check, and "build one" starts a chat in a fresh project folder.
// Cases 2-4 run the kit's own check.js with node (the kit is a Node product); without node on PATH they are skipped and counted as such.
use hearth_tests::*;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const CLAUDE: &str = r#"#!/usr/bin/env python3
import sys, os, json
t = sys.stdin.read()
open(os.environ['HOME'] + '/prompts.txt', 'a').write(os.getcwd() + '\n' + t + '\n----\n')
print(json.dumps({'type': 'system', 'subtype': 'init', 'session_id': 'mcp-%d' % os.getpid()})); print(json.dumps({'type': 'result', 'is_error': False, 'result': 'ok'}))
"#;

fn node_ok() -> bool { Command::new("node").arg("--version").output().map(|o| o.status.success()).unwrap_or(false) }

#[test]
fn mcpkit() {
    let env = Env::new("mcpkit");
    let mut c = Checks::new();
    let kit_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../desktop/relay/mcpkit");
    let files = fs::read_dir(&kit_dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect::<Vec<_>>();
    let packed = hearth_relay::mcpkit::FILES;
    c.ok("1 the packed kit equals the files (run node scripts/build-mcpkit.js when this fails)",
        files.iter().all(|f| packed.iter().any(|(n, t)| n == f && *t == fs::read_to_string(kit_dir.join(f)).unwrap())) && packed.len() == files.len());
    let k = env.t.join("kit"); fs::create_dir_all(&k).unwrap();
    for (f, t) in packed.iter() { fs::write(k.join(f), t).unwrap(); }
    let node = node_ok();
    if node {
        let o = Command::new("node").arg(k.join("check.js")).output().unwrap(); let out = String::from_utf8_lossy(&o.stdout).to_string();
        c.ok_x("2 the starter kit passes its own check", o.status.success() && re("All good", &out), out.chars().rev().take(120).collect::<String>().chars().rev().collect::<String>());
        fs::write(k.join("tools.js"), "module.exports=[{name:'Bad Name',description:'x',inputSchema:{type:'object',properties:{a:{type:'string'}},required:['a']},run:async()=>'x',sample:{a:'1'}}];").unwrap();
        let o2 = Command::new("node").arg(k.join("check.js")).output().unwrap(); let out2 = String::from_utf8_lossy(&o2.stdout).to_string();
        c.ok("3 the check catches a bad name, a short description and an undescribed argument", o2.status.code() == Some(1) && re("name must be lowercase", &out2) && re("description is missing or too short", &out2) && re("needs a description", &out2));
        fs::write(k.join("tools.js"), "module.exports=[{name:'boom',description:'Always throws an error on purpose.',inputSchema:{type:'object',properties:{}},run:async()=>{throw new Error('nope')},sample:{}}];").unwrap();
        c.ok("4 a tool that throws is reported, and a sample that should fail may fail", Command::new("node").arg(k.join("check.js")).output().unwrap().status.code() == Some(1));
    } else { for n in ["2 the starter kit passes its own check", "3 the check catches a bad name, a short description and an undescribed argument", "4 a tool that throws is reported, and a sample that should fail may fail"] { println!("SKIP {n}  (no node on PATH)"); c.skipped += 1; } }
    env.stub("claude", CLAUDE);
    let relay = Relay::start(&env, Opts::new());
    let a = relay.api(); let tok = relay.token.clone();
    c.ok("5 it needs to be told what to connect to", a.t("POST", "/mcp/build", &tok, Some(json!({"target": ""}))).code == 400);
    c.ok("6 a guest key cannot start one", a.t("POST", "/mcp/build", "share.abcd.abcdefghijklmnop", Some(json!({"target": "Inkscape"}))).code == 401);
    let r = a.t("POST", "/mcp/build", &tok, Some(json!({"target": "Inkscape (vector graphics)", "notes": "export to PNG too"}))); sleep(1500);
    let dir = env.proj.join("mcp-inkscape-vector-graphics");
    c.ok_x("7 a project folder with the kit is made", r.code == 200 && ["server.js", "tools.js", "check.js", "register.js"].iter().all(|f| dir.join(f).exists()), r.j.to_string());
    let pr = env.read("home/prompts.txt");
    c.ok("8 a chat starts in that folder with clear steps, the target and the notes", pr.contains(&dir.to_string_lossy().to_string()) && re(r"Inkscape \(vector graphics\)", &pr) && re("export to PNG too", &pr) && re(r"node check\.js", &pr) && re("node register\\.js mcp-inkscape-vector-graphics", &pr) && re("(?i)never write a secret", &pr));
    c.finish();
}
