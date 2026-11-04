import { useCallback, useEffect, useState } from "react";
import { CheckCircle2, CircleAlert, CircleDashed, FlaskConical, Loader2, Mic, ShieldCheck } from "lucide-react";
import { toast } from "sonner";
import { Badge, Button, Card, Kbd, Switch } from "@/components/ui";
import { api, errorMessage, type PermissionState, type VoiceLocale, type VoiceSelfTest, type VoiceStatus } from "@/lib/ipc";
import { modKey } from "@/lib/utils";
import { useSettings } from "@/stores/settings";

const PERM: Record<PermissionState, { label: string; tone: "success" | "error" | "warning" | "neutral" }> = {
  authorized: { label: "autorizado", tone: "success" },
  denied: { label: "negado", tone: "error" },
  restricted: { label: "restrito", tone: "error" },
  notDetermined: { label: "ainda não solicitado", tone: "warning" },
  unknown: { label: "desconhecido", tone: "neutral" },
};

const PREFERRED = ["pt-BR", "pt-PT", "en-US", "en-GB", "es-ES", "es-MX"];

export function VoiceSettings() {
  const values = useSettings((s) => s.values);
  const setSetting = useSettings((s) => s.set);
  const [status, setStatus] = useState<VoiceStatus | null>(null);
  const [locales, setLocales] = useState<VoiceLocale[]>([]);
  const [authorizing, setAuthorizing] = useState(false);
  const [testing, setTesting] = useState(false);
  const [test, setTest] = useState<VoiceSelfTest | null>(null);
  const locale = values["voice.locale"] || "pt-BR";

  const refresh = useCallback(() => {
    api.voiceStatus().then(setStatus).catch(() => setStatus(null));
  }, []);
  useEffect(() => {
    refresh();
    api
      .voiceLocales()
      .then((l) => setLocales([...l].sort((a, b) => (PREFERRED.indexOf(a.id) + 1 || 99) - (PREFERRED.indexOf(b.id) + 1 || 99) || a.id.localeCompare(b.id))))
      .catch(() => setLocales([]));
  }, [refresh]);

  const save = (k: "voice.onDevice" | "voice.speak" | "voice.autoSend", v: boolean) =>
    void setSetting(k, v).catch((e) => toast.error("Falha ao salvar", { description: errorMessage(e) }));

  const authorize = async () => {
    setAuthorizing(true);
    try {
      setStatus(await api.voiceAuthorize());
    } catch (e) {
      toast.error("Falha ao solicitar permissões", { description: errorMessage(e) });
    } finally {
      setAuthorizing(false);
    }
  };

  const runTest = async () => {
    setTesting(true);
    setTest(null);
    try {
      setTest(await api.voiceSelfTest(locale, values["voice.onDevice"]));
    } catch (e) {
      toast.error("Autoteste falhou", { description: errorMessage(e) });
    } finally {
      setTesting(false);
    }
  };

  const current = locales.find((l) => l.id === locale);
  const ready = status?.speech === "authorized" && status?.microphone === "authorized";

  if (status && !status.supported) {
    return <p className="pt-3 text-xs text-muted">A voz do FORGE usa o reconhecimento de fala do macOS e ainda não está disponível neste sistema.</p>;
  }

  return (
    <div className="space-y-4 pt-3">
      <p className="text-xs leading-relaxed text-muted">
        <b className="text-fg">Jarvis</b> (<Kbd>{modKey}</Kbd>
        <Kbd>⇧</Kbd>
        <Kbd>J</Kbd>) recebe comandos como “Forge, abra o projeto X”, “crie uma missão para…”, “verifique os erros do frontend” e “pause todos os agentes” (este pede
        confirmação). <b className="text-fg">Ditado</b> (<Kbd>{modKey}</Kbd>
        <Kbd>⇧</Kbd>
        <Kbd>D</Kbd>) fala direto com o agente em foco. Usa o reconhecimento de fala do macOS; o áudio não é gravado pelo FORGE.
      </p>

      <Card className="px-4 py-3">
        <div className="flex items-center gap-2 text-[13px] font-medium">
          <ShieldCheck className="size-4 text-accent" /> Permissões do macOS
        </div>
        {!status ? (
          <p className="mt-2 text-xs text-subtle">Verificando…</p>
        ) : (
          <div className="mt-2 space-y-1.5 text-[12.5px]">
            <div className="flex items-center gap-2">
              <Mic className="size-3.5 text-muted" /> Microfone <Badge tone={PERM[status.microphone].tone}>{PERM[status.microphone].label}</Badge>
            </div>
            <div className="flex items-center gap-2">
              <CircleDashed className="size-3.5 text-muted" /> Reconhecimento de fala <Badge tone={PERM[status.speech].tone}>{PERM[status.speech].label}</Badge>
            </div>
            {!status.helper && <p className="text-error">Helper de voz não encontrado no app — reinstale o FORGE.</p>}
            {(status.speech === "denied" || status.microphone === "denied") && (
              <p className="text-[11.5px] text-warning">Negado anteriormente: libere o FORGE em Ajustes do Sistema → Privacidade e Segurança → Microfone / Reconhecimento de Fala.</p>
            )}
            <div className="flex gap-2 pt-1">
              {!ready && (
                <Button size="sm" variant="primary" disabled={authorizing || !status.helper} onClick={() => void authorize()}>
                  {authorizing ? <Loader2 className="animate-spin" /> : <ShieldCheck />} Solicitar permissões
                </Button>
              )}
              <Button size="sm" variant="ghost" onClick={refresh}>
                Atualizar
              </Button>
            </div>
          </div>
        )}
      </Card>

      <Card className="space-y-3 px-4 py-3">
        <div className="flex items-center justify-between gap-3">
          <div>
            <p className="text-[13px] font-medium">Idioma</p>
            <p className="text-[11.5px] text-muted">{current?.onDevice ? "Este idioma tem reconhecimento local nesta máquina." : "Este idioma usa o serviço da Apple (o áudio pode sair da máquina)."}</p>
          </div>
          <select
            value={locale}
            onChange={(e) => void setSetting("voice.locale", e.target.value)}
            className="h-8 rounded-md border border-border bg-bg px-2 text-[12.5px] text-fg outline-none"
            aria-label="Idioma do reconhecimento"
          >
            {(locales.length ? locales : [{ id: locale, onDevice: false }]).map((l) => (
              <option key={l.id} value={l.id}>
                {l.id}
                {l.onDevice ? " · local" : ""}
              </option>
            ))}
          </select>
        </div>
        <Row label="Somente no dispositivo" hint="Recusa transcrever se o idioma não tiver reconhecimento local (nada sai da máquina).">
          <Switch checked={values["voice.onDevice"]} onCheckedChange={(v) => save("voice.onDevice", v)} label="Somente no dispositivo" />
        </Row>
        <Row label="Respostas faladas" hint="O Jarvis confirma as ações em voz (say do macOS).">
          <Switch checked={values["voice.speak"]} onCheckedChange={(v) => save("voice.speak", v)} label="Respostas faladas" />
        </Row>
        <Row label="Enviar ditado automaticamente" hint="Ao terminar de falar, o pedido é enviado ao agente sem precisar confirmar.">
          <Switch checked={values["voice.autoSend"]} onCheckedChange={(v) => save("voice.autoSend", v)} label="Enviar automaticamente" />
        </Row>
      </Card>

      <Card className="px-4 py-3">
        <div className="flex items-center gap-2 text-[13px] font-medium">
          <FlaskConical className="size-4 text-accent" /> Autoteste
        </div>
        <p className="mt-1 text-[11.5px] leading-relaxed text-muted">Gera uma frase com a voz do macOS, transcreve com o reconhecimento configurado e compara. Não usa o microfone.</p>
        <Button size="sm" className="mt-2" disabled={testing || status?.speech !== "authorized"} onClick={() => void runTest()}>
          {testing ? <Loader2 className="animate-spin" /> : <FlaskConical />} Testar reconhecimento
        </Button>
        {status?.speech !== "authorized" && <p className="mt-1 text-[11px] text-subtle">Requer a permissão de reconhecimento de fala.</p>}
        {test && (
          <div className="mt-3 space-y-1 text-[12px]">
            <p className="flex items-center gap-1.5">
              {test.ok ? <CheckCircle2 className="size-3.5 text-success" /> : <CircleAlert className="size-3.5 text-error" />}
              {test.ok ? "Reconhecimento funcionando" : "Transcrição divergente"} · {Math.round(test.recall * 100)}% das palavras · {(test.elapsedMs / 1000).toFixed(1)}s
            </p>
            <p className="text-muted">Falado: “{test.phrase}”</p>
            <p className="text-muted">Ouvido: “{test.transcript}”</p>
          </div>
        )}
      </Card>
    </div>
  );
}

function Row({ label, hint, children }: { label: string; hint: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-3">
      <div>
        <p className="text-[13px] font-medium">{label}</p>
        <p className="text-[11.5px] text-muted">{hint}</p>
      </div>
      {children}
    </div>
  );
}
