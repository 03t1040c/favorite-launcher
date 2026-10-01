const status = document.getElementById('status');
const save = document.getElementById('save');
document.getElementById('extension-id').value = chrome.runtime.id;
document.getElementById('copy').onclick = async () => {
  await navigator.clipboard.writeText(chrome.runtime.id);
  status.textContent = '拡張機能IDをコピーしました。';
};
chrome.tabs.query({ active: true, currentWindow: true }).then(([tab]) => {
  document.getElementById('page').textContent = tab?.title || tab?.url || 'ページを選択してください';
  if (!tab?.url || !/^https?:\/\//.test(tab.url)) { save.disabled = true; status.textContent = '通常のWebページで使用してください。'; return; }
  save.onclick = async () => {
    save.disabled = true;
    try {
      const response = await chrome.runtime.sendMessage({ action: 'favorite', item: { title: tab.title, url: tab.url, lastVisitTime: Date.now() } });
      if (!response?.ok) throw new Error(response?.error || '接続できません');
      status.textContent = response.duplicate ? `すでに登録済み、または送信済みです。${response.location ? `保存先：${response.location}` : 'アプリ内の検索で確認できます。'}` : 'アプリへ送信しました。起動中ならすぐに反映されます。';
    } catch { status.textContent = 'アプリに接続できません。アプリをインストールし、設定から拡張機能IDを登録してください。'; save.disabled = false; }
  };
});
