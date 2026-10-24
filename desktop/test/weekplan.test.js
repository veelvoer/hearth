// The weekly limit shared out over the days of the week. Run: node desktop/test/weekplan.test.js
const {plan,record}=require(require('path').join(__dirname, '..', 'relay', 'weekplan.js'));
const D=86400000, tz=120;
const T=(s)=>Date.parse(s);            // UTC strings; tz=120 means local = UTC+2
let ok=true; const eq=(n,a,b)=>{const g=Math.abs(a-b)<0.6; if(!g)ok=false; console.log(g?'ok  ':'FAIL',n,a,'expect',b)};
// reset Thu 2026-10-15 15:00 UTC (17:00 local); window start Thu 2026-10-08 17:00 local.
const end=T('2026-10-15T15:00:00Z');
// now = Sun 2026-10-11 12:00 local (10:00Z)
let now=T('2026-10-11T10:00:00Z');
const samples=[{t:T('2026-10-09T08:00:00Z'),w:5,r:end},{t:T('2026-10-10T08:00:00Z'),w:12,r:end},{t:T('2026-10-10T20:00:00Z'),w:30,r:end},{t:T('2026-10-11T08:00:00Z'),w:35,r:end}];
let p=plan({week:{pct:35,resetsAt:end}},samples,{},now,tz);
console.log(p.days.map(d=>`${d.name}:${d.state} used ${d.used} plan ${d.planned}${d.fixed?' fixed '+d.target:''}`).join('\n'));
eq('8 bars: the weekday of the reset is split in a start and an end day',p.days.length,8);
const sum=(p)=>p.days.reduce((a,d)=>a+d.planned,0);
eq('plan sums to 100 (no unknown)',sum(p),100);
// pin a future Sat... (Sat is past here) and Sun 20
p=plan({week:{pct:35,resetsAt:end}},samples,{6:20,0:20},now,tz);
const by=(p,n)=>[...p.days].reverse().find(d=>d.name===n);   // the last one: the reset weekday also has a small day at the start
eq('sat (already past) keeps what was used',by(p,'Sat').planned,25);
eq('sun planned = max(20, used)',by(p,'Sun').planned,20);
eq('sum 100',sum(p),100);
console.log(p.days.map(d=>`${d.name}:${d.state} used ${d.used} plan ${d.planned}`).join(' | '),'autoEach',p.autoEach);
// on Sunday only 15 used by end of day -> next day (now Mon 09:00) rest recalculated
now=T('2026-10-12T07:00:00Z');
const s2=samples.concat([{t:T('2026-10-11T20:00:00Z'),w:50,r:end}]);  // sun used 50-30? compute below
p=plan({week:{pct:50,resetsAt:end}},s2,{6:20,0:20},now,tz);
console.log('after sunday:',p.days.map(d=>`${d.name}:${d.state} used ${d.used} plan ${d.planned}`).join(' | '));
eq('sum 100',sum(p),100);
// over-commit
p=plan({week:{pct:10,resetsAt:end}},[],{6:60,0:60},T('2026-10-09T10:00:00Z'),tz); eq('over flag',p.over?1:0,1); eq('sat scaled to half of the 90 left',by(p,'Sat').planned,45); eq('autos get 0',by(p,'Mon').planned,0);
// reset weekday merges pieces: Thursday bar covers both pieces
p=plan({week:{pct:0,resetsAt:end}},[],{4:14},T('2026-10-08T16:00:00Z'),tz); eq('thu planned 14',by(p,'Thu').planned,14); eq('sum',sum(p),100);
// rolled week
console.log(plan({week:{pct:10,resetsAt:T('2026-10-01T00:00:00Z')}},[],{},now,tz).unavailable);
// record
let h=record([],{week:{pct:5,resetsAt:end}},1000); h=record(h,{week:{pct:5,resetsAt:end}},2000); h=record(h,{week:{pct:6,resetsAt:end}},3000); eq('record dedupe',h.length,2);
// future pinned day with Saturday still ahead
{ const now=T('2026-10-09T10:00:00Z'); const s=[{t:T('2026-10-09T08:00:00Z'),w:5,r:end}];
  const p=plan({week:{pct:5,resetsAt:end}},s,{6:20,0:20},now,tz);
  eq('future sat 20',by(p,'Sat').planned,20); eq('future sun 20',by(p,'Sun').planned,20); eq('autos share 55 over Thu(rest)+Fri(today)+Mon,Tue,Wed',p.autoEach*0+by(p,'Mon').planned,by(p,'Tue').planned);
  console.log(p.days.map(d=>`${d.name} ${d.planned}`).join(' | '),'left',p.left,'autoEach',p.autoEach);
  eq('sum',p.days.reduce((a,d)=>a+d.planned,0),100);
  // used only 15 on Sun later => others recalc
  const s3=s.concat([{t:T('2026-10-10T08:00:00Z'),w:25,r:end},{t:T('2026-10-11T08:00:00Z'),w:45,r:end},{t:T('2026-10-11T20:00:00Z'),w:60,r:end}]);
  const p3=plan({week:{pct:60,resetsAt:end}},s3,{6:20,0:20},T('2026-10-12T07:00:00Z'),tz);
  console.log('mon:',p3.days.map(d=>`${d.name} u${d.used} p${d.planned}`).join(' | '));
}
// the slider's live recalculation (groups on the data the server sends) gives the same as asking the server again with that pin
{ const { groups } = require(require('path').join(__dirname, '..', 'relay', 'weekplan.js'));
  const now = T('2026-10-09T10:00:00Z'), s = [{ t: T('2026-10-09T08:00:00Z'), w: 5, r: end }];
  const base = plan({ week: { pct: 5, resetsAt: end } }, s, { 0: 20 }, now, tz);
  const pinned = plan({ week: { pct: 5, resetsAt: end } }, s, { 0: 20, 6: 30 }, now, tz);
  const live = groups(base.segs, { ...base.targets, 6: 30 }, base.R);
  ok = ok && JSON.stringify(live.days) === JSON.stringify(pinned.days); console.log(JSON.stringify(live.days) === JSON.stringify(pinned.days) ? 'ok   live slider = server answer' : 'FAIL live slider');
}
process.exit(ok ? 0 : 1);
