const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const handlers = {};
let active = 0, peak = 0, visits = [], searches = 0, alarms = [];
const event = name => ({ addListener: handler => handlers[name] = handler });
const chrome = {
  runtime: { lastError: null, sendNativeMessage: (_host, payload, callback) => {
    active++; peak = Math.max(peak,active); visits.push(payload);
    setTimeout(() => { active--; callback({ ok:true, syncIntervalSeconds:10 }); },5);
  }, onInstalled:event('installed'), onStartup:event('startup'), onMessage:event('message') },
  alarms: { get:async()=>null, create:async(_name,options)=>alarms.push(options), onAlarm:event('alarm') },
  storage: { local:{ get:async()=>({lastSyncTime:1}),set:async()=>{} } },
  history: { search:async()=>{searches++;return [];},onVisited:event('visited') }
};
const context = vm.createContext({chrome,console,setTimeout,Promise,Date});
vm.runInContext(fs.readFileSync(require('node:path').join(__dirname,'../edge-extension/service_worker.js'),'utf8'),context);
(async()=>{
  handlers.visited({url:'https://a.test'}); handlers.visited({url:'https://b.test'});
  await new Promise(resolve=>setTimeout(resolve,35));
  assert.equal(peak,1,'native writes must be serialized');
  handlers.alarm({name:'historySync'}); handlers.alarm({name:'historySync'});
  await new Promise(resolve=>setTimeout(resolve,35));
  assert.equal(searches,1,'overlapping full syncs must be coalesced');
  assert.ok(alarms.every(alarm=>alarm.periodInMinutes>=0.5),'browser alarm minimum');
  const response=await new Promise(resolve=>assert.equal(handlers.message({action:'favorite',item:{url:'https://saved.test',title:'Saved'}},null,resolve),true));
  assert.equal(response.ok,true);
  assert.equal(visits.at(-1).favorite.url,'https://saved.test');
  let rejected;
  handlers.message({action:'favorite',item:{url:'file:///secret'}},null,result=>rejected=result);
  assert.equal(rejected.ok,false);
  console.log('Extension checks passed: serialization, sync coalescing, alarm minimum, direct save, URL validation.');
})().catch(error=>{console.error(error);process.exitCode=1;});
