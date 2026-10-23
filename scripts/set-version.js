#!/usr/bin/env node
'use strict';
/* One command to set the version everywhere:  node scripts/set-version.js feature   (or fix, or 1.2.3)
   (desktop app, server relay, phone app). The release workflow checks that they all agree with the tag. */
const fs = require('fs');
const path = require('path');
const root0 = path.join(__dirname, '..');
const current = require(path.join(root0, 'desktop/relay/version.js'));
/* The number tells what changed:  feature  = something new (second number up, last number back to 0)
                                   fix      = only bugs fixed (last number up)
                                   major    = something that breaks the old way (first number up)
   or give the number yourself: 1.2.3 */
const arg = process.argv[2], [cM, cm, cp] = current.split('.').map(Number);
const v = arg === 'feature' ? `${cM}.${cm + 1}.0` : arg === 'fix' ? `${cM}.${cm}.${cp + 1}` : arg === 'major' ? `${cM + 1}.0.0` : arg;
if (!/^\d+\.\d+\.\d+$/.test(v || '')) { console.error('Usage: node scripts/set-version.js feature | fix | major | 1.2.3   (now ' + current + ')'); process.exit(1); }
const root = path.join(__dirname, '..');
const [maj, min, pat] = v.split('.').map(Number);
const edit = (rel, fn) => { const f = path.join(root, rel); fs.writeFileSync(f, fn(fs.readFileSync(f, 'utf8'))); };
edit('desktop/package.json', (s) => s.replace(/"version": "[^"]+"/, `"version": "${v}"`));
fs.writeFileSync(path.join(root, 'desktop/relay/version.js'), `'use strict';\n/* The version of this Hearth. Set by scripts/set-version.js. */\nmodule.exports = '${v}';\n`);
edit('app/build.gradle.kts', (s) => s.replace(/versionCode = \d+/, `versionCode = ${maj * 10000 + min * 100 + pat}`).replace(/versionName = "[^"]+"/, `versionName = "${v}"`));
const notes = path.join(root, 'release-notes', v + '.md');
if (!fs.existsSync(notes)) { fs.mkdirSync(path.dirname(notes), { recursive: true }); fs.writeFileSync(notes, '## What is new\n\n- TODO: write the notes for ' + v + ' (what is new or fixed, in plain words)\n'); console.log('Made release-notes/' + v + '.md: fill it in before you tag.'); }
console.log('Version is now ' + v);
