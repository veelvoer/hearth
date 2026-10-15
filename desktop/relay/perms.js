'use strict';
/* "Don't ask again" for commands: the rules a person has allowed for good, and how a new rule is suggested from a command.
   A rule allows Bash commands that start with a prefix ("npm test"). A command with shell operators (; && | > $( ...) is never matched by a prefix,
   because "npm test; rm -rf ~" must not ride along. Some commands are never suggested at all. */
const SHELL_META = /[;&|`<>\n\r]|\$\(|\$\{/;
const NEVER = new Set(['rm', 'sudo', 'su', 'doas', 'dd', 'mkfs', 'chmod', 'chown', 'chgrp', 'mv', 'shutdown', 'reboot', 'halt', 'poweroff', 'kill', 'killall', 'pkill', 'eval', 'exec', 'curl', 'wget', 'ssh', 'scp', 'sh', 'bash', 'zsh', 'fish', 'xargs', 'find', 'env', 'nohup', 'crontab', 'passwd', 'iptables', 'ufw', 'mount', 'umount', 'truncate', 'shred']);
const TWO = new Set(['git', 'npm', 'pnpm', 'yarn', 'npx', 'bun', 'docker', 'podman', 'cargo', 'go', 'pip', 'pip3', 'python', 'python3', 'node', 'systemctl', 'journalctl', 'kubectl', 'make', 'gradle', 'dotnet', 'gh', 'apt', 'brew', 'flutter', 'rustup', 'composer', 'php', 'ruby', 'bundle', 'rake']);
const RUNNERS = new Set(['npm', 'pnpm', 'yarn', 'bun']);
const RISKY_SUB = new Set(['push', 'reset', 'clean', 'rebase', 'checkout', 'rm', 'publish', 'remove', 'uninstall', 'install', 'add', 'exec', 'stop', 'restart', 'disable', 'delete', 'prune', 'kill', 'rmi', 'system', 'force', 'config', 'login', 'logout', 'unpublish']);   // too broad to allow with one tap

const tokens = (cmd) => String(cmd || '').trim().split(/\s+/).filter(Boolean);

/** The rule that would be offered for this tool call, or null when none is safe to offer. */
function suggest(tool, input) {
  if (tool !== 'Bash') return null;
  const cmd = String((input || {}).command || '').trim();
  if (!cmd || SHELL_META.test(cmd)) return null;
  const t = tokens(cmd), base = t[0].replace(/^.*\//, '');
  if (NEVER.has(base) || /^[/~]/.test(t[0]) || t[0].startsWith('..')) return null;
  let prefix = t[0];
  if (RUNNERS.has(base)) {   // npm / pnpm / yarn / bun: "npm test", "npm run build", but never "npm install x" or "npm run" with any script
    if (t[1] === 'run' && t[2] && !t[2].startsWith('-')) prefix = t.slice(0, 3).join(' ');
    else if (t[1] && t[1] !== 'run' && !t[1].startsWith('-') && !RISKY_SUB.has(t[1])) prefix = t[0] + ' ' + t[1];
    else return null;
  } else if (TWO.has(base)) {   // git, docker, cargo...: the subcommand counts, a bare tool or a flag is too broad
    if (!t[1] || t[1].startsWith('-') || RISKY_SUB.has(t[1])) return null;
    prefix = t[0] + ' ' + t[1];
  }
  return { tool: 'Bash', prefix, label: prefix + ' …' };
}

/** Does a rule allow this call? */
function matches(rule, tool, input) {
  if (!rule || rule.tool !== tool || tool !== 'Bash' || !rule.prefix) return false;
  const cmd = String((input || {}).command || '').trim();
  if (!cmd || SHELL_META.test(cmd)) return false;
  return cmd === rule.prefix || cmd.startsWith(rule.prefix + ' ');
}
const anyMatch = (rules, tool, input) => (rules || []).some((r) => matches(r, tool, input));

/** A rule typed by hand: only plain command prefixes. Returns the rule or null. */
function fromPrefix(text) {
  const p = String(text || '').trim().replace(/\s+/g, ' ');
  if (!p || p.length > 80 || SHELL_META.test(p) || NEVER.has(p.split(' ')[0].replace(/^.*\//, ''))) return null;
  return { tool: 'Bash', prefix: p, label: p + ' …' };
}
module.exports = { suggest, matches, anyMatch, fromPrefix, SHELL_META };
