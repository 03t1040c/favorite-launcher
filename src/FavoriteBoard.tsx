import { useEffect, useRef, useState, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { createPortal } from 'react-dom';

type Tab = { id: number; name: string; color: string; position: number };
type Group = { id: number; tabId: number; name: string; color: string; position: number };
type Item = { id: number; pane: number; kind: string; label: string; target?: string | null };
type Layout = { groups?: Record<string, { width?: number; collapsed?: boolean; column?: number }>; recentGroup?: number };
type Props = {
  tabs: Tab[]; groups: Group[]; items: Item[]; activeTab: number;
  setActiveTab: (id: number) => void; reload: () => Promise<void>;
  renderItem: (id: number, pane: number) => ReactNode;
  moveItem: (id: number, pane: number) => Promise<void>;
  toast: (message: string) => void; limit: number; initialColor: string;
  colors: string[]; children: ReactNode;
};

export default function FavoriteBoard(props: Props) {
  const [layout, setLayout] = useState<Layout>({});
  const [ready, setReady] = useState(false);
  const [search, setSearch] = useState('');
  const [organizing, setOrganizing] = useState(false);
  const [menu, setMenu] = useState<number | null>(null);
  const [menuAnchor, setMenuAnchor] = useState({x:0,y:0});
  const [rename, setRename] = useState<{ id: number; text: string } | null>(null);
  const [drag, setDrag] = useState<number | null>(null);
  const [dragPoint, setDragPoint] = useState({x:0,y:0});
  const [drop, setDrop] = useState<{ type: string; id: number; after?: boolean } | null>(null);
  const dragRef = useRef<{ id: number; x: number; y: number; moved: boolean } | null>(null);
  const saving = useRef(Promise.resolve());
  const layoutRef = useRef(layout);
  const reloadRef = useRef(props.reload); reloadRef.current = props.reload;
  useEffect(() => {
    const close = (event: PointerEvent) => {
      if (!(event.target as HTMLElement)?.closest('.board-menu,.board-menu-toggle')) setMenu(null);
    };
    document.addEventListener('pointerdown',close);
    return () => document.removeEventListener('pointerdown',close);
  },[]);

  useEffect(() => {
    let disposed = false;
    void invoke<string>('prepare_favorite_board').then(async value => {
      if (disposed) return;
      const next = JSON.parse(value) as Layout;
      setLayout(next); layoutRef.current = next;
      await reloadRef.current(); setReady(true);
    }).catch(error => props.toast(`グループの準備に失敗しました: ${error}`));
    return () => { disposed = true; };
  }, []);

  const saveLayout = (next: Layout) => {
    setLayout(next); layoutRef.current = next;
    saving.current = saving.current.catch(() => {}).then(() => invoke<void>('save_board_layout', { value: JSON.stringify(next) })).catch(error => props.toast(`配置の保存に失敗しました: ${error}`));
  };
  const changeLayout = (id: number, change: { collapsed?: boolean; column?: number }) => {
    const current = layoutRef.current;
    saveLayout({ ...current, groups: { ...current.groups, [id]: { ...current.groups?.[id], ...change } } });
  };
  const act = async (action: () => Promise<unknown>) => {
    try { await action(); await props.reload(); } catch (error) { props.toast(String(error)); await props.reload(); }
  };
  const createGroup = () => void act(async () => {
    const id = await invoke<number>('add_favorite_column', { tabId: props.activeTab, limit: props.limit });
    await invoke('update_favorite_column', { id, name: '新しいグループ', color: props.initialColor });
    setRename({ id, text: '新しいグループ' });
  });
  const renameGroup = async (group: Group) => {
    if (!rename) return;
    setRename(null);
    await act(() => invoke('update_favorite_column', { id: group.id, name: rename.text, color: group.color }));
  };
  const targetAt = (x: number, y: number) => {
    const node = document.elementFromPoint(x, y);
    const tab = node?.closest<HTMLElement>('[data-favorite-tab-id]');
    if (tab) return { type: 'tab', id: Number(tab.dataset.favoriteTabId) };
    const group = node?.closest<HTMLElement>('[data-board-group]');
    if (!group) { const column = node?.closest<HTMLElement>('[data-board-column]'); return column ? {type:'column',id:Number(column.dataset.boardColumn)} : null; }
    const rect = group.getBoundingClientRect();
    return { type: 'group', id: Number(group.dataset.boardGroup), after: x > rect.left + rect.width/2 || y > rect.top + rect.height/2 };
  };
  const finishDrag = (x: number, y: number) => {
    const current = dragRef.current; dragRef.current = null;
    setDrag(null); setDrop(null);
    if (!current?.moved) return;
    const target = targetAt(x, y);
    if (!target) return;
    const group = props.groups.find(g => g.id === current.id);
    if (!group) return;
    void act(async () => {
      if (target.type === 'tab') {
        if (group.tabId !== target.id) await invoke('move_favorite_column', { id: group.id, tabId: target.id, limit: props.limit });
      } else if (target.type === 'column') {
        changeLayout(group.id, {column:target.id});
      } else if (target.id !== group.id) {
        changeLayout(group.id, {column:columnOf(props.groups.find(g => g.id === target.id)!)});
        const ids = props.groups.filter(g => g.tabId === group.tabId && g.id !== group.id).map(g => g.id);
        ids.splice(Math.max(0, ids.indexOf(target.id)) + ('after' in target && target.after ? 1 : 0), 0, group.id);
        await invoke('place_favorite_columns', { tabId: group.tabId, ids });
      }
    });
  };

  const text = search.trim().toLocaleLowerCase();
  const matches = props.items.filter(item => item.kind === 'link' && `${item.label} ${item.target ?? ''}`.toLocaleLowerCase().includes(text));
  const visibleGroups = props.groups.filter(group => text ? matches.some(item => item.pane === group.id) || group.name.toLocaleLowerCase().includes(text) : group.tabId === props.activeTab);
  const columnOf = (group: Group) => Math.min(props.limit - 1, layout.groups?.[group.id]?.column ?? props.groups.filter(g => g.tabId === group.tabId).findIndex(g => g.id === group.id) % props.limit);

  useEffect(() => {
    if (!ready) return;
    const next = {...layoutRef.current, groups:{...layoutRef.current.groups}, recentGroup:undefined};
    let changed = layoutRef.current.recentGroup !== undefined;
    props.groups.forEach(group => { if (next.groups[group.id]?.column === undefined) { next.groups[group.id] = {...next.groups[group.id], column:columnOf(group)}; changed = true; } });
    if (changed) saveLayout(next);
  },[ready,props.groups,props.limit]);

  return <>
    <div className="board-controls"><input aria-label="全タブのお気に入りを検索" placeholder="お気に入りを検索（すべてのタブ）" value={search} onChange={event => setSearch(event.target.value)} /><button className={organizing ? 'active' : ''} onClick={() => setOrganizing(!organizing)}>{organizing ? '整理を完了' : '整理'}</button><button onClick={createGroup}>＋ グループ</button></div>
    <div className={`favorite-panes board-grid${organizing ? ' organizing' : ''}`} style={{gridTemplateColumns:`repeat(${props.limit},minmax(0,1fr))`}} onClick={() => setMenu(null)}>
      {!ready ? <div className="board-empty">お気に入りを準備しています…</div> : <>
        {Array.from({length:props.limit},(_,column) => <div className={`board-column${drop?.type === 'column' && drop.id === column ? ' column-drop-tab' : ''}`} data-board-column={column} key={column}><div className="board-column-caption">列 {column+1}</div>{visibleGroups.filter(group => columnOf(group) === column).map(group => {
          const config = layout.groups?.[group.id] ?? {};
          const items = props.items.filter(item => item.pane === group.id && item.kind === 'link' && (!text || group.name.toLocaleLowerCase().includes(text) || matches.includes(item)));
          return <section key={group.id} data-board-group={group.id} data-favorite-pane={group.id} className={`board-group${drag === group.id ? ' board-dragging' : ''}${drop?.type === 'group' && drop.id === group.id ? ` board-drop${drop.after ? ' board-drop-after' : ''}` : ''}`} style={{ borderTopColor: group.color }}>
            <header>
              <button className="board-grip" aria-label={`${group.name}をまとめて移動`} title="つかんでグループを移動。タブへドロップすると中身ごと移動します。" onPointerDown={event => { if (event.button !== 0) return; dragRef.current = { id: group.id, x: event.clientX, y: event.clientY, moved: false }; event.currentTarget.setPointerCapture(event.pointerId); }} onPointerMove={event => { const current = dragRef.current; if (!current) return; if (Math.hypot(event.clientX - current.x, event.clientY - current.y) > 5) current.moved = true; if (current.moved) { setDrag(group.id); setDragPoint({x:event.clientX,y:event.clientY}); const target = targetAt(event.clientX, event.clientY); setDrop(target); document.querySelectorAll('[data-favorite-tab-id]').forEach(node => node.classList.toggle('column-drop-tab', target?.type === 'tab' && Number((node as HTMLElement).dataset.favoriteTabId) === target.id)); } }} onPointerUp={event => { if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId); finishDrag(event.clientX, event.clientY); document.querySelectorAll('.column-drop-tab').forEach(node => node.classList.remove('column-drop-tab')); }} onPointerCancel={() => { dragRef.current = null; setDrag(null); setDrop(null); document.querySelectorAll('.column-drop-tab').forEach(node => node.classList.remove('column-drop-tab')); }}>⠿</button>
              {rename?.id === group.id ? <input autoFocus value={rename.text} onFocus={event => event.target.select()} onChange={event => setRename({ id: group.id, text: event.target.value })} onKeyDown={event => { event.stopPropagation(); if (event.key === 'Enter') { event.preventDefault(); void renameGroup(group); } if (event.key === 'Escape') setRename(null); }} onBlur={() => void renameGroup(group)} /> : <button className="board-title" title="クリックで折り畳み。ダブルクリックで名前変更" onDoubleClick={() => setRename({ id: group.id, text: group.name })} onClick={() => changeLayout(group.id, { collapsed: !config.collapsed })}>{config.collapsed ? "▸ " : "▾ "}{group.name}</button>}
              <span>{items.length}</span><button className="board-menu-toggle" title="グループ設定" aria-label={`${group.name}の設定`} onClick={event => { event.stopPropagation(); const rect=event.currentTarget.getBoundingClientRect(); setMenuAnchor({x:Math.max(8,Math.min(window.innerWidth-240,rect.right-230)),y:Math.max(8,Math.min(window.innerHeight-390,rect.bottom+5))}); setMenu(menu === group.id ? null : group.id); }}>⋯</button>
            </header>
            {text && <small className="board-location">{props.tabs.find(tab => tab.id === group.tabId)?.name}</small>}
{menu === group.id && createPortal(<div className="board-menu" style={{position:"fixed",left:menuAnchor.x,top:menuAnchor.y,maxHeight:"min(380px,80vh)",overflowY:"auto"}} onClick={event => event.stopPropagation()}><button onClick={() => { setRename({ id: group.id, text: group.name }); setMenu(null); }}>名前を変更</button><label>表示する列<select value={columnOf(group)} onChange={event => changeLayout(group.id,{column:Number(event.target.value)})}>{Array.from({length:props.limit},(_,index)=><option key={index} value={index}>列 {index+1}</option>)}</select></label><div className="board-colors">{props.colors.map(color => <button key={color} aria-label={`色 ${color}`} style={{ background: color }} onClick={() => void act(() => invoke('update_favorite_column', { id: group.id, name: group.name, color }))} />)}</div><label>タブへ移動<select value={group.tabId} onChange={event => void act(() => invoke('move_favorite_column', { id: group.id, tabId: Number(event.target.value), limit: props.limit }))}>{props.tabs.map(tab => <option key={tab.id} value={tab.id}>{tab.name}</option>)}</select></label><button className="danger" onClick={() => { if (confirm(`「${group.name}」を削除しますか？リンクは検索下の追加されたお気に入りへ移動します。`)) void act(() => invoke('delete_favorite_column', { id: group.id, targetPane: 0 })); }}>グループを削除</button></div>, document.body)}
            {!config.collapsed || text ? <div className="favorites-list unified">{items.length ? items.map(item => <div key={item.id}>{props.renderItem(item.id, group.id)}{organizing && <select aria-label={`${item.label}の移動先`} value={item.pane} onChange={event => void act(() => props.moveItem(item.id, Number(event.target.value)))}><option value={0}>未整理</option>{props.groups.map(target => <option key={target.id} value={target.id}>{props.tabs.find(tab => tab.id === target.tabId)?.name} / {target.name}</option>)}</select>}</div>) : <div className="board-empty">ここへリンクをドラッグ</div>}</div> : null}
          </section>;
        })}<button className="column-add-group" onClick={() => void act(async () => {const id = await invoke<number>('add_favorite_column',{tabId:props.activeTab,limit:100}); await invoke('update_favorite_column',{id,name:'新しいグループ',color:props.initialColor}); changeLayout(id,{column}); setRename({id,text:'新しいグループ'});})}>＋ グループ</button></div>)}
        {text && !visibleGroups.length && <div className="board-empty">一致するお気に入りはありません。</div>}
      </>}
    </div>
    {drag && createPortal(<div className="board-drag-preview" style={{left:dragPoint.x+14,top:dragPoint.y+14}}><strong>{props.groups.find(group=>group.id===drag)?.name}</strong><small>{props.items.filter(item=>item.pane===drag && item.kind==='link').length}件のリンクをまとめて移動</small></div>,document.body)}
  </>;
}
