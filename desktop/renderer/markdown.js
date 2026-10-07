'use strict';
/* A small, safe Markdown renderer (builds DOM nodes, never injects HTML). */
(function () {
  const el = (tag, cls, text) => { const e = document.createElement(tag); if (cls) e.className = cls; if (text != null) e.textContent = text; return e; };

  function inline(text, parent) {
    const re = /(`[^`\n]+`)|(\*\*[^*\n]+\*\*)|(\*[^*\s][^*\n]*\*)|(\[[^\]\n]+\]\(https?:\/\/[^)\s]+\))|(https?:\/\/[^\s<>)\]]+)/g;
    let last = 0, m;
    const link = (label, url) => {
      const a = el('a', 'lnk', label); a.href = '#'; a.title = url;
      a.onclick = (e) => { e.preventDefault(); cm.openExternal(url); };
      return a;
    };
    while ((m = re.exec(text))) {
      if (m.index > last) parent.append(text.slice(last, m.index));
      const t = m[0];
      if (m[1]) parent.append(el('code', 'ic', t.slice(1, -1)));
      else if (m[2]) { const b = el('strong'); inline(t.slice(2, -2), b); parent.append(b); }
      else if (m[3]) { const i = el('em'); inline(t.slice(1, -1), i); parent.append(i); }
      else if (m[4]) { const mm = /^\[([^\]]+)\]\((.+)\)$/.exec(t); parent.append(link(mm[1], mm[2])); }
      else parent.append(link(t, t));
      last = re.lastIndex;
    }
    if (last < text.length) parent.append(text.slice(last));
  }

  function codeBlock(lang, code) {
    const box = el('div', 'code');
    const head = el('div', 'code-h');
    head.append(el('span', '', lang || 'code'));
    const copy = el('button', 'link', 'Copy');
    copy.onclick = () => { navigator.clipboard.writeText(code); copy.textContent = 'Copied'; setTimeout(() => { copy.textContent = 'Copy'; }, 1200); };
    head.append(copy);
    const pre = el('pre'); pre.append(el('code', '', code));
    box.append(head, pre);
    return box;
  }

  const LIST = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/;

  function render(src) {
    const root = document.createDocumentFragment();
    const lines = String(src).replace(/\r/g, '').split('\n');
    let i = 0;
    while (i < lines.length) {
      const line = lines[i];
      const fence = /^\s*```(\S*)\s*$/.exec(line);
      if (fence) {
        const buf = []; i++;
        while (i < lines.length && !/^\s*```\s*$/.test(lines[i])) buf.push(lines[i++]);
        i++;
        root.append(codeBlock(fence[1], buf.join('\n')));
        continue;
      }
      if (!line.trim()) { i++; continue; }
      let m;
      if ((m = /^(#{1,6})\s+(.*)$/.exec(line))) { const h = el('h' + Math.min(6, m[1].length + 1), 'mdh'); inline(m[2], h); root.append(h); i++; continue; }
      if (/^\s*(-{3,}|\*{3,}|_{3,})\s*$/.test(line)) { root.append(el('hr')); i++; continue; }
      if (/^\s*>/.test(line)) {
        const q = el('blockquote'); const buf = [];
        while (i < lines.length && /^\s*>/.test(lines[i])) buf.push(lines[i++].replace(/^\s*>\s?/, ''));
        q.append(render(buf.join('\n'))); root.append(q); continue;
      }
      if (/^\s*\|/.test(line) && i + 1 < lines.length && /^\s*\|?\s*:?-{2,}/.test(lines[i + 1])) {
        const rows = [];
        while (i < lines.length && /^\s*\|/.test(lines[i])) rows.push(lines[i++]);
        const cells = (r) => r.trim().replace(/^\||\|$/g, '').split('|').map((c) => c.trim());
        const t = el('table', 'mdt'), thead = el('thead'), tr = el('tr');
        cells(rows[0]).forEach((c) => { const th = el('th'); inline(c, th); tr.append(th); });
        thead.append(tr); t.append(thead);
        const tb = el('tbody');
        rows.slice(2).forEach((r) => { const trr = el('tr'); cells(r).forEach((c) => { const td = el('td'); inline(c, td); trr.append(td); }); tb.append(trr); });
        t.append(tb); const wrap = el('div', 'mdt-wrap'); wrap.append(t); root.append(wrap); continue;
      }
      if ((m = LIST.exec(line))) {
        const ordered = /\d/.test(m[2]);
        const list = el(ordered ? 'ol' : 'ul', 'mdl');
        while (i < lines.length && (m = LIST.exec(lines[i]))) {
          const li = el('li'); li.style.marginLeft = Math.min(3, Math.floor(m[1].length / 2)) * 18 + 'px'; inline(m[3], li); list.append(li); i++;
        }
        root.append(list); continue;
      }
      const buf = [];
      while (i < lines.length && lines[i].trim() && !/^\s*(```|#{1,6}\s|>|\|)/.test(lines[i]) && !LIST.test(lines[i])) buf.push(lines[i++]);
      if (!buf.length) { buf.push(lines[i++]); }
      const p = el('p');
      buf.forEach((l, k) => { if (k) p.append(el('br')); inline(l, p); });
      root.append(p);
    }
    return root;
  }

  window.md = render;
  window.mdInline = inline;
})();
