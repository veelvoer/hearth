#!/usr/bin/env node
'use strict';
/* One command to set the version everywhere:  node scripts/set-version.js 1.2.3
   (desktop app, server relay, phone app). The release workflow checks that they all agree with the tag. */
const fs = require('fs');
const path = require('path');
const v = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(v || '')) { console.error('Usage: node scripts/set-version.js 1.2.3'); process.exit(1); }
const root = path.join(__dirname, '..');
const [maj, min, pat] = v.split('.').map(Number);
const edit = (rel, fn) => { const f = path.join(root, rel); fs.writeFileSync(f, fn(fs.readFileSync(f, 'utf8'))); };
edit('desktop/package.json', (s) => s.replace(/"version": "[^"]+"/, `"version": "${v}"`));
fs.writeFileSync(path.join(root, 'desktop/relay/version.js'), `'use strict';\n/* The version of this Hearth. Set by scripts/set-version.js. */\nmodule.exports = '${v}';\n`);
edit('app/build.gradle.kts', (s) => s.replace(/versionCode = \d+/, `versionCode = ${maj * 10000 + min * 100 + pat}`).replace(/versionName = "[^"]+"/, `versionName = "${v}"`));
console.log('Version is now ' + v);
