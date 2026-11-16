// One-tap tools for Claude: only the ready-made ones can be added, with the right command.
use hearth_tests::*;
use serde_json::{json, Value};

fn arr(v: &Value) -> Vec<Value> { v.as_array().cloned().unwrap_or_default() }

#[test]
fn mcp_catalog() {
    let env = Env::new("mcpcat");
    std::fs::create_dir_all(env.proj.join("app")).unwrap();
    let log = env.t.join("args.jsonl");
    env.stub("claude", &format!("#!/usr/bin/env python3\nimport sys, json\nopen({:?}, 'a').write(json.dumps(sys.argv[1:]) + '\\n')\n", log.to_string_lossy()));
    let relay = Relay::start(&env, Opts::new().env("HEARTH_MEM_CHILD", "1"));
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let calls = || -> Vec<Value> { std::fs::read_to_string(&log).unwrap_or_default().trim().lines().filter_map(|l| serde_json::from_str(l).ok()).collect() };
    let cat = arr(&a.t("GET", "/mcp/catalog", &tok, None).j["tools"]);
    let find = |id: &str| cat.iter().find(|x| x["id"] == id).cloned().unwrap_or(json!({}));
    c.ok("1 the catalog lists the ready tools, and which need a secret", cat.len() >= 4 && truthy(&find("github")["needs"]) && !truthy(&find("memory")["needs"]));
    let m = a.t("POST", "/mcp/install", &tok, Some(json!({"id": "memory"})));
    c.ok_x("2 a tool is added with claude mcp add, for the user, with a fixed command", m.code == 200 && m.j["ok"] == true && calls().get(0) == Some(&json!(["mcp","add","--scope","user","memory","--","npx","-y","@modelcontextprotocol/server-memory"])), calls().get(0).map(|x| x.to_string()).unwrap_or_default());
    c.ok("3 a tool that is not in the catalog cannot be added", a.t("POST", "/mcp/install", &tok, Some(json!({"id": "rm -rf /"}))).code == 404 && calls().len() == 1);
    c.ok("4 a tool that needs a secret says so when it is missing, or looks wrong", a.t("POST", "/mcp/install", &tok, Some(json!({"id": "github"}))).code == 400 && a.t("POST", "/mcp/install", &tok, Some(json!({"id": "github", "secret": "a b; rm"}))).code == 400);
    let g = a.t("POST", "/mcp/install", &tok, Some(json!({"id": "github", "secret": "ghp_abcdefghijklmnop1234"})));
    let c1 = arr(&calls().get(1).cloned().unwrap_or(json!([])));
    c.ok("5 the secret is handed over as an environment setting of the tool", g.code == 200 && c1.contains(&json!("-e")) && c1.contains(&json!("GITHUB_PERSONAL_ACCESS_TOKEN=ghp_abcdefghijklmnop1234")));
    let f = a.t("POST", "/mcp/install", &tok, Some(json!({"id": "filesystem"})));
    let c2 = arr(&calls().get(2).cloned().unwrap_or(json!([])));
    c.ok_x("6 the files tool is limited to the projects folder", f.code == 200 && c2.last() == Some(&json!(env.proj.to_string_lossy())), json!(c2).to_string());
    c.ok("7 a guest key cannot add tools", a.t("POST", "/mcp/install", "share.abcd.abcdefghijklmnop", Some(json!({"id": "memory"}))).code == 401);
    c.finish();
}
