const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const path = require('node:path');

(async () => {
  const state = {lastSyncTime:123, fullSyncPending:false};
  let requested = true, failBatch = true, batches = 0, completed = 0;
  const starts = [];
  const chrome = {
    runtime: { lastError:null, onInstalled:{addListener(){}}, onStartup:{addListener(){}}, onMessage:{addListener(){}},
      sendNativeMessage(_host, payload, callback) {
        queueMicrotask(() => {
          if(payload.records.length) {
            batches++;
            // Simulate an old host clearing the request on the first batch.
            requested = false;
            if(failBatch && batches===2) {
              chrome.runtime.lastError = {message:'simulated interruption'};
              callback(undefined); chrome.runtime.lastError=null; return;
            }
          }
          if(payload.fullSyncComplete)completed++;
          callback({ok:true,fullSyncRequested:requested,syncIntervalSeconds:30});
        });
      }
    },
    alarms:{get:async()=>null,create:async()=>{},onAlarm:{addListener(){}}},
    storage:{local:{get:async()=>({...state}),set:async value=>Object.assign(state,value)}},
    history:{onVisited:{addListener(){}},search:async options=>{starts.push(options.startTime);return Array.from({length:4001},(_,i)=>({url:`https://example.test/${i}`}));}}
  };
  const context=vm.createContext({chrome,console,Promise,Date});
  vm.runInContext(fs.readFileSync(path.join(__dirname,'../edge-extension/service_worker.js'),'utf8'),context);
  await assert.rejects(vm.runInContext('syncHistory()',context),/simulated interruption/);
  assert.equal(state.fullSyncPending,true);
  assert.equal(state.lastSyncTime,123);
  assert.equal(completed,0,'never acknowledge incomplete history');
  failBatch=false;batches=0;
  await vm.runInContext('syncHistory()',context);
  assert.deepEqual(starts,[0,0],'retry all history even when native request disappeared');
  assert.equal(batches,3);
  assert.equal(completed,1);
  assert.equal(state.fullSyncPending,false);
  assert.ok(state.lastSyncTime>123);
  console.log('Full history retry checks passed: interrupted batches, durable pending flag, full retry, completion acknowledgment.');
})().catch(error=>{console.error(error);process.exitCode=1;});
