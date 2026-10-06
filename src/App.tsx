import { useEffect, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent, type PointerEvent as ReactPointerEvent } from "react";
import { createPortal } from "react-dom";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import "./compact.css";
import "./board.css";
import FavoriteBoard from "./FavoriteBoard";

type ServiceType = "spo" | "asana" | "notion" | "edge" | "folder";
type UiPreferences = { showFavoriteSearch: boolean; resetTabAfterLink: boolean };
const defaultUiPreferences: UiPreferences = {showFavoriteSearch:true,resetTabAfterLink:false};

type SearchResult = {
  id: number;
  service: ServiceType;
  title: string;
  site?: string | null;
  url: string;
  firstSeen?: string | null;
  lastAccess?: string | null;
  accessCount: number;
  isFavorite: number;
};

type Settings = {
  displayCount: number;
  quickKeywords: string[];
  blockedWords: string[];
  siteNames: string[];
  searchMode: "web" | "folder";
  windowSize: string;
  indexFolders: string[];
  favoritePaneCount: number;
  leftPanePercent: number;
  favoriteDensity: "compact" | "standard" | "comfortable";
  favoriteTabLimit: number;
  headingDefaultColor: string;
  headingPaletteSize: 32 | 64 | 128;
  edgeSyncIntervalSeconds: number;
  folderSyncIntervalSeconds: number;
};

type FavoriteItem = { id: number; kind: "link" | "heading"; historyId?: number | null; label: string; service?: ServiceType | null; target?: string | null; position: number; pane: number; color: string; openedCount: number; lastOpenedAt?: string | null; deletedAt?: string | null };
type FavoriteTab = { id:number; name:string; color:string; position:number };
type FavoriteColumn = { id:number; tabId:number; name:string; color:string; position:number };
type FileSearchResult = { id: number; name: string; path: string; parent: string; isDirectory: boolean; modifiedAt?: string | null; isFavorite: boolean };
type IndexUpdateSummary = { scanned: number; added: number; updated: number; deleted: number };

type SettingsResponse = {
  settings: Settings;
  isDefault: boolean;
};

type EdgeImportDebugEntry = {
  timestamp: string;
  stage: string;
  level: string;
  message: string;
};

type EdgeImportStatus = {
  running: boolean;
  message: string;
  lastOutcome?: "success" | "failure" | null;
  lastUpdatedAt?: string | null;
  revision: number;
  debugLog: EdgeImportDebugEntry[];
};
type AppLogEntry = { id: number; timestamp: string; category: string; level: "info" | "success" | "warning" | "error"; message: string };
type ExtensionConnectionStatus = { connected: boolean; lastContact?: string | null };

const defaultQuickKeywords = [
  "Quick1",
  "Quick2",
  "Quick3",
  "Quick4",
  "Quick5",
  "Quick6",
  "Quick7",
  "Quick8",
  "Quick9",
  "Quick10",
];

const defaultSettings: Settings = {
  displayCount: 0,
  quickKeywords: defaultQuickKeywords,
  blockedWords: [],
  siteNames: [],
  searchMode: "web",
  windowSize: "1100x760",
  indexFolders: [],
  favoritePaneCount: 5,
  leftPanePercent: 50,
  favoriteDensity: "standard",
  favoriteTabLimit: 5,
  headingDefaultColor: "#dbeafe",
  headingPaletteSize: 32,
  edgeSyncIntervalSeconds: 10,
  folderSyncIntervalSeconds: 1800,
};
const presetWindowSizes = ["1100x760", "1200x800", "1280x840", "1360x900"];

function ServiceDot({ service }: { service: ServiceType }) {
  const marks: Record<ServiceType, string> = { asana: "", notion: "", spo: "", edge: "", folder: "" };
  return <span className={`service-dot ${service}`} aria-label={service === "spo" ? "SharePoint" : service}>{marks[service]}</span>;
}

function websiteFaviconUrl(target?: string | null) {
  if (!target) return undefined;
  try { return `${new URL(target).origin}/favicon.ico`; } catch { return undefined; }
}

function SourceIcon({ src, service }: { src?: string; service: ServiceType }) {
  const [failed, setFailed] = useState(false);
  useEffect(() => setFailed(false), [src]);
  return src && !failed
    ? <img className="source-icon" src={src} alt="" onError={() => setFailed(true)} />
    : <ServiceDot service={service} />;
}

function processTitleForDisplay(title: string, blockedWords: string[]) {
  let display=title;
  for(const word of blockedWords || []) {
    if(word) display=display.replace(new RegExp(word.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"),"gi"),"");
  }
  display=display.replace(/^[\s\-_:–—]+|[\s\-_:–—]+$/g,"").replace(/[\s\-_:–—]{2,}/g," ").trim();
  return {displayTitle:display || "名称未設定"};
}

function SettingHelp({ text }: { text: string }) {
  const anchorRef = useRef<HTMLSpanElement>(null);
  const [position, setPosition] = useState<{ left: number; top: number; below: boolean } | null>(null);
  const show = () => {
    const rect = anchorRef.current?.getBoundingClientRect();
    if (!rect) return;
    const width = Math.min(320, window.innerWidth - 24);
    const left = Math.max(12, Math.min(window.innerWidth - width - 12, rect.left + rect.width / 2 - width / 2));
    const below = rect.top < 150;
    setPosition({ left, top: below ? rect.bottom + 8 : rect.top - 8, below });
  };
  const hide = () => setPosition(null);
  return <>
    <span ref={anchorRef} className="setting-help-tip" tabIndex={0} aria-label={text} onMouseEnter={show} onMouseLeave={hide} onFocus={show} onBlur={hide}>?</span>
    {position ? createPortal(<div className={`setting-help-popup${position.below ? " below" : ""}`} style={{ left: position.left, top: position.top }}>{text}</div>, document.body) : null}
  </>;
}

const baseHeadingColors = [
  "#f8fafc", "#f1f5f9", "#e2e8f0", "#dbeafe", "#bfdbfe", "#e0f2fe", "#bae6fd", "#cffafe",
  "#ccfbf1", "#d1fae5", "#dcfce7", "#ecfccb", "#fef9c3", "#fef3c7", "#ffedd5", "#fee2e2",
  "#ffe4e6", "#fce7f3", "#fae8ff", "#f3e8ff", "#ede9fe", "#e0e7ff", "#c7d2fe", "#ddd6fe",
  "#fecdd3", "#fbcfe8", "#f5d0fe", "#e9d5ff", "#fed7aa", "#fde68a", "#d9f99d", "#bbf7d0",
];

function hslToHex(hue: number, saturation: number, lightness: number) {
  hue = ((hue % 360) + 360) % 360;
  const s = saturation / 100;
  const l = lightness / 100;
  const chroma = (1 - Math.abs(2 * l - 1)) * s;
  const x = chroma * (1 - Math.abs(((hue / 60) % 2) - 1));
  const m = l - chroma / 2;
  const [r, g, b] = hue < 60 ? [chroma, x, 0] : hue < 120 ? [x, chroma, 0] : hue < 180 ? [0, chroma, x] : hue < 240 ? [0, x, chroma] : hue < 300 ? [x, 0, chroma] : [chroma, 0, x];
  return `#${[r, g, b].map((value) => Math.round((value + m) * 255).toString(16).padStart(2, "0")).join("")}`;
}

function createHeadingColors(size: 32 | 64 | 128) {
  if (size === 32) return baseHeadingColors;
  const colors = [...baseHeadingColors];
  const bands = size === 64 ? [{ lightness: 68, saturation: 72 }] : [
    { lightness: 76, saturation: 76 },
    { lightness: 60, saturation: 72 },
    { lightness: 44, saturation: 68 },
  ];
  for (const [bandIndex, band] of bands.entries()) {
    for (let index = 0; index < 32; index += 1) {
      colors.push(hslToHex(index * 11.25 + bandIndex * 3.75, band.saturation, band.lightness));
    }
  }
  return colors.slice(0, size);
}

type FavoriteTextStyle = { color: string; bold: boolean };
let favoriteTextStylesRequest: Promise<Record<string, FavoriteTextStyle>> | undefined;
function loadFavoriteTextStyles() {
  return favoriteTextStylesRequest ??= invoke<Record<string, FavoriteTextStyle>>("get_favorite_text_styles").catch(error => { favoriteTextStylesRequest = undefined; throw error; });
}

function FavoriteCard({ item, displayLabel, iconSrc, headingColors, editing, onDragStart, onPointerMove, onPointerDrop, onEdit, onSave, onDelete, onOpen, onDropBefore, onColor, draggingId, dropBeforeId, selectionMode, selected, onToggleSelect }: {
  item: FavoriteItem; displayLabel: string; editing: boolean; onDragStart: (id: number) => void; onEdit: (id: number) => void;
  iconSrc?: string;
  headingColors: string[];
  onPointerMove: (id: number, clientX: number, clientY: number) => void;
  onPointerDrop: (id: number, clientX: number, clientY: number) => void;
  onSave: (item: FavoriteItem, label: string) => Promise<void>; onDelete: (id: number) => Promise<void>;
  onOpen: (item: FavoriteItem) => void; onDropBefore: (dragId: number, beforeId: number) => void; onColor: (id: number, color: string) => Promise<void>; draggingId: number | null; dropBeforeId?: number; selectionMode: boolean; selected: boolean; onToggleSelect: (id: number) => void;
}) {
  const [menu, setMenu] = useState<{x:number;y:number}|null>(null);
  const [textStyle, setTextStyle] = useState<FavoriteTextStyle>({color:"",bold:false});
  const menuRef = useRef<HTMLDivElement>(null);
  useEffect(() => { let active=true; void loadFavoriteTextStyles().then(styles=>{if(active)setTextStyle(styles[item.id]??{color:"",bold:false});}).catch(console.error); return()=>{active=false;}; },[item.id]);
  useEffect(() => {
    if(!menu)return;
    const close=(event:PointerEvent)=>{if(!menuRef.current?.contains(event.target as Node))setMenu(null);};
    const key=(event:KeyboardEvent)=>{if(event.key==='Escape'){event.stopImmediatePropagation();setMenu(null);}};
    document.addEventListener('pointerdown',close);document.addEventListener('keydown',key,true);
    return()=>{document.removeEventListener('pointerdown',close);document.removeEventListener('keydown',key,true);};
  },[menu]);
  const saveStyle=async(next:FavoriteTextStyle)=>{
    try { await invoke('set_favorite_text_style',{id:item.id,...next});const styles=await loadFavoriteTextStyles();styles[item.id]=next;setTextStyle(next); }
    catch(error){console.error(error);window.alert('文字の設定を保存できませんでした。');}
  };
  const rgb = item.color?.match(/[a-f\d]{2}/gi)?.map((part) => parseInt(part, 16));
  const luminance = rgb ? rgb.map((value) => { const channel = value / 255; return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4; }) : null;
  const headingStyle = item.kind === "heading" ? { backgroundColor: item.color, color: luminance && luminance[0] * 0.2126 + luminance[1] * 0.7152 + luminance[2] * 0.0722 > 0.179 ? "#000" : "#fff" } : undefined;
  const colorDetailsRef = useRef<HTMLDetailsElement>(null);
  useEffect(() => {
    const closeOnOutsideClick = (event: PointerEvent) => { if (colorDetailsRef.current?.open && !colorDetailsRef.current.contains(event.target as Node)) colorDetailsRef.current.open = false; };
    document.addEventListener("pointerdown", closeOnOutsideClick);
    return () => document.removeEventListener("pointerdown", closeOnOutsideClick);
  }, []);
  return <div data-favorite-id={item.id} className={`favorite-item ${item.kind}${draggingId === item.id ? " pointer-dragging" : ""}${dropBeforeId === item.id ? " drop-before" : ""}${selected ? " selected-favorite" : ""}`} style={{...headingStyle, '--favorite-text-color':textStyle.color||'inherit','--favorite-text-weight':textStyle.bold?700:400} as React.CSSProperties} onContextMenu={event=>{event.preventDefault();event.stopPropagation();setMenu({x:Math.max(0,Math.min(event.clientX,window.innerWidth-170)),y:Math.max(0,Math.min(event.clientY,window.innerHeight-190))});}} onDragOver={(event) => { event.preventDefault(); event.dataTransfer.dropEffect = "move"; }} onDrop={(event) => { event.preventDefault(); event.stopPropagation(); const dragId = Number(event.dataTransfer.getData("text/plain")) || draggingId; if (dragId) onDropBefore(dragId, item.id); }}>
    {menu && createPortal(<div ref={menuRef} className="favorite-context-menu" style={{left:menu.x,top:menu.y}} onClick={event=>event.stopPropagation()} onContextMenu={event=>event.preventDefault()}><button onClick={()=>{setMenu(null);onEdit(item.id);}}>名前を変更</button><label>文字色<input aria-label="お気に入りの文字色" type="color" value={textStyle.color||'#334155'} onChange={event=>void saveStyle({...textStyle,color:event.target.value})}/></label><button onClick={()=>void saveStyle({...textStyle,bold:!textStyle.bold})}>{textStyle.bold?'✓ ':''}太字</button><button onClick={()=>void saveStyle({color:'',bold:false})}>文字装飾をリセット</button><button className="delete-column-menu" onClick={()=>{setMenu(null);void onDelete(item.id);}}>削除</button></div>,document.body)}
    <span className="drag-handle" onPointerDown={(event) => { if (event.button !== 0) return; event.preventDefault(); event.currentTarget.setPointerCapture(event.pointerId); onDragStart(item.id); }} onPointerMove={(event) => { if (event.currentTarget.hasPointerCapture(event.pointerId)) onPointerMove(item.id, event.clientX, event.clientY); }} onPointerUp={(event) => { if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId); onPointerDrop(item.id, event.clientX, event.clientY); }} title="ここをドラッグして移動">⋮⋮</span>
    {selectionMode ? <button type="button" className={`favorite-select-box${selected ? " checked" : ""}`} onClick={() => onToggleSelect(item.id)}>{selected ? "✓" : ""}</button> : null}
    {item.kind === "link" ? <SourceIcon src={iconSrc} service={item.service ?? "edge"} /> : null}
    {editing ? <input className="favorite-inline-input" defaultValue={item.label} autoFocus onClick={(event)=>event.stopPropagation()} onFocus={(event) => event.currentTarget.select()} onBlur={(event) => void onSave(item, event.currentTarget.value)} onKeyDown={(event) => { event.stopPropagation(); if (event.key === "Enter") { event.preventDefault(); event.currentTarget.blur(); } }} /> :
      <button className="favorite-content" onClick={() => selectionMode ? onToggleSelect(item.id) : item.kind === "link" && item.target ? onOpen(item) : onEdit(item.id)} onDoubleClick={() => { if (!selectionMode) onEdit(item.id); }} title={selectionMode ? "選択する" : item.kind === "link" ? `${displayLabel}\n${item.target ?? ""}\n使用 ${item.openedCount ?? 0}回 / 最終 ${item.lastOpenedAt ?? "未使用"}` : "クリックで見出し名を変更"}><span className="fav-title">{displayLabel}</span></button>}
    {item.kind === "heading" ? <details ref={colorDetailsRef} className="color-picker"><summary title="見出し色を変更" style={{ backgroundColor: item.color }} /><div className="color-palette">{headingColors.map((color) => <button key={color} type="button" style={{ backgroundColor: color }} onClick={(event) => { event.preventDefault(); void onColor(item.id, color); }} title={color} />)}</div></details> : null}
    <button className="favorite-edit" onClick={() => onEdit(item.id)} title="名前を変更">✎</button>
    <button className="favorite-delete" onClick={() => void onDelete(item.id)} title="お気に入りから削除">×</button>
  </div>;
}

function parseLegacyLocalStorageSettings(): Settings | null {
  try {
    const raw = localStorage.getItem("quickSettings");
    if (!raw) return null;

    const parsed = JSON.parse(raw) as Partial<Settings>;
    return {
      displayCount: typeof parsed.displayCount === "number" ? parsed.displayCount : 6,
      quickKeywords:
        parsed.quickKeywords && parsed.quickKeywords.length
          ? parsed.quickKeywords
          : defaultQuickKeywords,
      blockedWords: parsed.blockedWords ?? [],
      siteNames: parsed.siteNames ?? [],
      searchMode: parsed.searchMode === "folder" ? "folder" : "web",
      windowSize: parsed.windowSize ?? "1100x760",
      indexFolders: parsed.indexFolders ?? [],
      favoritePaneCount: parsed.favoritePaneCount ?? 1,
      leftPanePercent: parsed.leftPanePercent ?? 50,
      favoriteDensity: parsed.favoriteDensity ?? "standard",
      favoriteTabLimit: parsed.favoriteTabLimit ?? 5,
      headingDefaultColor: parsed.headingDefaultColor ?? "#dbeafe",
      headingPaletteSize: parsed.headingPaletteSize === 64 || parsed.headingPaletteSize === 128 ? parsed.headingPaletteSize : 32,
      edgeSyncIntervalSeconds: parsed.edgeSyncIntervalSeconds ?? 10,
      folderSyncIntervalSeconds: parsed.folderSyncIntervalSeconds ?? 1800,
    };
  } catch (error) {
    console.error("Failed to read legacy localStorage settings", error);
    return null;
  }
}

function sanitizeSettingsForSave(settings: Settings, quickKeywordsText: string, blockedWordsText: string): Settings {
  const quickKeywords = quickKeywordsText
    .split("\n")
    .map((value) => value.trim())
    .filter(Boolean)
    .slice(0, 50);
  const blockedWords = blockedWordsText
    .split("\n")
    .map((value) => value.trim())
    .filter(Boolean)
    .slice(0, 200);

  return {
    displayCount: Math.max(0, Math.min(20, settings.displayCount)),
    quickKeywords: quickKeywords.length ? quickKeywords : defaultQuickKeywords,
    blockedWords,
    siteNames: [],
    searchMode: settings.searchMode,
    windowSize: settings.windowSize,
    indexFolders: settings.indexFolders.map((v) => v.trim()).filter(Boolean).slice(0, 10),
    favoritePaneCount: Math.max(1, Math.min(10, settings.favoritePaneCount)),
leftPanePercent: Math.max(15, Math.min(75, settings.leftPanePercent)),
    favoriteDensity: settings.favoriteDensity,
    favoriteTabLimit: Math.max(1,Math.min(20,settings.favoriteTabLimit)),
    headingDefaultColor: /^#[0-9a-f]{6}$/i.test(settings.headingDefaultColor) ? settings.headingDefaultColor : "#dbeafe",
    headingPaletteSize: settings.headingPaletteSize === 64 || settings.headingPaletteSize === 128 ? settings.headingPaletteSize : 32,
    edgeSyncIntervalSeconds: Math.max(10, Math.min(3600, settings.edgeSyncIntervalSeconds)),
    folderSyncIntervalSeconds: Math.max(15, Math.min(86400, settings.folderSyncIntervalSeconds)),
  };
}

function App() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [_selectedResult, setSelectedResult] = useState<SearchResult | null>(null);
  const [_actionMessage, setActionMessage] = useState("");

  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settings, setSettings] = useState<Settings>(defaultSettings);
  const [persistedSettings, setPersistedSettings] = useState<Settings>(defaultSettings);
  const [quickKeywordsText, setQuickKeywordsText] = useState(defaultSettings.quickKeywords.join("\n"));
  const [blockedWordsText, setBlockedWordsText] = useState("");
  const [settingsSaving, setSettingsSaving] = useState(false);

  const [favoriteItems, setFavoriteItems] = useState<FavoriteItem[]>([]);
  const [favoriteTabs, setFavoriteTabs] = useState<FavoriteTab[]>([]);
  const [favoriteColumns, setFavoriteColumns] = useState<FavoriteColumn[]>([]);
  const [activeTabId, setActiveTabId] = useState<number>(1);
  const [contextMenu, setContextMenu] = useState<{type:"tab"|"column";id:number;x:number;y:number}|null>(null);
  const [trashOpen, setTrashOpen] = useState(false);
  const [deletedFavorites, setDeletedFavorites] = useState<FavoriteItem[]>([]);
  const [toastMessage, setToastMessage] = useState("");
  const [manualFavoriteOpen,setManualFavoriteOpen]=useState(false);
  const [manualFavoriteName,setManualFavoriteName]=useState("");
  const [manualFavoriteTarget,setManualFavoriteTarget]=useState("");
  const [organizing,setOrganizing]=useState(false);
  const [uiPreferences,setUiPreferences]=useState(defaultUiPreferences);
  const [persistedUiPreferences,setPersistedUiPreferences]=useState(defaultUiPreferences);
  const navigationPendingRef=useRef(false);
  const tabResetRef=useRef({enabled:false,firstTab:1});
  tabResetRef.current={enabled:persistedUiPreferences.resetTabAfterLink,firstTab:favoriteTabs[0]?.id??1};
  const [fileResults, setFileResults] = useState<FileSearchResult[]>([]);
  const [webIcons,setWebIcons]=useState<Record<string,string>>({});
  const [fileIcons,setFileIcons]=useState<Record<string,string>>({});
  const [indexFoldersText, setIndexFoldersText] = useState("");
  const [indexMessage, setIndexMessage] = useState("");
  const [editingFavoriteId, setEditingFavoriteId] = useState<number | null>(null);
  const [draggingFavoriteId, setDraggingFavoriteId] = useState<number | null>(null);
  const [favoriteDragPreview, setFavoriteDragPreview] = useState<{ x: number; y: number; pane: number; beforeId?: number; lineX?:number; lineY?:number; lineWidth?:number } | null>(null);
  const [draggingColumnId,setDraggingColumnId]=useState<number|null>(null);
  const draggingColumnRef=useRef<number|null>(null);
  const [draggingTabId,setDraggingTabId]=useState<number|null>(null);
  const [tabDropBeforeId,setTabDropBeforeId]=useState<number|null>(null);
  const tabDropBeforeRef=useRef<number|null>(null);
  const tabPointerDragRef=useRef<{id:number;startX:number;startY:number;dragging:boolean}|null>(null);
  const suppressTabClickRef=useRef(false);
  const [columnDropTarget,setColumnDropTarget]=useState<{type:"tab"|"column";id:number}|null>(null);
  const appContainerRef = useRef<HTMLDivElement>(null);
  const resizeSaveTimerRef = useRef<number | null>(null);
  const programmaticResizeRef = useRef(false);
  const [currentWindowSize, setCurrentWindowSize] = useState(defaultSettings.windowSize);
  const [appLogs, setAppLogs] = useState<AppLogEntry[]>([]);
  const deleteTimerRef = useRef<number | null>(null);
  const [deletedFavorite, setDeletedFavorite] = useState<FavoriteItem | null>(null);
  const [fileIndexReady, setFileIndexReady] = useState(false);
  const [fileFilter, setFileFilter] = useState('documents');
  const requestedWebIconsRef = useRef(new Set<string>());
  const iconQueueRef = useRef(Promise.resolve());
  const [extensionSyncMessage,setExtensionSyncMessage]=useState("10秒ごとに受信確認＋新規訪問時に自動送信");
  const [extensionConnection,setExtensionConnection]=useState<ExtensionConnectionStatus>({connected:false});

  const [edgeImportStatus, setEdgeImportStatus] = useState<EdgeImportStatus>({
    running: false,
    message: "まだEdge履歴を更新していません。",
    lastOutcome: null,
    lastUpdatedAt: null,
    revision: 0,
    debugLog: [],
  });

  const statusRevisionRef = useRef(0);
  const isEscapeKey = (event: KeyboardEvent) =>
    event.key === "Escape" ||
    event.key === "Esc" ||
    event.code === "Escape" ||
    event.keyCode === 27;

  const hideToTray = () => {
    void invoke("hide_main_window").catch((error) => console.error("hide_main_window", error));
  };

  const syncSettingsBuffers = (nextSettings: Settings) => {
    setQuickKeywordsText(nextSettings.quickKeywords.join("\n"));
    setBlockedWordsText(nextSettings.blockedWords.join("\n"));
    setIndexFoldersText(nextSettings.indexFolders.join("\n"));
  };

  const loadSearchResults = async () => {
    const history = await invoke<SearchResult[]>("fetch_history");
    setResults(history);
  };

  const loadFavorites = async () => {
    const items=await invoke<FavoriteItem[]>("list_favorite_items");
    setFavoriteItems(items);
    const targets=new Set(items.filter(item=>item.kind==='link').map(item=>item.target?.toLowerCase()));
    setResults(current=>current.map(item=>({...item,isFavorite:targets.has(item.url.toLowerCase())?1:0})));
    setFileResults(current=>current.map(item=>({...item,isFavorite:targets.has(item.path.toLowerCase())})));
  };
  const loadFavoriteLayout = async () => {
    const [tabs, columns] = await Promise.all([invoke<FavoriteTab[]>("list_favorite_tabs"), invoke<FavoriteColumn[]>("list_favorite_columns")]);
    setFavoriteTabs(tabs); setFavoriteColumns(columns); setActiveTabId((current) => tabs.some((tab) => tab.id === current) ? current : (tabs[0]?.id ?? 1));
  };
  const showToast = (message:string) => { setToastMessage(message); window.setTimeout(() => setToastMessage(""), 3500); };

  const loadEdgeImportStatus = async (reloadOnRevisionChange = false) => {
    const status = await invoke<EdgeImportStatus>("get_edge_import_status");
    setEdgeImportStatus(status);

    if (reloadOnRevisionChange && status.revision !== statusRevisionRef.current) {
      const previousRevision = statusRevisionRef.current;
      statusRevisionRef.current = status.revision;

      if (previousRevision !== 0 && status.lastOutcome === "success") {
        setQuery("");
        await Promise.all([loadSearchResults(), loadFavorites()]);
      }
    } else {
      statusRevisionRef.current = status.revision;
    }
  };

  useEffect(() => {
    const initialize = async () => {
      try {
        await invoke('prepare_favorite_board');
        const ui = await invoke<UiPreferences>('get_ui_preferences');
        setUiPreferences(ui); setPersistedUiPreferences(ui);
        setFileFilter(await invoke<string>('get_file_filter'));
        const response = await invoke<SettingsResponse>("get_settings");
        let nextSettings = response.settings;

        if (response.isDefault) {
          const legacy = parseLegacyLocalStorageSettings();
          if (legacy) {
            nextSettings = await invoke<Settings>("save_settings", { settings: legacy });
            localStorage.removeItem("quickSettings");
          }
        }

        setSettings(nextSettings);
        setPersistedSettings(nextSettings);
        syncSettingsBuffers(nextSettings);
        programmaticResizeRef.current = true;
        await invoke("apply_initial_window_size", { size: nextSettings.windowSize });
        setCurrentWindowSize(nextSettings.windowSize);
        window.setTimeout(() => { programmaticResizeRef.current = false; }, 800);
      } catch (error) {
        console.error("Failed to load settings", error);
      }

      try {
        await Promise.all([loadSearchResults(), loadFavorites(), loadFavoriteLayout(), loadEdgeImportStatus(), loadAppLogs()]);
        setFileIndexReady(await invoke<boolean>("is_file_index_initialized"));
      } catch (error) {
        console.error("Failed to initialize app data", error);
      }
    };

    void initialize();
  }, []);

  useEffect(() => {
    const handler = window.setTimeout(() => {
      if (settings.searchMode === "web") {
        invoke<SearchResult[]>("search_history", { query }).then(setResults).catch((error) => console.error("search_history error", error));
      } else {
        invoke<FileSearchResult[]>("search_file_index", { query, filter: fileFilter }).then(setFileResults).catch((error) => console.error("search_file_index error", error));
      }
    }, 200);

    return () => window.clearTimeout(handler);
  }, [query, settings.searchMode, fileFilter]);

  useEffect(()=>{
    const urls=[...new Set([...favoriteItems.filter((f)=>f.service!=="folder").map((f)=>f.target??""),...results.map((r)=>r.url)])].filter(Boolean).slice(0,300);
    const pending = urls.filter(url => !requestedWebIconsRef.current.has(url));
    pending.forEach(url => requestedWebIconsRef.current.add(url));
    iconQueueRef.current = iconQueueRef.current.catch(() => {}).then(async () => {
      for (let offset = 0; offset < pending.length; offset += 4) {
        await Promise.all(pending.slice(offset, offset + 4).map(async url => {
          try { const icon = await invoke<string|null>('get_edge_favicon', { url }); if (icon) setWebIcons(value => ({ ...value, [url]: icon })); }
          catch { /* Use the site's favicon fallback without retrying every render. */ }
        }));
      }
    });
  },[results,favoriteItems]);
  useEffect(()=>{
    const paths=[...new Set([...fileResults.map(file=>file.path),...favoriteItems.filter(item=>item.service==='folder'&&item.target).map(item=>item.target!)])];
    let active=true;
    iconQueueRef.current=iconQueueRef.current.catch(()=>{}).then(async()=>{
      for(const path of paths){if(!active)break;if(fileIcons[path])continue;try{const icon=await invoke<string|null>('get_file_icon',{path});if(active&&icon)setFileIcons(value=>({...value,[path]:icon}));}catch(error){console.error('get_file_icon',error);}}
    });
    return()=>{active=false;};
  },[fileResults,favoriteItems]);

  useEffect(() => {
    const intervalId = window.setInterval(() => {
      void loadEdgeImportStatus(true).catch((error) => {
        console.error("get_edge_import_status error", error);
      });
      if (settingsOpen) void loadAppLogs().catch((error) => console.error("get_app_logs error", error));
    }, 1000);

    return () => window.clearInterval(intervalId);
  }, [settingsOpen]);

  useEffect(()=>{let busy=false;const sync=async()=>{if(busy)return;busy=true;try{const count=await invoke<number>("import_extension_history");if(count>0){setExtensionSyncMessage(`拡張機能から${count}件取り込みました`);setResults(await invoke<SearchResult[]>("search_history",{query}));await loadFavorites();}}catch(error){console.error("extension history sync",error);}finally{busy=false;}};void sync();const id=window.setInterval(()=>void sync(),1000);return()=>window.clearInterval(id);},[query]);
  useEffect(()=>{const load=()=>void invoke<ExtensionConnectionStatus>("extension_connection_status").then(setExtensionConnection).catch(()=>{});load();const id=window.setInterval(load,10000);return()=>window.clearInterval(id);},[]);

  useEffect(() => {
    if (!fileIndexReady || settings.indexFolders.length === 0) return;
    const id = window.setInterval(() => { void invoke("rebuild_file_index", { folders: settings.indexFolders }); }, Math.max(15, settings.folderSyncIntervalSeconds) * 1000);
    return () => window.clearInterval(id);
  }, [fileIndexReady, settings.indexFolders, settings.folderSyncIntervalSeconds]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void (async () => {
      const appWindow = getCurrentWindow();
      const scale = await appWindow.scaleFactor();
      unlisten = await appWindow.onResized(({ payload }) => {
        if (disposed) return;
        const size = `${Math.round(payload.width / scale)}x${Math.round(payload.height / scale)}`;
        setCurrentWindowSize(size);
        if (programmaticResizeRef.current) return;
        if (resizeSaveTimerRef.current) window.clearTimeout(resizeSaveTimerRef.current);
        resizeSaveTimerRef.current = window.setTimeout(() => {
          void invoke("save_window_size", { size });
          setSettings((current) => ({ ...current, windowSize: size }));
          setPersistedSettings((current) => ({ ...current, windowSize: size }));
        }, 500);
      });
    })();
    return () => { disposed = true; unlisten?.(); if (resizeSaveTimerRef.current) window.clearTimeout(resizeSaveTimerRef.current); };
  }, []);

  const loadAppLogs = async () => setAppLogs(await invoke<AppLogEntry[]>("get_app_logs"));
  const filteredResults = useMemo(() => results, [results]);
  const headingColors = useMemo(() => createHeadingColors(settings.headingPaletteSize), [settings.headingPaletteSize]);
  const favoriteDisplayLabel = (item: FavoriteItem) => item.label;
  const favoriteIcon = (item:FavoriteItem) => { if(!item.target)return undefined;if(item.service!=="folder")return webIcons[item.target] ?? websiteFaviconUrl(item.target);return fileIcons[item.target]; };
  const activeResultCount = settings.searchMode === "web" ? filteredResults.length : fileResults.length;

  useEffect(() => {
    if (selectedIndex >= activeResultCount) {
      setSelectedIndex(Math.max(0, activeResultCount - 1));
    }
  }, [activeResultCount, selectedIndex]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      const active = document.activeElement as HTMLElement | null;
      if (active && active.id !== 'search-input' && (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA' || active.tagName === 'SELECT' || active.isContentEditable)) return;

      if (isEscapeKey(event)) {
        event.preventDefault();
        event.stopPropagation();
        hideToTray();
        return;
      }

      if (event.key === "Enter" && active && active.id !== "search-input" && (active.tagName === "INPUT" || active.tagName === "SELECT" || active.isContentEditable)) {
        return;
      }

      if (settingsOpen) {
        if (event.key === "Enter") {
          if (active && active.tagName === "TEXTAREA") {
            return;
          }
          event.preventDefault();
          return;
        }
      }

      if (event.key === "ArrowDown") {
        if (active && active.tagName === "TEXTAREA") return;
        event.preventDefault();
        setSelectedIndex((current) => Math.min(current + 1, activeResultCount - 1));
        return;
      }

      if (event.key === "ArrowUp") {
        if (active && active.tagName === "TEXTAREA") return;
        event.preventDefault();
        setSelectedIndex((current) => Math.max(current - 1, 0));
        return;
      }

      if (event.key === "Enter") {
        if (active && active.id && active.id !== "search-input" && active !== document.body) {
          return;
        }

        event.preventDefault();
        if (settings.searchMode === "folder") {
          const file = fileResults[selectedIndex];
          if (file) void openLocalPathAndHide(file.path);
          return;
        }
        const item = filteredResults[selectedIndex];
        if (item) {
          setSelectedResult(item);
          setActionMessage(`「${item.title}」を選択しました。`);
openWebLink(item.url)
            .then(() => {
              try {
                void getCurrentWindow().hide();
              } catch {
                // noop
              }
            })
            .catch((error: unknown) => {
              console.error("open_url error", error);
              setActionMessage(`外部ブラウザで開けませんでした: ${String(error)}`);
            });
        }
        return;
      }

    };

    const handleKeyUp = (event: KeyboardEvent) => {
      const active = document.activeElement as HTMLElement | null;
      if (active && active.id !== 'search-input' && (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA' || active.tagName === 'SELECT')) return;
      if (isEscapeKey(event)) {
        event.preventDefault();
        event.stopPropagation();
        hideToTray();
      }
    };

    window.addEventListener("keydown", handleKeyDown, true);
    document.addEventListener("keydown", handleKeyDown, true);
    window.addEventListener("keyup", handleKeyUp, true);
    document.addEventListener("keyup", handleKeyUp, true);
    return () => {
      window.removeEventListener("keydown", handleKeyDown, true);
      document.removeEventListener("keydown", handleKeyDown, true);
      window.removeEventListener("keyup", handleKeyUp, true);
      document.removeEventListener("keyup", handleKeyUp, true);
    };
  }, [filteredResults, fileResults, activeResultCount, selectedIndex, settings, settingsOpen]);

  const appendPhrase = (phrase: string) => {
    setQuery((current) => (current ? `${current} ${phrase}` : phrase));
    setActionMessage(`「${phrase}」を検索語に追加しました。`);
  };

  const openSettings = () => {
    setUiPreferences(persistedUiPreferences);
    setSettings(persistedSettings);
    syncSettingsBuffers(persistedSettings);
    setSettingsOpen(true);
  };

  const saveSettings = async () => {
    const nextSettings = sanitizeSettingsForSave(
      settings,
      quickKeywordsText,
      blockedWordsText,
    );
    nextSettings.indexFolders = indexFoldersText.split("\n").map((v) => v.trim()).filter(Boolean).slice(0, 10);

    setSettingsSaving(true);
    try {
      const saved = await invoke<Settings>("save_settings", { settings: nextSettings });
      await invoke('save_ui_preferences',{preferences:uiPreferences});
      setPersistedUiPreferences(uiPreferences);
      setSettings(saved);
      setPersistedSettings(saved);
      syncSettingsBuffers(saved);
      programmaticResizeRef.current = true;
      await invoke("apply_window_size", { size: saved.windowSize });
      setCurrentWindowSize(saved.windowSize);
      window.setTimeout(() => { programmaticResizeRef.current = false; }, 800);
      await loadFavorites();
      setSettingsOpen(false);
      localStorage.removeItem("quickSettings");
    } catch (error) {
      console.error("Failed to save settings", error);
    } finally {
      setSettingsSaving(false);
    }
  };

  const cancelSettings = () => {
    setUiPreferences(persistedUiPreferences);
    setSettings(persistedSettings);
    syncSettingsBuffers(persistedSettings);
    setSettingsOpen(false);
  };

  const toggleFavorite = (result: SearchResult) => {
    invoke<boolean>("toggle_favorite", { id: result.id })
      .then(async (active) => {
        setResults((previous) =>
          previous.map((item) =>
            item.id === result.id
              ? { ...item, isFavorite: active ? 1 : 0 }
              : item,
          ),
        );
        if (active) await routeNewFavorite(result.url);
        await loadFavorites();
        showToast(active ? 'お気に入りに保存しました。「整理」から移動先を変更できます。' : 'お気に入りから外しました');
      })
      .catch((error: unknown) => {
        console.error("toggle_favorite error", error);
        showToast(`登録できませんでした: ${error}`);
      });
  };
  const routeNewFavorite = async (_target: string) => { /* New links remain in the search-side inbox. */ };
  const openWebLink = (url: string) => {
    navigationPendingRef.current=true;
    return invoke('open_url',{url}).catch(error=>{navigationPendingRef.current=false;throw error;});
  };
  useEffect(()=>{
    let disposed=false;
    let unlisten: (()=>void)|undefined;
    void getCurrentWindow().onFocusChanged(event=>{
      if(event.payload && navigationPendingRef.current){
        navigationPendingRef.current=false;
        if(tabResetRef.current.enabled) setActiveTabId(tabResetRef.current.firstTab);
      }
    }).then(stop=>{if(disposed)stop();else unlisten=stop;});
    return ()=>{disposed=true;unlisten?.();};
  },[]);
  const openLocalPathAndHide = async (path: string) => {
    await invoke("open_local_path", { path });
    navigationPendingRef.current=true;
    try { await new Promise((resolve) => window.setTimeout(resolve, 100)); await getCurrentWindow().hide(); } catch { /* path is already open */ }
  };

  const saveFavoriteLabel = async (item: FavoriteItem, label: string) => {
    await invoke("rename_favorite_item", { id: item.id, label }); setEditingFavoriteId(null); await loadFavorites();
  };
  const removeFavorite = async (id: number) => {
    const item = favoriteItems.find((value) => value.id === id); if (!item) return;
    if (deleteTimerRef.current) window.clearTimeout(deleteTimerRef.current);
    await invoke("delete_favorite_item", { id: item.id });
    setFavoriteItems((items) => items.filter((value) => value.id !== id)); setDeletedFavorite(item);
    void loadSearchResults();
    if (settings.searchMode === "folder") void invoke<FileSearchResult[]>("search_file_index", { query }).then(setFileResults);
    deleteTimerRef.current = window.setTimeout(() => { setDeletedFavorite(null); deleteTimerRef.current = null; }, 10000);
  };
  const undoFavoriteDelete = async () => { if (deleteTimerRef.current) window.clearTimeout(deleteTimerRef.current); deleteTimerRef.current = null; if (deletedFavorite) { await invoke("restore_deleted_favorite", { id: deletedFavorite.id }); await loadFavorites(); void loadSearchResults(); } setDeletedFavorite(null); };
  const moveFavorite = async (id: number, toPane: number, beforeId?: number) => {
    const moving = favoriteItems.find((item) => item.id === id); if (!moving) return;
    const without = favoriteItems.filter((item) => item.id !== id);
    const targetItems = without.filter((item) => item.pane === toPane);
    const targetIndex = beforeId ? targetItems.findIndex((item) => item.id === beforeId) : -1;
    const insertAt = targetIndex < 0 ? targetItems.length : targetIndex;
    targetItems.splice(insertAt, 0, { ...moving, pane: toPane });
    const next = without.filter((item) => item.pane !== toPane).concat(targetItems);
    setFavoriteItems(next); setDraggingFavoriteId(null);
    try { await invoke("move_board_item", { id, pane: toPane, beforeId: beforeId ?? null }); }
    catch (error) { await loadFavorites(); showToast(`移動できませんでした: ${error}`); throw error; }
  };
  const favoriteDropTargetAtPoint = (clientX: number, clientY: number) => {
    const target = document.elementFromPoint(clientX, clientY) as HTMLElement | null;
    let paneElement = target?.closest<HTMLElement>("[data-favorite-pane]");
    if(!paneElement){const column=target?.closest<HTMLElement>('[data-board-column]');const groups=column?.querySelectorAll<HTMLElement>('[data-favorite-pane]');paneElement=groups?.length?groups[groups.length-1]:undefined;}
    if(!paneElement)return {pane:NaN};
    const pane=Number(paneElement.dataset.favoritePane);
    const cards=Array.from(paneElement.querySelectorAll<HTMLElement>('[data-favorite-id]')).filter(card=>Number(card.dataset.favoriteId)!==draggingFavoriteId);
    const before=cards.find(card=>{const rect=card.getBoundingClientRect();return clientY<rect.top+rect.height/2;});
    const list=paneElement.querySelector<HTMLElement>('.favorites-list')??paneElement;
    const rect=list.getBoundingClientRect();
    const last=cards[cards.length-1]?.getBoundingClientRect();
    const lineY=before?before.getBoundingClientRect().top:(last?.bottom??rect.bottom-3);
    return {pane,beforeId:before?Number(before.dataset.favoriteId):undefined,lineX:rect.left+3,lineWidth:Math.max(8,rect.width-6),lineY:Math.max(4,Math.min(window.innerHeight-6,lineY))};
  };
  const trackFavoriteDrag = (_id: number, clientX: number, clientY: number) => {
    const target = favoriteDropTargetAtPoint(clientX, clientY);
    setFavoriteDragPreview({ x: clientX, y: clientY, ...target });
  };
  const dropFavoriteAtPoint = (id: number, clientX: number, clientY: number) => {
    const target = favoriteDropTargetAtPoint(clientX, clientY);
    setFavoriteDragPreview(null);
    if (!Number.isFinite(target.pane)) { setDraggingFavoriteId(null); return; }
    void moveFavorite(id, target.pane, target.beforeId && target.beforeId !== id ? target.beforeId : undefined);
  };
  const changeHeadingColor = async (id: number, color: string) => { await invoke("set_favorite_color", { id, color }); setFavoriteItems((items) => items.map((item) => item.id === id ? { ...item, color } : item)); };
  const applyHeadingColorToAll = async () => { await invoke("set_all_heading_colors", { color: settings.headingDefaultColor }); setFavoriteItems((items) => items.map((item) => item.kind === "heading" ? { ...item, color: settings.headingDefaultColor } : item)); await loadFavoriteLayout(); showToast("すべてのグループを同じ色に変更しました"); };
  const beginPaneResize = (event: ReactMouseEvent) => {
    event.preventDefault(); const container = appContainerRef.current; if (!container) return;
    const move = (e: MouseEvent) => { const rect = container.getBoundingClientRect(); const percent = Math.max(15, Math.min(75, ((e.clientX - rect.left) / rect.width) * 100)); setSettings((current) => ({ ...current, leftPanePercent: Math.round(percent) })); };
    const up = async () => { window.removeEventListener("mousemove", move); window.removeEventListener("mouseup", up); setSettings((current) => { const next = { ...current }; setPersistedSettings(next); void invoke("save_settings", { settings: next }); return next; }); };
    window.addEventListener("mousemove", move); window.addEventListener("mouseup", up);
  };
  const setSearchMode = async (mode: "web" | "folder") => {
    const next = { ...settings, searchMode: mode }; setSettings(next); setPersistedSettings(next); setSelectedIndex(0);
    await invoke("save_settings", { settings: next });
  };
  const rebuildIndex = async () => {
    const folders = indexFoldersText.split("\n").map((v) => v.trim()).filter(Boolean).slice(0, 10); setIndexMessage("インデックス作成中…");
    try { const summary = await invoke<IndexUpdateSummary>("rebuild_file_index", { folders }); setIndexMessage(`確認${summary.scanned}件／追加${summary.added}・変更${summary.updated}・削除${summary.deleted}件`); setFileIndexReady(true); await loadAppLogs(); }
    catch (error) { setIndexMessage(`作成に失敗しました: ${String(error)}`); }
  };
  const clearAppLogs = async () => { await invoke("clear_app_logs"); setAppLogs([]); };
  const toggleFileFavorite = async (file: FileSearchResult) => {
    const active = await invoke<boolean>("toggle_file_favorite", { path: file.path, name: file.name });
    if (active) await routeNewFavorite(file.path);
    setFileResults((items) => items.map((item) => item.id === file.id ? { ...item, isFavorite: active } : item)); await loadFavorites();
  };
  const chooseFileFilter = (value: string) => {
    if (value === 'custom') {
      const extensions = prompt('表示する拡張子をカンマで区切って入力してください。例：dwg,msg,jpg', fileFilter.startsWith('custom:') ? fileFilter.slice(7) : 'dwg,msg');
      if (!extensions) return;
      const clean = extensions.split(',').map(ext => ext.trim().replace(/^\./,'').toLowerCase()).filter(ext => /^[a-z0-9]{1,12}$/.test(ext)).slice(0,20);
      if (!clean.length) { showToast('拡張子を入力してください'); return; }
      value = `custom:${clean.join(',')}`;
    }
    setFileFilter(value); void invoke('save_file_filter', { value }).catch(error => showToast(String(error)));
  };
  const openFavoriteTarget = (item: FavoriteItem) => {
    if (!item.target) return;
    void invoke("record_favorite_open", { id:item.id });
    setFavoriteItems((items) => items.map((value) => value.id === item.id ? {...value, openedCount:(value.openedCount ?? 0)+1, lastOpenedAt:new Date().toISOString()} : value));
    if (item.service === "folder") void openLocalPathAndHide(item.target); else { hideToTray(); void openWebLink(item.target).catch(error=>showToast(`開けませんでした: ${error}`)); }
  };
  const activeColumns = favoriteColumns.filter((column) => column.tabId === activeTabId);
  const addTab = async () => { try { const id=await invoke<number>("add_favorite_tab",{limit:settings.favoriteTabLimit}); await loadFavoriteLayout(); setActiveTabId(id); } catch(error){showToast(String(error));} };

  const favoriteLocation = (item:FavoriteItem) => { const column=favoriteColumns.find((value)=>value.id===item.pane);const tab=favoriteTabs.find((value)=>value.id===column?.tabId);return `${tab?.name??"不明なタブ"} の ${column?.name??"追加されたお気に入り"}`; };
  const submitManualFavorite = async()=>{
    const target=manualFavoriteTarget.trim();const duplicate=favoriteItems.find((item)=>item.target?.trim().toLowerCase()===target.toLowerCase());
    if(duplicate){showToast(`そのリンクはすでにお気に入り登録済みです。${favoriteLocation(duplicate)} にあります。`);return;}
    const pane=0;
    try{await invoke("add_manual_favorite",{label:manualFavoriteName,target,pane});await routeNewFavorite(target);await loadFavorites();setManualFavoriteOpen(false);setManualFavoriteName("");setManualFavoriteTarget("");showToast("検索下の「追加されたお気に入り」に保存しました。");}catch(error){showToast(String(error));}
  };
  const editTab = async (tab:FavoriteTab, color=tab.color) => { const name=window.prompt("タブ名を入力してください",tab.name); if(name===null)return; await invoke("update_favorite_tab",{id:tab.id,name,color}); await loadFavoriteLayout(); };
  const setTabColor = async(tab:FavoriteTab,color:string)=>{await invoke("update_favorite_tab",{id:tab.id,name:tab.name,color});await loadFavoriteLayout();setContextMenu(null);};
  const deleteTab = async(tab:FavoriteTab)=>{if(!window.confirm(`「${tab.name}」を削除しますか？\n中の列とお気に入りは隣のタブへ移動します。`))return;try{const targetId=await invoke<number>("delete_favorite_tab",{id:tab.id});setActiveTabId(targetId);setContextMenu(null);await loadFavoriteLayout();showToast("タブを削除し、中の列を隣のタブへ移動しました");}catch(error){showToast(String(error));}};
  const reorderTab = async(dragId:number,beforeId:number)=>{if(dragId===beforeId)return;const ordered=favoriteTabs.filter((tab)=>tab.id!==dragId);const index=ordered.findIndex((tab)=>tab.id===beforeId);const moving=favoriteTabs.find((tab)=>tab.id===dragId);if(!moving)return;ordered.splice(index<0?ordered.length:index,0,moving);setFavoriteTabs(ordered.map((tab,position)=>({...tab,position})));setDraggingTabId(null);setTabDropBeforeId(null);await invoke("place_favorite_tabs",{ids:ordered.map((tab)=>tab.id)});};
  const moveTabPointer=(event:ReactPointerEvent<HTMLButtonElement>)=>{const drag=tabPointerDragRef.current;if(!drag||!event.currentTarget.hasPointerCapture(event.pointerId))return;if(!drag.dragging&&Math.hypot(event.clientX-drag.startX,event.clientY-drag.startY)>4){drag.dragging=true;suppressTabClickRef.current=true;setDraggingTabId(drag.id);}if(drag.dragging){const element=document.elementFromPoint(event.clientX,event.clientY) as HTMLElement|null;const targetElement=element?.closest<HTMLElement>("[data-favorite-tab-id]");const targetId=Number(targetElement?.dataset.favoriteTabId);let beforeId:number|null=targetId&&targetId!==drag.id?targetId:null;if(targetElement&&targetId&&event.clientX>targetElement.getBoundingClientRect().left+targetElement.getBoundingClientRect().width/2){const visible=favoriteTabs.filter((tab)=>tab.id!==drag.id);const index=visible.findIndex((tab)=>tab.id===targetId);beforeId=visible[index+1]?.id??-1;}tabDropBeforeRef.current=beforeId;setTabDropBeforeId(beforeId);}};
  const endTabPointer=(event:ReactPointerEvent<HTMLButtonElement>)=>{const drag=tabPointerDragRef.current;const beforeId=tabDropBeforeRef.current;if(event.currentTarget.hasPointerCapture(event.pointerId))event.currentTarget.releasePointerCapture(event.pointerId);tabPointerDragRef.current=null;tabDropBeforeRef.current=null;if(drag?.dragging&&beforeId&&beforeId!==drag.id)void reorderTab(drag.id,beforeId);else{setDraggingTabId(null);setTabDropBeforeId(null);}};
  const moveColumnToTab = async(column:FavoriteColumn,tabId:number)=>{try{await invoke("move_favorite_column",{id:column.id,tabId,limit:settings.favoritePaneCount});await loadFavoriteLayout();setContextMenu(null);}catch(error){showToast(String(error));}};
  const deleteColumn = async(column:FavoriteColumn)=>{const index=activeColumns.findIndex((c)=>c.id===column.id);const target=activeColumns[index-1]?.id??activeColumns[index+1]?.id??0;await invoke("delete_favorite_column",{id:column.id,targetPane:target});setContextMenu(null);await Promise.all([loadFavoriteLayout(),loadFavorites()]);showToast(target?"列を削除し、中のお気に入りを隣の列へ移動しました。":"列を削除し、中のお気に入りを追加欄へ移動しました。");};
  const openTrash = async()=>{try{setDeletedFavorites(await invoke<FavoriteItem[]>("list_deleted_favorites"));setTrashOpen(true);}catch(error){console.error("list_deleted_favorites",error);showToast(`削除済み一覧を開けませんでした: ${String(error)}`);}};
  const restoreTrash = async(id:number)=>{await invoke("restore_deleted_favorite",{id});await Promise.all([loadFavorites(),loadSearchResults()]);setDeletedFavorites((items)=>items.filter((item)=>item.id!==id));};
  const purgeTrash = async(id:number)=>{await invoke("permanently_delete_favorite",{id});setDeletedFavorites((items)=>items.filter((item)=>item.id!==id));};

  const handleEdgeHistoryRefresh = async () => {
    if (edgeImportStatus.running) {
      return;
    }

    setEdgeImportStatus((current) => ({
      ...current,
      running: true,
      message: "Edge履歴を更新しています…",
    }));

    try {
      await invoke("refresh_edge_history");
      setQuery("");
      await Promise.all([loadSearchResults(), loadFavorites(), loadEdgeImportStatus(true)]);
    } catch (error) {
      console.error("refresh_edge_history error", error);
      try {
        await loadEdgeImportStatus(true);
      } catch (statusError) {
        console.error("Failed to reload Edge import status", statusError);
      }
    }
  };

  const handleEdgeHistoryClear = async () => {
    if (edgeImportStatus.running) {
      return;
    }

    try {
      await invoke("clear_edge_history");
      setQuery("");
      await Promise.all([loadSearchResults(), loadFavorites()]);
      setEdgeImportStatus((current) => ({
        ...current,
        message: "Edge履歴をクリアしました。",
        lastOutcome: "success",
        lastUpdatedAt: new Date().toLocaleString(),
        revision: current.revision + 1,
      }));
    } catch (error) {
      console.error("clear_edge_history error", error);
      setEdgeImportStatus((current) => ({
        ...current,
        message: `Edge履歴のクリアに失敗しました: ${String(error)}`,
        lastOutcome: "failure",
        lastUpdatedAt: new Date().toLocaleString(),
      }));
    }
  };

  return (
    <div className="app-shell">
      <div className="window-drag-region" data-tauri-drag-region />
      <div className="app-container" ref={appContainerRef}>
        <div className="left-pane" style={{ flexBasis: `${settings.leftPanePercent}%` }}>
          <div className="search-card">
            <div className="header-block">
              <div className="window-label">検索</div>
              <div className="top-right-controls"><button className="settings-button" onClick={openSettings}>設定</button></div>
            </div>
              <div className="search-mode-switch">
                <button className={settings.searchMode === "web" ? "active" : ""} onClick={() => void setSearchMode("web")}>Web</button>
                <button className={settings.searchMode === "folder" ? "active" : ""} onClick={() => void setSearchMode("folder")}>フォルダ</button>
              </div>

            <div className="search-panel">
              <input
                id="search-input"
                className="search-input"
                value={query}
                onChange={(event) => setQuery(event.currentTarget.value)}
                placeholder="検索ワードを入力..."
                autoFocus
              />

              {settings.displayCount > 0 ? <div className="machine-buttons" aria-label="機種ボタン">
                {settings.quickKeywords.slice(0, settings.displayCount).map((button, index) => (
                  <button
                    key={`${button}-${index}`}
                    type="button"
                    className="machine-button"
                    onClick={() => appendPhrase(button)}
                  >
                    {button}
                  </button>
                ))}
              </div> : null}


              {settingsOpen && (
                <div className="settings-modal-backdrop">
                  <div className="settings-modal">
                    <div className="settings-modal-header">
                      <h3>設定</h3>
                      <div className="settings-actions settings-header-actions">
                        <button className="settings-save-primary" onClick={() => void saveSettings()} disabled={settingsSaving}>{settingsSaving ? "保存中..." : "保存"}</button>
                        <button onClick={cancelSettings} disabled={settingsSaving || edgeImportStatus.running}>キャンセル</button>
                      </div>
                    </div>
                    <div className="settings-modal-layout settings-dashboard">
                      <div className="settings-pane settings-pane-left settings-stack">
                        <section className="settings-section">
                          <div className="settings-section-title"><div><strong>表示とレイアウト</strong><small>お気に入り画面の見え方を調整します</small></div></div>
                          <div className="display-settings-grid">
                            <label><span>タブ数上限 <SettingHelp text="お気に入りタブを何個まで作成できるかを指定します。1～20の範囲で設定できます。設定を現在のタブ数より小さくしても、既存のタブが勝手に削除されることはありません。" /></span><input type="number" min={1} max={20} value={settings.favoriteTabLimit} onChange={(e)=>setSettings({...settings,favoriteTabLimit:Number(e.target.value)})}/></label>
                            <label><span>お気に入りの列数 <SettingHelp text="各タブを等幅の列に分けます。各列にグループとリンクを上から並べ、列ごとにスクロールできます。1～10列。列数を減らしてもリンクは削除されず、右側の内容を最後の表示列にまとめます。列数を戻すと元の列に表示されます。" /></span><input type="number" min={1} max={10} value={settings.favoritePaneCount} onChange={(e)=>setSettings({...settings,favoritePaneCount:Math.max(1,Math.min(10,Number(e.target.value)||1))})}/></label>
                            <label><span>カードサイズ <SettingHelp text="お気に入りカードの高さと余白を変更します。多くの項目を一画面に表示したい場合は「コンパクト」を選んでください。" /></span><select value={settings.favoriteDensity} onChange={(e)=>setSettings({...settings,favoriteDensity:e.target.value as Settings["favoriteDensity"]})}><option value="compact">コンパクト</option><option value="standard">標準</option><option value="comfortable">ゆったり</option></select></label>
                            <label><span>Quickボタン数 <SettingHelp text="検索欄の上に表示する検索ショートカットの数です。0にするとQuickボタンの領域ごと非表示になり、検索欄を広く使えます。" /></span><input type="number" min={0} max={20} value={settings.displayCount} onChange={(e)=>setSettings({...settings,displayCount:Number(e.target.value)})}/></label>
                            <label><span>アプリサイズ <SettingHelp text="起動時に復元するアプリの大きさです。設定画面の全項目を見切れず表示できるよう、最小サイズは1100 × 760です。ウィンドウをマウスで直接リサイズした場合は「カスタム」として、その大きさを次回も復元します。" /></span><select value={presetWindowSizes.includes(settings.windowSize) ? settings.windowSize : "custom"} onChange={(e)=>{if(e.target.value!=="custom")setSettings({...settings,windowSize:e.target.value});}}><option value="1100x760">1100 × 760</option><option value="1200x800">1200 × 800</option><option value="1280x840">1280 × 840</option><option value="1360x900">1360 × 900</option><option value="custom">カスタム</option></select></label>
                            <div className="current-size-display"><span>現在のサイズ</span><strong>{currentWindowSize.replace("x"," × ")}</strong></div>
                            <label className="ui-preference"><span>お気に入り検索を表示 <SettingHelp text="お気に入りペインの検索欄を表示します。非表示にすると検索による絞り込みも解除され、現在のタブのすべてのお気に入りが表示されます。設定は再起動後も保持されます。" /></span><input type="checkbox" checked={uiPreferences.showFavoriteSearch} onChange={e=>setUiPreferences({...uiPreferences,showFavoriteSearch:e.target.checked})}/></label>
                            <label className="ui-preference"><span>リンクを開いた後は先頭タブ <SettingHelp text="Webページやフォルダを開いてアプリが隠れた後、再表示すると一番左のタブへ戻ります。オフの場合は使っていたタブを保持します。Escなどで隠しただけの場合はタブを変更しません。" /></span><input type="checkbox" checked={uiPreferences.resetTabAfterLink} onChange={e=>setUiPreferences({...uiPreferences,resetTabAfterLink:e.target.checked})}/></label>
                            <label><span>グループの初期色 <SettingHelp text="新しく作るグループの識別色です。既存のグループは自動では変わりません。「全グループに適用」を押した場合だけ、既存のグループもこの色に揃えます。" /></span><div className="heading-default-control"><input type="color" value={settings.headingDefaultColor} onChange={(e)=>setSettings({...settings,headingDefaultColor:e.target.value})}/><button type="button" onClick={()=>void applyHeadingColorToAll()}>全グループに適用</button></div></label>
                            <label><span>グループの色数 <SettingHelp text="各グループの設定メニューに表示する色の候補数です。32色は薄め中心、64色と128色では濃い色も段階的に追加されます。" /></span><select value={settings.headingPaletteSize} onChange={(e)=>setSettings({...settings,headingPaletteSize:Number(e.target.value) as 32|64|128})}><option value={32}>32色</option><option value={64}>64色</option><option value={128}>128色</option></select></label>
                          </div>
                        </section>

                        <section className="settings-section">
<div className="settings-section-title"><div><strong>ページ名の整形</strong><small>検索結果の表示と、お気に入り登録時の名前を整えます</small></div></div>
                          <div className="cleanup-settings-grid">
                            <label><span>不要語 <SettingHelp text="名前から取り除く語を1行に1つ登録します。検索結果は表示のみを整形します。お気に入りは登録時に取り除いた名前を保存し、編集欄にもその名前を表示します。登録後に不要語を追加しても既存の名前は自動変更しません。最大200件です。" /></span><textarea rows={3} value={blockedWordsText} onChange={(e)=>setBlockedWordsText(e.target.value)}/></label>

                          </div>
                        </section>

                        <section className="settings-section">
                          <div className="settings-section-title"><div><strong>フォルダ検索</strong><small>検索対象とインデックスの更新方法を設定します</small></div><div className="section-actions"><label>自動更新 <input type="number" min={30} max={86400} value={settings.folderSyncIntervalSeconds} onChange={(e)=>setSettings({...settings,folderSyncIntervalSeconds:Number(e.target.value)})}/> 秒 <SettingHelp text="最初のインデックス作成後に、追加・変更・削除されたファイルを確認する間隔です。既定値1800秒は30分です。短くしすぎるとネットワークフォルダやPCへ負荷がかかる場合があります。" /></label><button type="button" title="登録フォルダを今すぐ走査します。初回は全体を登録し、2回目以降は追加・変更・削除された差分だけを反映します。" onClick={()=>void rebuildIndex()}>今すぐ更新</button></div></div>
                          <label className="folder-paths-field"><span>対象フォルダ <SettingHelp text="検索したい親フォルダを1行に1つ入力します。ローカルパスとネットワーク共有パスを合わせて最大10件まで登録できます。サブフォルダも検索対象になります。" /></span><textarea rows={3} value={indexFoldersText} onChange={(e)=>setIndexFoldersText(e.target.value)} placeholder={'C:\\Work\n\\\\server\\share'}/></label>
                          {indexMessage?<div className="index-message">{indexMessage}</div>:null}
                        </section>

                        <section className="settings-section edge-settings-section">
                          <div className="settings-section-title"><div><strong>Edge履歴</strong><small>通常は拡張機能が自動で差分を取り込みます</small><span className={`edge-live-status${extensionConnection.connected?" connected":""}`}>● {extensionConnection.connected?`接続済み（最終通信 ${extensionConnection.lastContact ?? "-"}）`:`未接続・待機中（${extensionSyncMessage}）`}</span></div><button type="button" title="アプリ内のWeb履歴だけを削除します。Edge本体の履歴とお気に入りは削除しません。削除後は拡張機能へ全件再送を自動要求します。" onClick={()=>void handleEdgeHistoryClear()} disabled={edgeImportStatus.running}>履歴クリア</button></div>
                          <div className="edge-primary-row"><span>自動取得 <SettingHelp text="ページを開くと拡張機能からすぐ送信され、起動中のアプリは約1秒以内を目安に反映します。この間隔は取りこぼしを補う定期同期の間隔です。ブラウザーの制約で定期同期は最短30秒です。" /></span><label>間隔 <input type="number" min={10} max={3600} value={settings.edgeSyncIntervalSeconds} onChange={(e)=>setSettings({...settings,edgeSyncIntervalSeconds:Number(e.target.value)})}/> 秒</label><button type="button" title={`履歴の欠落が疑われる場合や、履歴クリア後にEdgeの全履歴を送り直します。実行は次回の拡張通信時です。\n現在の状態: ${extensionSyncMessage}`} onClick={async()=>{await invoke("request_extension_full_sync");setExtensionSyncMessage("全件受信を要求しました。次回通信を待っています");showToast("全件受信を要求しました");}}>全件受信</button></div>
                          <div className="edge-emergency-row"><span>非常用：Historyファイル読込 <SettingHelp text="Edge拡張機能が使えない場合だけ利用します。EdgeのHistoryファイルは使用中にロックされるため、読み込めない場合はEdgeを完全終了し、案内に従ってHistoryファイルを選択してください。通常は使いません。" /></span><button type="button" title="非常用の手動読込を開始します。Edgeが起動中だとHistoryファイルを読めない場合があります。" onClick={()=>void handleEdgeHistoryRefresh()} disabled={edgeImportStatus.running}>{edgeImportStatus.running?"読込中…":"読込"}</button></div>
                          <div className="edge-store-row"><button type="button" onClick={async()=>{const id=prompt("拡張機能のポップアップでコピーしたストア版IDを入力してください");if(!id)return;try{await invoke("configure_store_extension",{id:id.trim()});showToast("ストア版拡張機能との連携を登録しました");}catch(error){showToast(String(error));}}}>ストア版IDを登録</button><button type="button" onClick={()=>void invoke("open_edge_extensions")}>拡張機能の管理</button></div>
                        </section>
                      </div>

                      <div className="settings-pane settings-pane-right settings-stack">
                        <section className="settings-section quick-settings-section">
                          <div className="settings-section-title"><div><strong>Quickボタン</strong><small>よく使う検索語を上から順に登録します</small></div><SettingHelp text="検索欄の上に表示するショートカットです。1行が1つのボタンになります。実際に表示する個数は「表示とレイアウト」のQuickボタン数で指定します。" /></div>
                          <textarea rows={6} value={quickKeywordsText} onChange={(e)=>setQuickKeywordsText(e.target.value)}/>
                        </section>
                        <section className="settings-section data-tools-section">
<div className="settings-section-title"><div><strong>バックアップと診断</strong><small>お気に入り・レイアウト・設定を安全に保管します</small></div><span className="app-version">v0.3.4</span></div>
                          <div className="data-tool-actions"><button type="button" title="現在のDBを整合性のある状態で1ファイルへ保存します。お気に入り、タブ、列、色、設定をすべて含みます。" onClick={async()=>{const path=await invoke<string|null>("create_backup");if(path)showToast(`バックアップを保存しました: ${path}`);}}>バックアップ作成</button><button type="button" title="バックアップから復元します。復元直前の現在DBも自動退避し、完了後に画面を再読み込みします。" onClick={async()=>{if(!confirm("バックアップから復元しますか？"))return;const path=await invoke<string|null>("restore_backup");if(path)window.location.reload();}}>復元</button><button type="button" title="個人のURLやファイル名を含まない診断概要をクリップボードへコピーします。" onClick={async()=>{const value=await invoke<string>("diagnostics_text");await navigator.clipboard.writeText(value);showToast("診断情報をコピーしました");}}>診断情報をコピー</button></div>
                        </section>
                        <div className="edge-import-debug app-log-panel">
                            <div className="edge-import-debug-title"><span>ログ <SettingHelp text="Edge履歴やフォルダインデックスなど、アプリ内処理の結果を確認できます。最新500件だけを保存し、それより古いログは自動削除されるため、放置しても無制限には増えません。" /></span><button type="button" onClick={() => void clearAppLogs()}>クリア</button></div>
                            {appLogs.length > 0 ? (
                              <div className="edge-import-debug-list">
                                {appLogs.map((entry) => (
                                  <div key={entry.id} className={`edge-import-debug-entry ${entry.level}`}>
                                    <span className="edge-import-debug-meta">
                                      [{entry.timestamp}] [{entry.category}] [{entry.level}]
                                    </span>{" "}
                                    <span>{entry.message}</span>
                                  </div>
                                ))}
                              </div>
                            ) : (
                              <div className="edge-import-debug-empty">ログはありません。</div>
                            )}
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              )}
            </div>

            <div className="result-summary">
              <span>{query === "" ? (settings.searchMode === "web" ? "最近アクセス" : "最近更新された項目") : `${activeResultCount} 件の検索結果`}</span>
              <span>↑↓で移動 / Enter で候補選択 / Esc で閉じる</span>
            </div>

            {settings.searchMode === 'folder' && <div className="file-filters" role="group" aria-label="ファイル種類">{[['documents','文書中心'],['folders','フォルダ'],['excel','Excel'],['pdf','PDF'],['all','すべて'],['custom','その他…']].map(([value,label]) => <button key={value} className={fileFilter === value || (value === 'custom' && fileFilter.startsWith('custom:')) ? 'active' : ''} onClick={() => chooseFileFilter(value)}>{label}</button>)}</div>}
            <div className="results-list">
              {settings.searchMode === "folder" ? (
                fileResults.length === 0 ? <div className="empty-state">候補がありません。設定からインデックスを作成してください。</div> : fileResults.map((file, index) => (
                  <div key={file.id} className={`file-result-card ${index === selectedIndex ? "selected" : ""}`} title={`${file.name}\n${file.path}`}>
                    <button type="button" className={`result-star ${file.isFavorite ? "favorited" : ""}`} onClick={(event) => { event.stopPropagation(); void toggleFileFavorite(file); }}>{file.isFavorite ? "★" : "☆"}</button>
                    <button type="button" className="file-open-button" onClick={() => void openLocalPathAndHide(file.path)}><span className="file-kind">{fileIcons[file.path] ? <img className="source-icon" src={fileIcons[file.path]} alt=""/> : file.isDirectory ? "📁" : "▤"}</span><span className="file-result-main"><strong>{file.name}</strong><small>{file.path}</small></span></button>
                  </div>
                ))
              ) : filteredResults.length === 0 && query !== "" ? (
                <div className="empty-state">候補が見つかりませんでした。</div>
              ) : (
                filteredResults.map((result, index) => (
                  <div key={result.id} className={`result-card ${index === selectedIndex ? "selected" : ""}`} title={`${result.title}\n${result.url}\n最終: ${result.lastAccess ?? "-"}`}>
                    <button
                      type="button"
                      className={`result-star ${result.isFavorite === 1 ? "favorited" : ""}`}
                      onClick={(event) => {
                        event.stopPropagation();
                        toggleFavorite(result);
                      }}
                    >
                      {result.isFavorite === 1 ? "★" : "☆"}
                    </button>
                    <button
                      type="button"
                      className="result-content"
                      onClick={() => {
                        setSelectedIndex(index);
                        setSelectedResult(result);
                        setActionMessage(`「${result.title}」を選択しました。`);
                        openWebLink(result.url)
                          .then(() => {
                            try {
                              void getCurrentWindow().hide();
                            } catch {
                              // noop
                            }
                          })
                          .catch((error: unknown) => {
                            console.error("open_url error", error);
                            setActionMessage(`外部ブラウザで開けませんでした: ${String(error)}`);
                          });
                      }}
                    >
                      <div className="result-main">
                        <div className="result-title-row">
                          <SourceIcon src={webIcons[result.url] ?? websiteFaviconUrl(result.url)} service={result.service} />
                          {(() => {
                            const { displayTitle } = processTitleForDisplay(
                              result.title || "",
                              settings.blockedWords,
                            );
                            return (
                              <>
                                <div className="result-title">{displayTitle || result.title}</div>
                                <div className="site-name">{result.site ?? ""}</div>
                              </>
                            );
                          })()}
                        </div>
                        <div className="result-footer">
                          <div className="result-timestamps">
                            初回: {result.firstSeen ?? "-"}　最終: {result.lastAccess ?? "-"}　回数: {result.accessCount}
                          </div>
                          <div className="result-url">{result.url}</div>
                        </div>
                      </div>
                    </button>
                  </div>
                ))
              )}
            </div>
            {settings.searchMode === 'web' && <div className={`edge-connection-banner ${extensionConnection.connected ? 'connected' : ''}`}><span>{extensionConnection.connected ? 'Edge連携中' : 'Edge連携の応答がありません'}{extensionConnection.lastContact && <small>最終受信 {extensionConnection.lastContact}</small>}</span><button onClick={() => void invoke<ExtensionConnectionStatus>('extension_connection_status').then(setExtensionConnection)}>接続確認</button><button onClick={() => void invoke('open_edge_extensions')}>拡張機能を確認</button></div>}
            <section className={`search-inbox density-${settings.favoriteDensity}`} data-favorite-pane="0"><header>追加されたお気に入り <span>{favoriteItems.filter(item=>(item.pane===0 || !favoriteColumns.some(group=>group.id===item.pane)) && item.kind==='link').length}</span></header><div className="favorites-list">{favoriteItems.filter(item=>(item.pane===0 || !favoriteColumns.some(group=>group.id===item.pane)) && item.kind==='link').map(item=><FavoriteCard key={item.id} item={item} displayLabel={favoriteDisplayLabel(item)} iconSrc={favoriteIcon(item)} headingColors={headingColors} editing={editingFavoriteId===item.id} onDragStart={setDraggingFavoriteId} onPointerMove={trackFavoriteDrag} onPointerDrop={dropFavoriteAtPoint} onEdit={setEditingFavoriteId} onSave={saveFavoriteLabel} onDelete={removeFavorite} onOpen={openFavoriteTarget} onDropBefore={(dragId,beforeId)=>void moveFavorite(dragId,0,beforeId)} onColor={changeHeadingColor} draggingId={draggingFavoriteId} selectionMode={false} selected={false} onToggleSelect={()=>{}} />)}</div></section>
          </div>
        </div>

        <div className="pane-divider" onMouseDown={beginPaneResize} title="ドラッグして左右の幅を変更" />
        <div className="right-pane" style={{ flexBasis: `${100 - settings.leftPanePercent}%` }}>
          <div className={`favorites-container density-${settings.favoriteDensity}`} onClick={()=>setContextMenu(null)}>
            <div className="favorites-toolbar">
              <strong>お気に入り</strong>
              <button type="button" className={organizing?'active':''} onClick={()=>setOrganizing(!organizing)}>{organizing?'整理を完了':'整理'}</button>
              <button type="button" className="manual-favorite-button" onClick={()=>setManualFavoriteOpen(true)} title="URLやファイルパスを手入力してお気に入りに追加">＋ 手動登録</button>
              <button type="button" onClick={()=>void openTrash()} title="削除したお気に入り（30日・最大30件）">削除済閲覧</button>
            </div>
            <div className={`favorite-tabs${tabDropBeforeId===-1?" tab-drop-end":""}`}>
              {favoriteTabs.map((tab)=><button key={tab.id} data-favorite-tab-id={tab.id} className={`${activeTabId===tab.id?"active":""}${columnDropTarget?.type==="tab"&&columnDropTarget.id===tab.id?" column-drop-tab":""}${tabDropBeforeId===tab.id?" tab-drop-before":""}${draggingTabId===tab.id?" dragging-tab":""}`} style={{borderTopColor:tab.color}} onClick={()=>{if(suppressTabClickRef.current){suppressTabClickRef.current=false;return;}setActiveTabId(tab.id);}} onPointerDown={(e)=>{if(e.button!==0)return;tabPointerDragRef.current={id:tab.id,startX:e.clientX,startY:e.clientY,dragging:false};e.currentTarget.setPointerCapture(e.pointerId);}} onPointerMove={moveTabPointer} onPointerUp={endTabPointer} onPointerCancel={(e)=>{if(e.currentTarget.hasPointerCapture(e.pointerId))e.currentTarget.releasePointerCapture(e.pointerId);tabPointerDragRef.current=null;setDraggingTabId(null);setTabDropBeforeId(null);}} onDragOver={(e)=>{if(draggingColumnRef.current||draggingColumnId){e.preventDefault();e.dataTransfer.dropEffect="move";setColumnDropTarget({type:"tab",id:tab.id});}}} onDrop={(e)=>{const raw=e.dataTransfer.getData("text/plain");const id=Number(raw.replace("column:",""))||draggingColumnRef.current||draggingColumnId;if(id){e.preventDefault();const column=favoriteColumns.find((c)=>c.id===id);if(column&&column.tabId!==tab.id)void moveColumnToTab(column,tab.id);}draggingColumnRef.current=null;setDraggingColumnId(null);setColumnDropTarget(null);}} onContextMenu={(event)=>{event.preventDefault();event.stopPropagation();setContextMenu({type:"tab",id:tab.id,x:event.clientX,y:event.clientY});}} title="ドラッグで並べ替え。右クリックで名前・色を変更。グループをドロップすると中身ごとこのタブへ移動">{tab.name}</button>)}
              <button type="button" className="add-tab" onClick={()=>void addTab()} title="お気に入りタブを追加">＋</button>
            </div>
            <FavoriteBoard organizing={organizing} showSearch={uiPreferences.showFavoriteSearch} tabs={favoriteTabs} groups={favoriteColumns} items={favoriteItems} activeTab={activeTabId} setActiveTab={setActiveTabId} reload={async () => { await loadFavoriteLayout(); await loadFavorites(); }} moveItem={moveFavorite} toast={showToast} limit={settings.favoritePaneCount} initialColor={settings.headingDefaultColor} colors={headingColors} renderItem={(id, pane) => { const item = favoriteItems.find(value => value.id === id); return item ? <FavoriteCard key={id} item={item} displayLabel={favoriteDisplayLabel(item)} iconSrc={favoriteIcon(item)} headingColors={headingColors} editing={editingFavoriteId === id} onDragStart={setDraggingFavoriteId} onPointerMove={trackFavoriteDrag} onPointerDrop={dropFavoriteAtPoint} onEdit={setEditingFavoriteId} onSave={saveFavoriteLabel} onDelete={removeFavorite} onOpen={openFavoriteTarget} onDropBefore={(dragId, beforeId) => void moveFavorite(dragId, pane, beforeId)} onColor={changeHeadingColor} draggingId={draggingFavoriteId} dropBeforeId={favoriteDragPreview?.pane === pane ? favoriteDragPreview.beforeId : undefined} selectionMode={false} selected={false} onToggleSelect={() => {}} /> : null; }} />
          </div>
        </div>
      </div>
      {favoriteDragPreview && draggingFavoriteId ? <div className="favorite-drag-preview" style={{ left: favoriteDragPreview.x + 14, top: favoriteDragPreview.y + 14 }}>{favoriteItems.find((item) => item.id === draggingFavoriteId)?.label ?? "お気に入り"}</div> : null}
      {favoriteDragPreview?.lineY!==undefined && createPortal(<div className="drop-insertion-line" style={{left:favoriteDragPreview.lineX,top:favoriteDragPreview.lineY,width:favoriteDragPreview.lineWidth}} />,document.body)}
      {deletedFavorite ? <div className="undo-toast"><span>{deletedFavorite.kind === "heading" ? "見出し" : "お気に入り"}を削除しました</span><button type="button" onClick={()=>void undoFavoriteDelete()}>元に戻す</button><span className="toast-timer" /></div> : null}
      {toastMessage ? <div className="simple-toast">{toastMessage}</div>:null}
      {contextMenu ? <div className={`favorite-context-menu${contextMenu.type==="column"?" column-menu":""}`} style={{left:contextMenu.x,top:contextMenu.y}} onClick={(e)=>e.stopPropagation()}>
        {contextMenu.type==="tab" ? (()=>{const tab=favoriteTabs.find((v)=>v.id===contextMenu.id);return tab?<><button onClick={()=>void editTab(tab)}>名前変更</button><label>色 <input type="color" value={tab.color} onChange={(e)=>void setTabColor(tab,e.target.value)}/></label><button className="delete-column-menu" onClick={()=>void deleteTab(tab)}>このタブを削除</button></>:null;})() : (()=>{const column=favoriteColumns.find((v)=>v.id===contextMenu.id);return column?<button className="delete-column-menu" onClick={()=>void deleteColumn(column)}>この列を削除</button>:null;})()}
      </div>:null}
      {trashOpen?<div className="trash-backdrop" onClick={()=>setTrashOpen(false)}><div className="trash-dialog" onClick={(e)=>e.stopPropagation()}><div className="trash-header"><strong>削除済みのお気に入り</strong><button onClick={()=>setTrashOpen(false)}>×</button></div><p>30日後に自動削除されます（最大30件）。</p><div className="trash-list">{deletedFavorites.length?deletedFavorites.map((item)=><div key={item.id}><span>{favoriteDisplayLabel(item)}</span><small>{item.deletedAt}</small><button onClick={()=>void restoreTrash(item.id)}>戻す</button><button className="danger" onClick={()=>void purgeTrash(item.id)}>完全削除</button></div>):<div className="empty-favorites">削除済みはありません。</div>}</div></div></div>:null}
      {manualFavoriteOpen?<div className="trash-backdrop" onClick={()=>setManualFavoriteOpen(false)}><form className="manual-favorite-dialog" onSubmit={(e)=>{e.preventDefault();void submitManualFavorite();}} onClick={(e)=>e.stopPropagation()}><div className="trash-header"><strong>お気に入りを手動登録</strong><button type="button" onClick={()=>setManualFavoriteOpen(false)}>×</button></div><label>名前（省略可）<input value={manualFavoriteName} onChange={(e)=>setManualFavoriteName(e.target.value)} placeholder="表示する名前" autoFocus/></label><label>URLまたはファイルパス<input value={manualFavoriteTarget} onChange={(e)=>setManualFavoriteTarget(e.target.value)} placeholder="https://... または C:\\..."/></label><div className="manual-favorite-actions"><button type="button" onClick={()=>setManualFavoriteOpen(false)}>キャンセル</button><button type="submit">登録</button></div></form></div>:null}
    </div>
  );
}

export default App;
