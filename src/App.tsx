import { useCallback, useEffect, useMemo, useRef, useState, lazy, Suspense } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

const SkyIsland = lazy(() => import("./SkyIsland"));

type AccountInfo = {
  uuid: string;
  username: string;
  offline?: boolean;
};

type Settings = {
  microsoftClientId: string;
  selectedInstanceId: string | null;
  memoryMib: number;
  javaPathOverride: string | null;
  theme: string;
};

const THEMES = [
  { id: "grass", label: "Grass" },
  { id: "deepslate", label: "Deepslate" },
  { id: "nether", label: "Nether" },
  { id: "copper", label: "Copper" },
  { id: "pale", label: "Pale" },
] as const;

function themeLabel(id: string): string {
  return THEMES.find((t) => t.id === id)?.label ?? "Grass";
}

function applyTheme(theme: string) {
  const id = THEMES.some((t) => t.id === theme) ? theme : "grass";
  document.documentElement.dataset.theme = id;
}

type VersionInfo = {
  id: string;
  versionType: string;
  url: string;
  time: string;
  releaseTime: string;
  sha1?: string;
};

type ModLoader = "vanilla" | "fabric" | "quilt" | "forge" | "neoforge";

type Instance = {
  id: string;
  name: string;
  versionId: string;
  loader?: ModLoader;
  loaderVersion?: string | null;
  launchVersionId?: string | null;
  createdAt: string;
};

type ModHit = {
  projectId: string;
  slug: string;
  title: string;
  description: string;
  iconUrl?: string | null;
  downloads: number;
  categories: string[];
};

const LOADERS: { id: ModLoader; label: string; ready: boolean; blurb: string }[] = [
  { id: "vanilla", label: "Vanilla", ready: true, blurb: "Plain Minecraft, no mods" },
  { id: "fabric", label: "Fabric", ready: true, blurb: "Lightweight mods via Modrinth" },
  { id: "quilt", label: "Quilt", ready: true, blurb: "Fabric-compatible fork" },
  { id: "forge", label: "Forge", ready: false, blurb: "Coming soon" },
  { id: "neoforge", label: "NeoForge", ready: false, blurb: "Coming soon" },
];

function loaderLabel(id?: ModLoader | null): string {
  return LOADERS.find((l) => l.id === (id ?? "vanilla"))?.label ?? "Vanilla";
}

type DownloadProgress = {
  phase: string;
  id: string;
  file: string;
  bytesDone: number;
  bytesTotal: number;
  filesDone: number;
  filesTotal: number;
};

type LoginCode = {
  userCode: string;
  verificationUri: string;
  message: string;
};

function errMessage(err: unknown): string {
  if (typeof err === "string") return err;
  if (err && typeof err === "object" && "message" in err) {
    return String((err as { message: unknown }).message);
  }
  return String(err);
}

export default function App() {
  const [account, setAccount] = useState<AccountInfo | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [versions, setVersions] = useState<VersionInfo[]>([]);
  const [instances, setInstances] = useState<Instance[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [newName, setNewName] = useState("");
  const [newVersion, setNewVersion] = useState("");
  const [newLoader, setNewLoader] = useState<ModLoader>("vanilla");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [loginCode, setLoginCode] = useState<LoginCode | null>(null);
  const [memoryDraft, setMemoryDraft] = useState(2048);
  const [clientIdDraft, setClientIdDraft] = useState("");
  const [offlineName, setOfflineName] = useState("Player");
  const [createOpen, setCreateOpen] = useState(false);
  const [loaderOpen, setLoaderOpen] = useState(false);
  const [modsOpen, setModsOpen] = useState(false);
  const [modQuery, setModQuery] = useState("");
  const [modHits, setModHits] = useState<ModHit[]>([]);
  const [installedMods, setInstalledMods] = useState<string[]>([]);
  const [themeOpen, setThemeOpen] = useState(false);
  const [theme, setTheme] = useState("grass");
  const [themeBrowse, setThemeBrowse] = useState(0);
  const [themeSlide, setThemeSlide] = useState<"none" | "next" | "prev">("none");
  const themeBrowseRef = useRef(0);
  const themeBeforeRef = useRef("grass");
  const themeSlideTimer = useRef<number | null>(null);

  const browseTheme = THEMES[themeBrowse] ?? THEMES[0];
  const prevTheme = THEMES[(themeBrowse - 1 + THEMES.length) % THEMES.length];
  const nextTheme = THEMES[(themeBrowse + 1) % THEMES.length];

  const selected = useMemo(
    () => instances.find((i) => i.id === selectedId) ?? null,
    [instances, selectedId],
  );

  const refresh = useCallback(async () => {
    const [s, inst, vers, acc] = await Promise.all([
      invoke<Settings>("get_settings"),
      invoke<Instance[]>("list_instances"),
      invoke<VersionInfo[]>("list_versions"),
      invoke<AccountInfo | null>("get_account"),
    ]);
    setSettings(s);
    setMemoryDraft(s.memoryMib);
    setClientIdDraft(s.microsoftClientId ?? "");
    const nextTheme = s.theme || "grass";
    setTheme(nextTheme);
    applyTheme(nextTheme);
    setInstances(inst);
    setVersions(vers.filter((v) => v.versionType === "release"));
    setAccount(acc);
    setSelectedId(s.selectedInstanceId ?? inst[0]?.id ?? null);
    if (!newVersion) {
      const latestRelease = vers.find((v) => v.versionType === "release");
      if (latestRelease) {
        setNewVersion(latestRelease.id);
        setNewName(latestRelease.id);
      }
    }
  }, [newVersion]);

  useEffect(() => {
    if ((!createOpen && !modsOpen) || loaderOpen || themeOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        setCreateOpen(false);
        setModsOpen(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [createOpen, modsOpen, loaderOpen, themeOpen]);

  useEffect(() => {
    let unsubs: Array<() => void> = [];
    (async () => {
      try {
        setBusy("Loading");
        await invoke("restore_session").catch(() => null);
        await refresh();
        const u1 = await listen<DownloadProgress>("download-progress", (e) => {
          setProgress(e.payload);
          if (e.payload.phase === "java") {
            setBusy("Downloading Java…");
          }
        });
        const u2 = await listen("install-complete", () => {
          setBusy(null);
          setProgress(null);
        });
        const u3 = await listen("game-exited", () => {
          setBusy(null);
        });
        const u4 = await listen<LoginCode>("login-code", (e) => {
          setLoginCode(e.payload);
          setBusy("Waiting for Microsoft sign-in…");
        });
        unsubs = [u1, u2, u3, u4];
      } catch (err) {
        setError(errMessage(err));
      } finally {
        setBusy(null);
      }
    })();
    return () => {
      unsubs.forEach((u) => u());
    };
  }, [refresh]);

  async function saveSettingsPatch(patch: Partial<Settings>) {
    if (!settings) return;
    const next = { ...settings, ...patch };
    await invoke("save_settings", { settings: next });
    setSettings(next);
  }

  function openLoaderBrowser() {
    setLoaderOpen(true);
  }

  function pickLoader(id: ModLoader, ready: boolean) {
    if (!ready) return;
    setNewLoader(id);
    setLoaderOpen(false);
  }

  useEffect(() => {
    if (!loaderOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        setLoaderOpen(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [loaderOpen]);

  function openThemeBrowser() {
    const idx = THEMES.findIndex((t) => t.id === theme);
    const start = idx >= 0 ? idx : 0;
    themeBeforeRef.current = theme;
    setThemeBrowse(start);
    themeBrowseRef.current = start;
    setThemeOpen(true);
    applyTheme(THEMES[start].id);
  }

  function cancelThemeBrowse() {
    const prev = themeBeforeRef.current;
    applyTheme(prev);
    setTheme(prev);
    setThemeOpen(false);
  }

  async function confirmThemeBrowse() {
    const chosen = THEMES[themeBrowseRef.current]?.id ?? "grass";
    setTheme(chosen);
    applyTheme(chosen);
    setThemeOpen(false);
    try {
      await saveSettingsPatch({ theme: chosen });
    } catch (err) {
      setError(errMessage(err));
    }
  }

  function stepTheme(delta: number) {
    setThemeSlide(delta > 0 ? "next" : "prev");
    if (themeSlideTimer.current) window.clearTimeout(themeSlideTimer.current);
    themeSlideTimer.current = window.setTimeout(() => setThemeSlide("none"), 420);
    setThemeBrowse((i) => {
      const next = (i + delta + THEMES.length) % THEMES.length;
      themeBrowseRef.current = next;
      applyTheme(THEMES[next].id);
      return next;
    });
  }

  useEffect(() => {
    if (!themeOpen) return;

    let wheelAcc = 0;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        cancelThemeBrowse();
        return;
      }
      if (e.key === "ArrowLeft" || e.key === "a" || e.key === "A") {
        e.preventDefault();
        stepTheme(-1);
      } else if (e.key === "ArrowRight" || e.key === "d" || e.key === "D") {
        e.preventDefault();
        stepTheme(1);
      } else if (e.key === "Enter") {
        e.preventDefault();
        void confirmThemeBrowse();
      }
    };

    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const horizontal = Math.abs(e.deltaX) > Math.abs(e.deltaY);
      wheelAcc += horizontal ? e.deltaX : e.deltaY;
      if (Math.abs(wheelAcc) < 40) return;
      stepTheme(wheelAcc > 0 ? 1 : -1);
      wheelAcc = 0;
    };

    window.addEventListener("keydown", onKey);
    window.addEventListener("wheel", onWheel, { passive: false });
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("wheel", onWheel);
    };
  }, [themeOpen]);

  async function onOfflineLogin() {
    setError(null);
    setLoginCode(null);
    setBusy("Signing in…");
    try {
      const acc = await invoke<AccountInfo>("login_offline", {
        username: offlineName.trim() || "Player",
      });
      setAccount(acc);
    } catch (err) {
      setError(errMessage(err));
    } finally {
      setBusy(null);
    }
  }

  async function onLogin() {
    setError(null);
    setLoginCode(null);
    setBusy("Starting Microsoft sign-in…");
    try {
      if (clientIdDraft.trim() !== (settings?.microsoftClientId ?? "")) {
        await saveSettingsPatch({ microsoftClientId: clientIdDraft.trim() });
      }
      if (!clientIdDraft.trim()) {
        setError(
          'Create a Microsoft Entra app named "Cubic", paste its Application (client) ID below, then try again.',
        );
        return;
      }
      const acc = await invoke<AccountInfo>("login");
      setAccount(acc);
      setLoginCode(null);
    } catch (err) {
      setError(errMessage(err));
    } finally {
      setBusy(null);
    }
  }

  async function onLogout() {
    setError(null);
    try {
      await invoke("logout");
      setAccount(null);
      setLoginCode(null);
    } catch (err) {
      setError(errMessage(err));
    }
  }

  async function onCreate() {
    setError(null);
    setBusy("Creating instance…");
    setProgress(null);
    try {
      const created = await invoke<Instance>("create_instance", {
        request: {
          name: newName.trim() || newVersion,
          versionId: newVersion,
          loader: newLoader,
        },
      });
      await refresh();
      setSelectedId(created.id);
      setNewName(newVersion);
      setCreateOpen(false);
      setBusy("Installing…");
      await invoke("install_instance", { id: created.id });
      // busy / progress cleared by install-complete
    } catch (err) {
      setError(errMessage(err));
      setBusy(null);
      setProgress(null);
    }
  }

  async function openMods() {
    if (!selected) return;
    if ((selected.loader ?? "vanilla") === "vanilla") {
      setError("Pick Fabric or Quilt when creating an instance to install mods.");
      return;
    }
    setError(null);
    setModsOpen(true);
    setModQuery("");
    setBusy("Loading popular mods…");
    try {
      const [installed, hits] = await Promise.all([
        invoke<string[]>("list_installed_mods", { instanceId: selected.id }),
        invoke<ModHit[]>("search_mods", { instanceId: selected.id, query: "" }),
      ]);
      setInstalledMods(installed);
      setModHits(hits);
    } catch (err) {
      setError(errMessage(err));
    } finally {
      setBusy(null);
    }
  }

  async function onSearchMods() {
    if (!selected) return;
    setError(null);
    const q = modQuery.trim();
    setBusy(q ? "Searching Modrinth…" : "Loading popular mods…");
    try {
      const hits = await invoke<ModHit[]>("search_mods", {
        instanceId: selected.id,
        query: q,
      });
      setModHits(hits);
    } catch (err) {
      setError(errMessage(err));
    } finally {
      setBusy(null);
    }
  }

  async function onInstallMod(projectId: string) {
    if (!selected) return;
    setError(null);
    setBusy("Installing mod…");
    try {
      const file = await invoke<string>("install_mod", {
        instanceId: selected.id,
        projectId,
      });
      const installed = await invoke<string[]>("list_installed_mods", {
        instanceId: selected.id,
      });
      setInstalledMods(installed);
      setBusy(null);
      setProgress(null);
      if (file) {
        /* keep quiet success via list refresh */
      }
    } catch (err) {
      setError(errMessage(err));
      setBusy(null);
    }
  }

  async function onRemoveMod(fileName: string) {
    if (!selected) return;
    try {
      await invoke("remove_mod", { instanceId: selected.id, fileName });
      setInstalledMods(await invoke<string[]>("list_installed_mods", { instanceId: selected.id }));
    } catch (err) {
      setError(errMessage(err));
    }
  }

  function openCreate() {
    if (!newName && newVersion) setNewName(newVersion);
    setCreateOpen(true);
  }

  async function onInstall() {
    if (!selected) return;
    setError(null);
    setBusy("Installing…");
    setProgress(null);
    try {
      await invoke("install_instance", { id: selected.id });
    } catch (err) {
      setError(errMessage(err));
      setBusy(null);
    }
  }

  async function onCancelInstall() {
    try {
      await invoke("cancel_install");
    } catch (err) {
      setError(errMessage(err));
    }
  }

  async function onPlay() {
    if (!selected) return;
    setError(null);
    setBusy("Launching…");
    setProgress(null);
    try {
      if (memoryDraft !== settings?.memoryMib) {
        await saveSettingsPatch({ memoryMib: memoryDraft });
      }
      await saveSettingsPatch({ selectedInstanceId: selected.id });
      await invoke("launch_instance", { id: selected.id });
      setProgress(null);
      setBusy("Game running…");
    } catch (err) {
      setError(errMessage(err));
      setBusy(null);
      setProgress(null);
    }
  }

  async function onDelete() {
    if (!selected) return;
    setError(null);
    try {
      await invoke("delete_instance", { id: selected.id });
      await refresh();
    } catch (err) {
      setError(errMessage(err));
    }
  }

  const pct =
    progress && progress.filesTotal > 0
      ? Math.min(100, Math.round((progress.filesDone / progress.filesTotal) * 100))
      : progress && progress.bytesTotal > 0
        ? Math.min(100, Math.round((progress.bytesDone / progress.bytesTotal) * 100))
        : 0;

  const skyInstances = useMemo(
    () =>
      instances.map((inst) => ({
        id: inst.id,
        name: inst.name,
        versionId: inst.versionId,
        loaderLabel: loaderLabel(inst.loader),
      })),
    [instances],
  );

  return (
    <div className="app">
      <div className="stage">
        <section className="stage-main">
          <Suspense fallback={<div className="sky-scene sky-fallback" aria-hidden />}>
            <SkyIsland
              instances={skyInstances}
              selectedId={selectedId}
              onSelect={setSelectedId}
            />
          </Suspense>

          <header className="sky-chrome top">
            <div className="brand-lockup">
              <img className="brand-mark" src="/icon.png" alt="" width={44} height={44} />
              <div>
                <h1 className="brand">
                  Cub<span>ic</span>
                </h1>
                <p className="tagline">Windows launcher</p>
              </div>
            </div>
            <button type="button" className="create-open" onClick={openCreate} disabled={!!busy}>
              New instance
            </button>
          </header>

          <div className="sky-chrome bottom">
            <div className="stage-hud" key={selected ? selected.id : "empty"}>
              {selected ? (
                <>
                  <p className="stage-label">Selected</p>
                  <h2>{selected.name}</h2>
                  <p className="meta">
                    {selected.versionId} · {loaderLabel(selected.loader)}
                  </p>
                </>
              ) : (
                <>
                  <p className="stage-label">Instance</p>
                  <h2>{instances.length ? "Select an instance" : "No instances"}</h2>
                  <p className="meta">
                    {instances.length
                      ? "Click a pod · drag to rotate"
                      : "Create an instance to get started"}
                  </p>
                </>
              )}
            </div>

            <div className="stage-actions">
              <button
                type="button"
                className="play"
                onClick={onPlay}
                disabled={!selected || !account || !!busy}
              >
                Play
              </button>
              <button type="button" className="ghost" onClick={onInstall} disabled={!selected || !!busy}>
                Install
              </button>
              <button
                type="button"
                className="ghost"
                onClick={() => void openMods()}
                disabled={!selected || !!busy || (selected.loader ?? "vanilla") === "vanilla"}
              >
                Mods
              </button>
              <button
                type="button"
                className="ghost"
                onClick={onCancelInstall}
                disabled={busy !== "Installing…"}
              >
                Cancel
              </button>
              <button
                type="button"
                className="danger"
                onClick={onDelete}
                disabled={!selected || !!busy}
              >
                Delete
              </button>
            </div>

            {progress && (
              <div className="progress-wrap">
                <div className="progress" aria-label="Download progress">
                  <span style={{ width: `${pct}%` }} />
                </div>
                <p className="muted">
                  {progress.phase}: {progress.filesDone}/{progress.filesTotal} · {progress.id}
                </p>
              </div>
            )}

            {loginCode && (
              <div className="login-code">
                <p className="muted">Enter this code in the browser</p>
                <p className="code">{loginCode.userCode}</p>
                <p className="muted">{loginCode.verificationUri}</p>
              </div>
            )}

            {busy && <p className="status-line">{busy}</p>}
            {error && <div className="error">{error}</div>}
          </div>
        </section>

        <footer className="dock">
          <div className="dock-fields">
            <div className="field">
              <label htmlFor="clientId">Microsoft app ID</label>
              <input
                id="clientId"
                value={clientIdDraft}
                onChange={(e) => setClientIdDraft(e.target.value)}
                placeholder="Azure Application (client) ID"
                autoComplete="off"
                spellCheck={false}
              />
              <p className="hint">
                App named <strong>Cubic</strong>, public client flows on.
              </p>
            </div>
            <div className="field">
              <label htmlFor="memory">Memory (MiB)</label>
              <input
                id="memory"
                type="number"
                min={512}
                step={256}
                value={memoryDraft}
                onChange={(e) => setMemoryDraft(Number(e.target.value))}
              />
            </div>
            <div className="field">
              <label htmlFor="theme">Theme</label>
              <button
                type="button"
                id="theme"
                className="theme-trigger"
                onClick={openThemeBrowser}
              >
                <span className="theme-preview" data-theme-id={theme} aria-hidden />
                <span>{themeLabel(theme)}</span>
              </button>
            </div>
          </div>
          <div className="dock-actions">
            {account ? (
              <>
                <span className="account-pill">
                  <span className="avatar" aria-hidden>
                    {account.username.slice(0, 1).toUpperCase()}
                  </span>
                  {account.username}
                  {account.offline ? <span className="account-mode">offline</span> : null}
                </span>
                <button type="button" className="ghost" onClick={onLogout}>
                  Sign out
                </button>
              </>
            ) : (
              <>
                <label className="offline-login">
                  <span className="sr-only">Username</span>
                  <input
                    value={offlineName}
                    onChange={(e) => setOfflineName(e.target.value)}
                    placeholder="Username"
                    maxLength={16}
                    autoComplete="username"
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void onOfflineLogin();
                    }}
                  />
                </label>
                <button type="button" onClick={() => void onOfflineLogin()} disabled={!!busy}>
                  Continue
                </button>
                <button type="button" className="ghost" onClick={onLogin} disabled={!!busy}>
                  Microsoft
                </button>
              </>
            )}
            <button
              type="button"
              className="ghost"
              onClick={() =>
                invoke("open_azure_setup").catch((err) => setError(errMessage(err)))
              }
            >
              Azure
            </button>
            <button
              type="button"
              className="ghost"
              onClick={() =>
                invoke("open_mojang_app_review").catch((err) => setError(errMessage(err)))
              }
            >
              Mojang
            </button>
            <button
              type="button"
              className="ghost"
              onClick={() =>
                saveSettingsPatch({
                  microsoftClientId: clientIdDraft.trim(),
                  memoryMib: memoryDraft,
                  theme,
                }).catch((err) => setError(errMessage(err)))
              }
            >
              Save
            </button>
          </div>
        </footer>
      </div>

      {createOpen && (
        <div className="create-overlay" role="dialog" aria-modal="true" aria-label="Create instance">
          <div
            className="create-overlay-bg"
            onClick={() => setCreateOpen(false)}
          />
          <div className="create-panel">
            <p className="create-kicker">New instance</p>
            <h2 className="create-title">Create instance</h2>

            <div className="create-fields">
              <div className="field">
                <label htmlFor="name">Name</label>
                <input
                  id="name"
                  value={newName}
                  onChange={(e) => setNewName(e.target.value)}
                  placeholder="Instance name"
                  autoFocus
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && newVersion && !busy) void onCreate();
                  }}
                />
              </div>

              <div className="field">
                <label htmlFor="loader">Loader</label>
                <button
                  type="button"
                  id="loader"
                  className="loader-trigger"
                  onClick={openLoaderBrowser}
                >
                  <span className="loader-badge" data-loader={newLoader} aria-hidden />
                  <span>{loaderLabel(newLoader)}</span>
                </button>
                <p className="hint">
                  Mods need <strong>Fabric</strong> or <strong>Quilt</strong>. Source: Modrinth.
                </p>
              </div>

              <div className="field">
                <label>Version</label>
                <div className="create-version-list" role="listbox" aria-label="Minecraft version">
                  {versions.map((v) => (
                    <button
                      key={v.id}
                      type="button"
                      role="option"
                      aria-selected={v.id === newVersion}
                      className={`create-version ${v.id === newVersion ? "active" : ""}`}
                      onClick={() => {
                        setNewVersion(v.id);
                        setNewName((name) =>
                          !name || name === newVersion ? v.id : name,
                        );
                      }}
                    >
                      <span className="cube" aria-hidden />
                      <span>{v.id}</span>
                    </button>
                  ))}
                </div>
              </div>
            </div>

            <div className="create-actions">
              <button type="button" className="ghost" onClick={() => setCreateOpen(false)}>
                Cancel
              </button>
              <button type="button" onClick={() => void onCreate()} disabled={!newVersion || !!busy}>
                Create
              </button>
            </div>
            <p className="create-help">Esc cancel · Enter create</p>
          </div>
        </div>
      )}

      {loaderOpen && (
        <div className="loader-overlay" role="dialog" aria-modal="true" aria-label="Choose loader">
          <div className="loader-overlay-bg" onClick={() => setLoaderOpen(false)} />
          <div className="loader-expand">
            <p className="create-kicker">Loader</p>
            <h2 className="create-title">Choose loader</h2>
            <div className="loader-grid">
              {LOADERS.map((l, i) => (
                <button
                  key={l.id}
                  type="button"
                  className={`loader-tile ${newLoader === l.id ? "active" : ""} ${l.ready ? "" : "soon"}`}
                  data-loader={l.id}
                  style={{ animationDelay: `${60 + i * 55}ms` }}
                  disabled={!l.ready}
                  onClick={() => pickLoader(l.id, l.ready)}
                >
                  <span className="loader-tile-mark" aria-hidden />
                  <strong>{l.label}</strong>
                  <span className="loader-tile-blurb">{l.ready ? l.blurb : "Coming soon"}</span>
                </button>
              ))}
            </div>
            <button type="button" className="ghost loader-expand-close" onClick={() => setLoaderOpen(false)}>
              Close
            </button>
          </div>
        </div>
      )}

      {modsOpen && selected && (
        <div className="mods-overlay" role="dialog" aria-modal="true" aria-label="Mods">
          <div className="mods-overlay-bg" onClick={() => setModsOpen(false)} />
          <div className="mods-panel">
            <p className="create-kicker">Modrinth</p>
            <h2 className="create-title">Mods</h2>
            <p className="muted">
              {selected.name} · {selected.versionId} · {loaderLabel(selected.loader)}
            </p>

            <div className="mods-search">
              <input
                value={modQuery}
                onChange={(e) => setModQuery(e.target.value)}
                placeholder="Search mods"
                onKeyDown={(e) => {
                  if (e.key === "Enter") void onSearchMods();
                }}
              />
              <button type="button" onClick={() => void onSearchMods()} disabled={!!busy}>
                Search
              </button>
            </div>

            <div className="mods-columns">
              <div className="mods-col">
                <h3>{modQuery.trim() ? "Results" : "Popular"}</h3>
                <div className="mods-list">
                  {modHits.length === 0 ? (
                    <p className="muted">
                      {busy ? "Loading…" : "No mods found for this version/loader."}
                    </p>
                  ) : (
                    modHits.map((hit) => (
                      <div key={hit.projectId} className="mod-row">
                        {hit.iconUrl ? (
                          <img
                            className="mod-icon"
                            src={hit.iconUrl}
                            alt=""
                            loading="lazy"
                            referrerPolicy="no-referrer"
                          />
                        ) : (
                          <span className="mod-icon fallback" aria-hidden />
                        )}
                        <div className="mod-meta">
                          <strong>{hit.title}</strong>
                          <p className="muted">{hit.description}</p>
                          <p className="mod-downloads">
                            {hit.downloads.toLocaleString()} downloads
                          </p>
                        </div>
                        <button
                          type="button"
                          onClick={() => void onInstallMod(hit.projectId)}
                          disabled={!!busy}
                        >
                          Install
                        </button>
                      </div>
                    ))
                  )}
                </div>
              </div>
              <div className="mods-col">
                <h3>Installed</h3>
                <div className="mods-list">
                  {installedMods.length === 0 ? (
                    <p className="muted">No jars in mods/ yet.</p>
                  ) : (
                    installedMods.map((file) => (
                      <div key={file} className="mod-row installed">
                        <strong>{file}</strong>
                        <button
                          type="button"
                          className="danger"
                          onClick={() => void onRemoveMod(file)}
                        >
                          Remove
                        </button>
                      </div>
                    ))
                  )}
                </div>
              </div>
            </div>

            <div className="create-actions">
              <button type="button" className="ghost" onClick={() => setModsOpen(false)}>
                Close
              </button>
            </div>
          </div>
        </div>
      )}

      {themeOpen && (
        <div className="theme-overlay" role="dialog" aria-modal="true" aria-label="Choose theme">
          <div className="theme-overlay-bg" onClick={cancelThemeBrowse} />
          <div className="theme-carousel">
            <p className="theme-overlay-kicker">Theme</p>
            <div className={`theme-carousel-row slide-${themeSlide}`}>
              <button
                type="button"
                className="theme-card side left"
                data-theme-preview={prevTheme.id}
                onClick={() => stepTheme(-1)}
                aria-label={`Previous: ${prevTheme.label}`}
              >
                <div className="theme-card-face" key={`prev-${prevTheme.id}`}>
                  <span className="theme-card-cube" />
                  <strong>{prevTheme.label}</strong>
                </div>
              </button>

              <button
                type="button"
                className="theme-card center"
                data-theme-preview={browseTheme.id}
                onClick={() => void confirmThemeBrowse()}
                aria-label={`Select ${browseTheme.label}`}
              >
                <div className="theme-card-face" key={`center-${browseTheme.id}`}>
                  <span className="theme-card-cube" />
                  <strong>{browseTheme.label}</strong>
                  <span className="theme-card-hint">Enter to use</span>
                </div>
              </button>

              <button
                type="button"
                className="theme-card side right"
                data-theme-preview={nextTheme.id}
                onClick={() => stepTheme(1)}
                aria-label={`Next: ${nextTheme.label}`}
              >
                <div className="theme-card-face" key={`next-${nextTheme.id}`}>
                  <span className="theme-card-cube" />
                  <strong>{nextTheme.label}</strong>
                </div>
              </button>
            </div>
            <p className="theme-overlay-help">
              Scroll · ← → · A D · Esc cancel
            </p>
            <div className="theme-overlay-actions">
              <button type="button" className="ghost" onClick={cancelThemeBrowse}>
                Cancel
              </button>
              <button type="button" onClick={() => void confirmThemeBrowse()}>
                Use {browseTheme.label}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
