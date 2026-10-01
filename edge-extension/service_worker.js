const HOST = 'com.t10.search_launcher_history';
let sending = Promise.resolve();
let syncing = null;

function sendNative(records, fullSync = false, favorite = undefined) {
  const task = sending.catch(() => {}).then(() => new Promise((resolve, reject) => {
    chrome.runtime.sendNativeMessage(HOST, { records, fullSync, favorite }, response => {
      const error = chrome.runtime.lastError;
      if (error) reject(new Error(error.message)); else if (!response?.ok) reject(new Error('Local bridge rejected the message')); else resolve(response);
    });
  }));
  sending = task;
  return task;
}

async function configureAlarm(value) {
  // Chromium alarms have a 30-second minimum. Visits are sent immediately.
  const seconds = Math.max(30, Math.min(3600, Number(value) || 30));
  const minutes = seconds / 60;
  const alarm = await chrome.alarms.get('historySync');
  if (!alarm || alarm.periodInMinutes !== minutes) await chrome.alarms.create('historySync', { periodInMinutes: minutes });
}

function syncHistory() {
  if (syncing) return syncing;
  syncing = (async () => {
    const handshake = await sendNative([]);
    await configureAlarm(handshake.syncIntervalSeconds);
    const stored = await chrome.storage.local.get('lastSyncTime');
    const fullSync = Boolean(handshake.fullSyncRequested);
    const syncStarted = Date.now();
    const startTime = fullSync ? 0 : stored.lastSyncTime ? Math.max(0, stored.lastSyncTime - 300000) : 0;
    const records = await chrome.history.search({ text: '', startTime, maxResults: 100000 });
    if (!records.length) await sendNative([], fullSync);
    for (let index = 0; index < records.length; index += 2000) await sendNative(records.slice(index, index + 2000), fullSync);
    await chrome.storage.local.set({ lastSyncTime: syncStarted });
  })().finally(() => { syncing = null; });
  return syncing;
}

chrome.runtime.onInstalled.addListener(() => configureAlarm(30).then(syncHistory).catch(console.error));
chrome.runtime.onStartup.addListener(() => configureAlarm(30).then(syncHistory).catch(console.error));
chrome.alarms.onAlarm.addListener(alarm => { if (alarm.name === 'historySync') syncHistory().catch(console.error); });
chrome.history.onVisited.addListener(item => sendNative([item]).then(response => configureAlarm(response.syncIntervalSeconds)).catch(console.error));
chrome.runtime.onMessage.addListener((message, _sender, respond) => {
  if (message.action !== 'favorite') return false;
  if (!message.item?.url || !/^https?:\/\//.test(message.item.url)) { respond({ ok: false, error: 'Unsupported page' }); return false; }
  sendNative([], false, message.item).then(response => respond(response)).catch(error => respond({ ok: false, error: error.message }));
  return true;
});
