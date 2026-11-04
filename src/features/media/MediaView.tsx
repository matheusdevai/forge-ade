import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  AudioLines,
  CheckCircle2,
  CircleAlert,
  CircleDashed,
  Clapperboard,
  FileAudio,
  FolderOpen,
  ImagePlus,
  KeyRound,
  Loader2,
  Save,
  ScanEye,
  Trash2,
  Upload,
} from "lucide-react";
import { toast } from "sonner";
import { Badge, Button, Card, CardHeader, Dialog, DialogContent, EmptyState, Input, Label, Textarea } from "@/components/ui";
import { api, errorMessage, type MediaAsset, type MediaInput, type MediaOp, type MediaStatus } from "@/lib/ipc";
import { cn, relativeTime } from "@/lib/utils";
import { useWorkspaces } from "@/stores/data";
import { useSettings } from "@/stores/settings";

type Tab = "image" | "analyze" | "tts" | "transcribe" | "video" | "library";
const TABS: { id: Tab; label: string; icon: typeof ImagePlus }[] = [
  { id: "image", label: "Gerar imagem", icon: ImagePlus },
  { id: "analyze", label: "Analisar imagem", icon: ScanEye },
  { id: "tts", label: "Voz", icon: AudioLines },
  { id: "transcribe", label: "Transcrever", icon: FileAudio },
  { id: "video", label: "Vídeo", icon: Clapperboard },
  { id: "library", label: "Biblioteca", icon: FolderOpen },
];
const OP_LABEL: Record<MediaOp, string> = {
  "generate-image": "Gerar imagem",
  "analyze-image": "Analisar imagem / transcrever",
  tts: "Voz (TTS)",
  transcribe: "Transcrição",
  video: "Vídeo",
};
const ASSET_LABEL: Record<string, string> = {
  "generate-image": "Imagem gerada",
  "analyze-image": "Análise de imagem",
  tts: "Voz",
  transcribe: "Transcrição",
  video: "Vídeo",
  screenshot: "Captura",
};
/** Prebuilt voices documented for Gemini TTS. */
const VOICES = ["Kore", "Puck", "Charon", "Fenrir", "Aoede", "Leda", "Orus", "Zephyr"];
const select = "h-8 rounded-md border border-border bg-bg px-2 text-[12.5px] text-fg outline-none focus:border-accent/60";

function b64ToBlobUrl(data: string, mime: string): string {
  const bin = atob(data);
  const arr = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) arr[i] = bin.charCodeAt(i);
  return URL.createObjectURL(new Blob([arr], { type: mime }));
}

function useAssetUrl(asset: MediaAsset | null) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!asset?.path || asset.status !== "done") return;
    let alive = true;
    let made: string | null = null;
    api
      .mediaRead(asset.id)
      .then((d) => {
        if (!alive) return;
        made = b64ToBlobUrl(d.data, d.mime);
        setUrl(made);
      })
      .catch(() => undefined);
    return () => {
      alive = false;
      if (made) URL.revokeObjectURL(made);
      setUrl(null);
    };
  }, [asset?.id, asset?.path, asset?.status]);
  return url;
}

function AssetPreview({ asset, compact }: { asset: MediaAsset; compact?: boolean }) {
  const url = useAssetUrl(asset);
  if (asset.status === "running") {
    return (
      <div className="flex h-24 items-center justify-center gap-2 text-xs text-muted">
        <Loader2 className="size-4 animate-spin" /> {asset.op === "video" ? "Gerando vídeo (pode levar minutos)…" : "Processando…"}
      </div>
    );
  }
  if (asset.status === "failed") return <p className="text-[12px] text-error">{asset.error}</p>;
  if (asset.kind === "text") return <p className={cn("text-[12.5px] leading-relaxed whitespace-pre-wrap text-fg", compact && "line-clamp-5")}>{asset.resultText}</p>;
  if (!url) return <div className="h-24 animate-pulse rounded bg-elevated" />;
  if (asset.kind === "image") return <img src={url} alt={asset.prompt} className={cn("w-full rounded-md object-contain", compact ? "max-h-40" : "max-h-[60vh]")} />;
  if (asset.kind === "audio") return <audio src={url} controls className="w-full" />;
  return <video src={url} controls className={cn("w-full rounded-md", compact ? "max-h-40" : "max-h-[60vh]")} />;
}

function Providers({ status, onChange }: { status: MediaStatus | null; onChange: (s: MediaStatus) => void }) {
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const g = status?.gemini;
  const refresh = () => void api.mediaStatus().then(onChange).catch(() => undefined);
  const run = async (f: () => Promise<unknown>, ok?: string) => {
    setBusy(true);
    try {
      await f();
      if (ok) toast.success(ok);
    } catch (e) {
      toast.error("Falha", { description: errorMessage(e) });
    } finally {
      setBusy(false);
      refresh();
    }
  };
  return (
    <Card>
      <CardHeader title="Provedores" />
      <div className="space-y-3 p-4 text-[12.5px]">
        <div>
          <div className="flex items-center gap-2 font-medium">
            Google Gemini
            {!g?.keySet ? (
              <Badge>sem chave</Badge>
            ) : g.state.lastOk === true ? (
              <Badge tone="success">verificado · {g.state.models.length} modelos</Badge>
            ) : g.state.lastOk === false ? (
              <Badge tone="error">falhou</Badge>
            ) : (
              <Badge tone="warning">não verificado</Badge>
            )}
          </div>
          {g?.state.lastError && <p className="mt-1 text-[11.5px] text-error">{g.state.lastError}</p>}
          <form
            className="mt-2 flex gap-1.5"
            onSubmit={(e) => {
              e.preventDefault();
              const k = key;
              setKey("");
              void run(async () => {
                const st = await api.mediaSetKey(k);
                if (st.state.lastOk) toast.success("Chave salva no Keychain e verificada");
                else toast.error("Chave salva, mas o teste falhou", { description: st.state.lastError ?? undefined });
              });
            }}
          >
            <Input type="password" autoComplete="off" value={key} onChange={(e) => setKey(e.target.value)} placeholder={g?.keySet ? "Substituir chave…" : "Cole a chave da API (AI Studio)"} className="h-8" />
            <Button size="sm" disabled={busy || key.trim().length < 20}>
              <KeyRound /> Salvar
            </Button>
          </form>
          <p className="mt-1 text-[11px] text-subtle">A chave fica no Keychain do macOS e só é enviada ao Google, no cabeçalho da requisição.</p>
          {g?.keySet && (
            <div className="mt-2 flex gap-1.5">
              <Button size="sm" variant="ghost" disabled={busy} onClick={() => void run(() => api.mediaTest())}>
                {busy ? <Loader2 className="animate-spin" /> : <CheckCircle2 />} Testar
              </Button>
              <Button size="sm" variant="ghost" className="text-error" disabled={busy} onClick={() => setConfirmClear(true)}>
                <Trash2 /> Remover chave
              </Button>
            </div>
          )}
          {g?.state.lastOk && (
            <div className="mt-3 space-y-1.5">
              {(Object.keys(OP_LABEL) as MediaOp[])
                .filter((op) => op !== "transcribe")
                .map((op) => (
                  <div key={op} className="flex items-center gap-2">
                    <span className="w-28 shrink-0 text-[11.5px] text-muted">{OP_LABEL[op]}</span>
                    {g.candidatesByOp[op].length === 0 ? (
                      <span className="text-[11.5px] text-subtle">sem modelo na sua conta</span>
                    ) : (
                      <select
                        className={cn(select, "h-7 min-w-0 flex-1")}
                        value={g.modelsByOp[op] ?? ""}
                        onChange={(e) => void run(async () => onChange({ ...status!, gemini: await api.mediaSetModel(op, e.target.value || null) }))}
                      >
                        {g.candidatesByOp[op].map((m) => (
                          <option key={m} value={m}>
                            {m}
                          </option>
                        ))}
                      </select>
                    )}
                  </div>
                ))}
            </div>
          )}
        </div>
        <div className="space-y-1 border-t border-border pt-3 text-[12px]">
          <p className="flex items-center gap-1.5">
            {status?.claudeInstalled ? <CheckCircle2 className="size-3.5 text-success" /> : <CircleDashed className="size-3.5 text-subtle" />}
            Claude Code — análise de imagem {status?.claudeInstalled ? "" : "(não instalado)"}
          </p>
          <p className="flex items-center gap-1.5">
            {status?.macosSpeech === "authorized" ? <CheckCircle2 className="size-3.5 text-success" /> : <CircleDashed className="size-3.5 text-subtle" />}
            macOS — transcrição {status?.macosSpeech === "authorized" ? "" : "(autorize em Configurações → Voz)"}
          </p>
        </div>
      </div>
      <Dialog open={confirmClear} onOpenChange={setConfirmClear}>
        <DialogContent title="Remover a chave do Gemini?" description="A chave é apagada do Keychain. Itens já gerados continuam na biblioteca.">
          <div className="flex justify-end gap-2 px-5 py-4">
            <Button variant="ghost" onClick={() => setConfirmClear(false)}>
              Cancelar
            </Button>
            <Button
              variant="danger"
              onClick={() => {
                setConfirmClear(false);
                void run(() => api.mediaClearKey(), "Chave removida");
              }}
            >
              Remover
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </Card>
  );
}

function FilePicker({ kind, value, onChange, assets }: { kind: "image" | "audio"; value: { input: MediaInput; label: string } | null; onChange: (v: { input: MediaInput; label: string } | null) => void; assets: MediaAsset[] }) {
  const fromLibrary = assets.filter((a) => a.kind === kind && a.status === "done" && a.path);
  return (
    <div className="flex flex-wrap items-center gap-2">
      <Button
        size="sm"
        onClick={() =>
          void api
            .mediaPick(kind)
            .then((r) => r && onChange({ input: { type: "picked", token: r[0] }, label: r[1] }))
            .catch((e) => toast.error("Falha", { description: errorMessage(e) }))
        }
      >
        <Upload /> Escolher arquivo
      </Button>
      {fromLibrary.length > 0 && (
        <select
          className={select}
          value={value?.input.type === "asset" ? value.input.id : ""}
          onChange={(e) => {
            const a = fromLibrary.find((x) => x.id === e.target.value);
            onChange(a ? { input: { type: "asset", id: a.id }, label: `${ASSET_LABEL[a.op]} · ${a.prompt.slice(0, 40)}` } : null);
          }}
        >
          <option value="">…ou da biblioteca</option>
          {fromLibrary.map((a) => (
            <option key={a.id} value={a.id}>
              {ASSET_LABEL[a.op]} · {a.prompt.slice(0, 40) || relativeTime(a.createdAt)}
            </option>
          ))}
        </select>
      )}
      {value && <span className="truncate text-[12px] text-muted">{value.label}</span>}
    </div>
  );
}

function SaveDialog({ asset, onClose }: { asset: MediaAsset; onClose: () => void }) {
  const workspaces = useWorkspaces((s) => s.items);
  const [ws, setWs] = useState(asset.workspaceId ?? workspaces[0]?.id ?? "");
  const [dir, setDir] = useState("assets/media");
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent title="Salvar no workspace" description="Copia o arquivo para dentro do projeto (nunca sobrescreve: cria um nome novo se já existir).">
        <div className="space-y-3 px-5 py-4">
          <div className="space-y-1">
            <Label>Workspace</Label>
            <select className={cn(select, "w-full")} value={ws} onChange={(e) => setWs(e.target.value)}>
              {workspaces.map((w) => (
                <option key={w.id} value={w.id}>
                  {w.name}
                </option>
              ))}
            </select>
          </div>
          <div className="space-y-1">
            <Label>Pasta (relativa ao projeto)</Label>
            <Input value={dir} onChange={(e) => setDir(e.target.value)} />
          </div>
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={onClose}>
              Cancelar
            </Button>
            <Button
              variant="primary"
              disabled={!ws || !dir.trim()}
              onClick={() =>
                void api
                  .mediaSaveToWorkspace(asset.id, ws, dir)
                  .then((rel) => {
                    toast.success("Salvo no workspace", { description: rel });
                    onClose();
                  })
                  .catch((e) => toast.error("Não foi possível salvar", { description: errorMessage(e) }))
              }
            >
              <Save /> Salvar
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function AssetCard({ a, onChanged, onAnalyze }: { a: MediaAsset; onChanged: () => void; onAnalyze: (a: MediaAsset) => void }) {
  const [saving, setSaving] = useState(false);
  const [confirm, setConfirm] = useState(false);
  return (
    <Card className="flex flex-col gap-2 p-3">
      <div className="flex flex-wrap items-center gap-1.5 text-[11px]">
        <Badge tone={a.status === "failed" ? "error" : a.status === "running" ? "warning" : "neutral"}>{ASSET_LABEL[a.op] ?? a.op}</Badge>
        <span className="text-subtle">
          {a.provider}
          {a.model ? ` · ${a.model}` : ""}
        </span>
        <span className="ml-auto text-subtle">{relativeTime(a.createdAt)}</span>
      </div>
      {a.prompt && a.op !== "screenshot" && <p className="line-clamp-2 text-[12px] text-muted">“{a.prompt}”</p>}
      {a.op === "screenshot" && <p className="truncate font-mono text-[11px] text-subtle">{a.prompt}</p>}
      <AssetPreview asset={a} compact />
      <div className="mt-auto flex flex-wrap gap-1">
        {a.path && a.status === "done" && (
          <>
            <Button size="sm" variant="ghost" onClick={() => setSaving(true)}>
              <Save /> Workspace
            </Button>
            <Button size="sm" variant="ghost" onClick={() => void api.mediaReveal(a.id).catch(() => undefined)}>
              <FolderOpen /> Revelar
            </Button>
          </>
        )}
        {a.kind === "image" && a.status === "done" && (
          <Button size="sm" variant="ghost" onClick={() => onAnalyze(a)}>
            <ScanEye /> Analisar
          </Button>
        )}
        {a.kind === "text" && a.resultText && (
          <Button size="sm" variant="ghost" onClick={() => void navigator.clipboard.writeText(a.resultText!).then(() => toast.success("Copiado"))}>
            Copiar
          </Button>
        )}
        <Button size="sm" variant="ghost" className="ml-auto text-error" onClick={() => setConfirm(true)} aria-label="Excluir">
          <Trash2 />
        </Button>
      </div>
      {saving && <SaveDialog asset={a} onClose={() => setSaving(false)} />}
      <Dialog open={confirm} onOpenChange={setConfirm}>
        <DialogContent title="Excluir da biblioteca?" description="O arquivo gerado pelo FORGE é apagado. Cópias salvas em workspaces não são afetadas.">
          <div className="flex justify-end gap-2 px-5 py-4">
            <Button variant="ghost" onClick={() => setConfirm(false)}>
              Cancelar
            </Button>
            <Button
              variant="danger"
              onClick={() => {
                setConfirm(false);
                void api.mediaDelete(a.id).then(onChanged).catch((e) => toast.error("Falha", { description: errorMessage(e) }));
              }}
            >
              Excluir
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </Card>
  );
}

export function MediaView() {
  const workspaces = useWorkspaces((s) => s.items);
  const [tab, setTab] = useState<Tab>("image");
  const [status, setStatus] = useState<MediaStatus | null>(null);
  const [assets, setAssets] = useState<MediaAsset[]>([]);
  const [ws, setWs] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const [last, setLast] = useState<MediaAsset | null>(null);
  const [prompt, setPrompt] = useState("");
  const [file, setFile] = useState<{ input: MediaInput; label: string } | null>(null);
  const [analyzer, setAnalyzer] = useState<"claude" | "gemini">("claude");
  const [transcriber, setTranscriber] = useState<"macos" | "gemini">("macos");
  const [voice, setVoice] = useState("Kore");
  const locale = useSettings((s) => s.values["voice.locale"]) || "pt-BR";

  const load = useCallback(() => {
    void api.mediaList(null, 300).then(setAssets).catch(() => undefined);
  }, []);
  useEffect(() => {
    void api.mediaStatus().then(setStatus).catch((e) => toast.error("Falha ao carregar Mídia", { description: errorMessage(e) }));
    load();
    if (!useWorkspaces.getState().loaded) void useWorkspaces.getState().refresh().catch(() => undefined);
    const un = listen("forge://media-changed", () => load());
    return () => void un.then((f) => f());
  }, [load]);
  // Running video jobs: refresh while any is pending.
  useEffect(() => {
    if (!assets.some((a) => a.status === "running")) return;
    const t = setInterval(load, 8000);
    return () => clearInterval(t);
  }, [assets, load]);

  const g = status?.gemini;
  const geminiFor = (op: MediaOp) => !!g?.state.lastOk && !!g.modelsByOp[op];
  const why = (op: MediaOp) =>
    !g?.keySet ? "Defina a chave do Gemini ao lado." : !g.state.lastOk ? "Teste a conexão com o Gemini." : !g.modelsByOp[op] ? "Sua conta Gemini não tem modelo para isso." : null;

  const go = async (f: () => Promise<MediaAsset>) => {
    setBusy(true);
    setLast(null);
    try {
      const a = await f();
      setLast(a);
      load();
    } catch (e) {
      toast.error("Operação falhou", { description: errorMessage(e) });
      load();
    } finally {
      setBusy(false);
    }
  };
  const wsId = ws || null;

  const library = useMemo(() => assets, [assets]);

  return (
    <div className="h-full overflow-auto">
      <div className="mx-auto max-w-6xl px-8 py-8">
        <div className="mb-5 flex flex-wrap items-end gap-3">
          <div>
            <h1 className="text-xl font-semibold">Mídia</h1>
            <p className="mt-1 max-w-2xl text-xs leading-relaxed text-muted">
              Gere e analise imagens, áudio e vídeo com provedores reais. Sem provedor configurado, a operação fica indisponível — nada é simulado.
            </p>
          </div>
          <div className="ml-auto flex items-center gap-2 text-[12px] text-muted">
            Vincular ao workspace
            <select className={select} value={ws} onChange={(e) => setWs(e.target.value)}>
              <option value="">Nenhum</option>
              {workspaces.map((w) => (
                <option key={w.id} value={w.id}>
                  {w.name}
                </option>
              ))}
            </select>
          </div>
        </div>
        <div className="grid gap-4 lg:grid-cols-[1fr_320px]">
          <div className="min-w-0 space-y-4">
            <div className="flex flex-wrap gap-1 rounded-md border border-border bg-bg p-0.5">
              {TABS.map((t) => (
                <button
                  key={t.id}
                  onClick={() => {
                    setTab(t.id);
                    setLast(null);
                  }}
                  className={cn("flex items-center gap-1.5 rounded px-2.5 py-1 text-[12px]", tab === t.id ? "bg-elevated text-fg" : "text-muted hover:text-fg")}
                >
                  <t.icon className="size-3.5" /> {t.label}
                </button>
              ))}
            </div>

            {tab !== "library" && (
              <Card className="space-y-3 p-4">
                {tab === "image" && (
                  <>
                    <Textarea rows={4} value={prompt} onChange={(e) => setPrompt(e.target.value)} placeholder="Descreva a imagem…" />
                    <Unavailable msg={why("generate-image")} />
                    <Button variant="primary" disabled={busy || !prompt.trim() || !geminiFor("generate-image")} onClick={() => void go(() => api.mediaGenerateImage(wsId, prompt))}>
                      {busy ? <Loader2 className="animate-spin" /> : <ImagePlus />} Gerar com {g?.modelsByOp["generate-image"] ?? "Gemini"}
                    </Button>
                  </>
                )}
                {tab === "analyze" && (
                  <>
                    <FilePicker kind="image" value={file} onChange={setFile} assets={assets} />
                    <Textarea rows={3} value={prompt} onChange={(e) => setPrompt(e.target.value)} placeholder="O que você quer saber? (vazio = descrever a imagem)" />
                    <div className="flex flex-wrap items-center gap-2">
                      <select className={select} value={analyzer} onChange={(e) => setAnalyzer(e.target.value as "claude" | "gemini")}>
                        <option value="claude" disabled={!status?.claudeInstalled}>
                          Claude Code (instalado)
                        </option>
                        <option value="gemini" disabled={!geminiFor("analyze-image")}>
                          Gemini {g?.modelsByOp["analyze-image"] ?? "(indisponível)"}
                        </option>
                      </select>
                      <Button
                        variant="primary"
                        disabled={busy || !file || (analyzer === "gemini" ? !geminiFor("analyze-image") : !status?.claudeInstalled)}
                        onClick={() => file && void go(() => api.mediaAnalyzeImage(wsId, file.input, prompt, analyzer))}
                      >
                        {busy ? <Loader2 className="animate-spin" /> : <ScanEye />} Analisar
                      </Button>
                    </div>
                  </>
                )}
                {tab === "tts" && (
                  <>
                    <Textarea rows={4} value={prompt} onChange={(e) => setPrompt(e.target.value)} placeholder="Texto para narrar (até 5000 caracteres)…" maxLength={5000} />
                    <Unavailable msg={why("tts")} />
                    <div className="flex items-center gap-2">
                      <select className={select} value={voice} onChange={(e) => setVoice(e.target.value)} aria-label="Voz">
                        {VOICES.map((v) => (
                          <option key={v}>{v}</option>
                        ))}
                      </select>
                      <Button variant="primary" disabled={busy || !prompt.trim() || !geminiFor("tts")} onClick={() => void go(() => api.mediaTts(wsId, prompt, voice))}>
                        {busy ? <Loader2 className="animate-spin" /> : <AudioLines />} Gerar áudio
                      </Button>
                    </div>
                  </>
                )}
                {tab === "transcribe" && (
                  <>
                    <FilePicker kind="audio" value={file} onChange={setFile} assets={assets} />
                    <div className="flex flex-wrap items-center gap-2">
                      <select className={select} value={transcriber} onChange={(e) => setTranscriber(e.target.value as "macos" | "gemini")}>
                        <option value="macos" disabled={status?.macosSpeech !== "authorized"}>
                          macOS ({locale})
                        </option>
                        <option value="gemini" disabled={!geminiFor("transcribe")}>
                          Gemini {g?.modelsByOp.transcribe ?? "(indisponível)"}
                        </option>
                      </select>
                      <Button
                        variant="primary"
                        disabled={busy || !file || (transcriber === "gemini" ? !geminiFor("transcribe") : status?.macosSpeech !== "authorized")}
                        onClick={() => file && void go(() => api.mediaTranscribe(wsId, file.input, transcriber, locale))}
                      >
                        {busy ? <Loader2 className="animate-spin" /> : <FileAudio />} Transcrever
                      </Button>
                    </div>
                  </>
                )}
                {tab === "video" && (
                  <>
                    <Textarea rows={4} value={prompt} onChange={(e) => setPrompt(e.target.value)} placeholder="Descreva o vídeo…" />
                    <Unavailable msg={why("video")} />
                    <p className="text-[11.5px] text-subtle">A geração de vídeo roda em segundo plano (pode levar minutos) e aparece na Biblioteca quando terminar. Pode gerar custo na sua conta Google.</p>
                    <Button variant="primary" disabled={busy || !prompt.trim() || !geminiFor("video")} onClick={() => void go(() => api.mediaVideoStart(wsId, prompt))}>
                      {busy ? <Loader2 className="animate-spin" /> : <Clapperboard />} Gerar vídeo
                    </Button>
                  </>
                )}
                {last && (
                  <div className="border-t border-border pt-3">
                    <AssetPreview asset={last} />
                  </div>
                )}
              </Card>
            )}

            {tab === "library" &&
              (library.length === 0 ? (
                <Card>
                  <EmptyState icon={<FolderOpen />} title="Biblioteca vazia" description="Imagens, áudios, vídeos, análises, transcrições e capturas do navegador aparecem aqui." />
                </Card>
              ) : (
                <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
                  {library.map((a) => (
                    <AssetCard
                      key={a.id}
                      a={a}
                      onChanged={load}
                      onAnalyze={(x) => {
                        setFile({ input: { type: "asset", id: x.id }, label: `${ASSET_LABEL[x.op]} · ${x.prompt.slice(0, 40)}` });
                        setPrompt("");
                        setTab("analyze");
                      }}
                    />
                  ))}
                </div>
              ))}
          </div>
          <div className="space-y-4">
            <Providers status={status} onChange={setStatus} />
          </div>
        </div>
      </div>
    </div>
  );
}

function Unavailable({ msg }: { msg: string | null }) {
  if (!msg) return null;
  return (
    <p className="flex items-center gap-1.5 text-[11.5px] text-warning">
      <CircleAlert className="size-3.5" /> {msg}
    </p>
  );
}
